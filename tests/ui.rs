//! HTTP tests for `ledger ui` (V5 Phase 4).
//!
//! These speak raw HTTP over a loopback socket so the test suite gains no HTTP
//! client dependency, which is the same reason the server has none.

#![cfg(feature = "ui")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::ChildStdout;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ledger::store::Store;

struct Server {
    child: Child,
    /// Kept open on purpose: closing the pipe after the banner makes the server's
    /// next `println!` fail with EPIPE, which panics the server thread and resets
    /// the client connection. macOS tolerates the write; Linux does not.
    _stdout: BufReader<ChildStdout>,
    port: u16,
    token: String,
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-ui-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");
    ledger::demo::seed(&store, false, false).expect("fixture");
    (store, dir)
}

fn start(tag: &str, operator: &str, port: u16) -> Server {
    let (store, dir) = fixture(tag);
    let mut child = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--data")
        .arg(store.root())
        .args(["ui", "--operator", operator])
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ledger ui");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut banner = String::new();
    stdout.read_line(&mut banner).expect("read banner");
    let token = banner
        .split("token=")
        .nth(1)
        .expect("token in banner")
        .trim()
        .to_owned();

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    Server {
        child,
        _stdout: stdout,
        port,
        token,
        dir,
    }
}

/// A raw HTTP/1.1 request. Returns status code and body.
fn http(
    port: u16,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some(cookie) = cookie {
        request.push_str(&format!("Cookie: {cookie}\r\n"));
    }
    if let Some(body) = body {
        request.push_str("Content-Type: application/json\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes()).expect("write");
    stream.flush().expect("flush");

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let body = response
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    (status, body)
}

/// Exchanges the launch token for a session cookie.
fn login(port: u16, token: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let request =
        format!("GET /?token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .and_then(|line| line.split(';').next())
        .and_then(|pair| pair.split(' ').nth(1))
        .expect("session cookie")
        .to_owned()
}

#[test]
fn the_session_is_required_and_bound_to_one_operator() {
    let server = start("auth", "aud-bell", 8411);

    let (status, body) = http(server.port, "GET", "/", None, None);
    assert_eq!(status, 401, "{body}");

    let cookie = login(server.port, &server.token);
    let (status, body) = http(server.port, "GET", "/", Some(&cookie), None);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Triage"), "the page renders");

    // The token is single-use.
    let (status, _) = http(
        server.port,
        "GET",
        &format!("/?token={}", server.token),
        None,
        None,
    );
    assert_eq!(status, 401);
}

#[test]
fn an_auditor_can_read_but_not_dispose() {
    let server = start("rbac", "aud-bell", 8412);
    let cookie = login(server.port, &server.token);

    let (status, _) = http(server.port, "GET", "/audit", Some(&cookie), None);
    assert_eq!(status, 200, "auditors read the audit trail");

    let (status, body) = http(
        server.port,
        "POST",
        "/api/dispose/batch",
        Some(&cookie),
        Some(r#"[{"application_id":"APP-1150","disposition":"FIELD"}]"#),
    );
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("auditor"), "{body}");
}

#[test]
fn a_coordinator_disposes_atomically_and_the_records_verify() {
    let server = start("dispose", "rec-rui", 8413);
    let cookie = login(server.port, &server.token);
    let store = Store::open(&server.dir).expect("workspace");

    // Preview is a read: no lock, no write.
    let (status, body) = http(
        server.port,
        "POST",
        "/api/preview",
        Some(&cookie),
        Some(r#"{"application_id":"APP-1150","disposition":"FIELD"}"#),
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Update on your application"), "{body}");

    // One new disposition and one correction, in a single batch.
    let (status, body) = http(
        server.port,
        "POST",
        "/api/dispose/batch",
        Some(&cookie),
        Some(
            r#"[{"application_id":"APP-1150","disposition":"FIELD"},
                {"application_id":"APP-1149","disposition":"ADMIN",
                 "reason":"role-rescoped","supersedes":"DSP-0149"}]"#,
        ),
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("DSP-0150"), "{body}");
    assert!(body.contains("DSP-0151"), "{body}");

    let report = ledger::verify::verify_workspace(&store).expect("verify");
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.dispositions_unsigned, 0);

    // The chain is untouched: the batch wrote dispositions, not sends.
    let log = ledger::log::SendLog::open(&store).expect("log");
    assert_eq!(log.len(), 0);
}

#[test]
fn a_rejected_batch_writes_nothing() {
    let server = start("atomic", "rec-rui", 8414);
    let cookie = login(server.port, &server.token);
    let store = Store::open(&server.dir).expect("workspace");
    let before = std::fs::read_to_string(store.path(ledger::store::DISPOSITIONS)).expect("read");

    let (status, body) = http(
        server.port,
        "POST",
        "/api/dispose/batch",
        Some(&cookie),
        Some(
            r#"[{"application_id":"APP-1150","disposition":"FIELD"},
                {"application_id":"APP-1149","disposition":"FIELD"}]"#,
        ),
    );
    assert_eq!(status, 400, "APP-1149 already has a disposition: {body}");

    let after = std::fs::read_to_string(store.path(ledger::store::DISPOSITIONS)).expect("read");
    assert_eq!(before, after, "a rejected batch must not write anything");
}

#[test]
fn a_lock_held_elsewhere_is_a_conflict() {
    let server = start("lock", "rec-rui", 8415);
    let cookie = login(server.port, &server.token);
    let store = Store::open(&server.dir).expect("workspace");
    let held = store.lock().expect("take the workspace lock");

    let (status, body) = http(
        server.port,
        "POST",
        "/api/dispose/batch",
        Some(&cookie),
        Some(r#"[{"application_id":"APP-1150","disposition":"FIELD"}]"#),
    );
    assert_eq!(status, 409, "{body}");
    assert!(body.contains("locked by"), "{body}");

    drop(held);
    let (status, body) = http(
        server.port,
        "POST",
        "/api/dispose/batch",
        Some(&cookie),
        Some(r#"[{"application_id":"APP-1150","disposition":"FIELD"}]"#),
    );
    assert_eq!(status, 200, "{body}");
}
