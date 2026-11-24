//! Crash-safety: interrupted dispatch, torn tails, and quarantine (V4 1.5).
//!
//! These tests kill a real `ledger dispatch` process mid-run rather than
//! simulating one, then restart it. The two branches are:
//!
//! * no tear (every append was fsynced whole) — restart continues and commits
//!   exactly the remaining sends, with no duplicate application;
//! * a torn tail — restart refuses (`exit 3`), and only an explicit
//!   `log quarantine` restores a log the tool will append to again.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ledger::store::{self, Store};

const REQUISITION: &str = "REQ-2026-014";

fn workspace(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-crash-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");
    ledger::demo::seed(&store, false, false).expect("fixture");
    (store, dir)
}

fn ledger() -> &'static str {
    env!("CARGO_BIN_EXE_ledger")
}

fn dispatch(store: &Store, abort_after: Option<usize>) -> std::process::Output {
    let mut command = Command::new(ledger());
    command.arg("--data").arg(store.root()).args([
        "dispatch",
        "--requisition",
        REQUISITION,
        "--allow-blocked",
    ]);
    if let Some(limit) = abort_after {
        command.env("LEDGER_ABORT_AFTER_SENDS", limit.to_string());
    }
    command.output().expect("run ledger")
}

/// The application ids in the send log, in order.
fn sent_applications(store: &Store) -> Vec<String> {
    let path = store.path(store::SEND_LOG);
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|value| {
            value
                .get("application_id")
                .and_then(|id| id.as_str())
                .map(str::to_owned)
        })
        .collect()
}

fn verify(store: &Store) -> std::process::Output {
    Command::new(ledger())
        .arg("--data")
        .arg(store.root())
        .args(["log", "verify"])
        .output()
        .expect("run ledger")
}

#[test]
fn a_killed_dispatch_resumes_without_duplicates() {
    let (store, dir) = workspace("resume");

    let killed = dispatch(&store, Some(70));
    assert!(
        !killed.status.success(),
        "the child must die mid-dispatch, got {:?}",
        killed.status
    );
    let committed = sent_applications(&store);
    assert_eq!(
        committed.len(),
        70,
        "each send was fsynced before the abort"
    );
    assert!(verify(&store).status.success(), "the chain still verifies");

    let restarted = dispatch(&store, None);
    assert!(
        restarted.status.success(),
        "restart failed: {}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    let all = sent_applications(&store);
    assert_eq!(all.len(), 147, "70 + 77 sends, exactly");
    assert_eq!(
        all.len() - committed.len(),
        77,
        "exactly 77 new sends after the restart"
    );

    let unique: BTreeSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "no application was sent twice");
    assert_eq!(committed.as_slice(), &all[..70], "the prefix is unchanged");
    assert!(verify(&store).status.success());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_torn_tail_blocks_restart_until_it_is_quarantined() {
    let (store, dir) = workspace("torn");

    let killed = dispatch(&store, Some(70));
    assert!(!killed.status.success());
    let before = sent_applications(&store);
    assert_eq!(before.len(), 70);

    // Simulate a writer that died between the write and the newline.
    let path = store.path(store::SEND_LOG);
    let tail = b"{\"seq\":70,\"prev_hash\":\"dead";
    let mut bytes = fs::read(&path).expect("read log");
    bytes.extend_from_slice(tail);
    fs::write(&path, &bytes).expect("write torn tail");

    // Branch 1: no quarantine → refuse, commit nothing.
    let refused = dispatch(&store, None);
    assert_eq!(
        refused.status.code(),
        Some(3),
        "a torn tail is invalid state: got {:?} / {}",
        refused.status,
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        sent_applications(&store).len(),
        70,
        "nothing may be sent while the log is torn"
    );

    // Branch 2: quarantine deliberately, with a reason on the record.
    let quarantine = Command::new(ledger())
        .arg("--data")
        .arg(store.root())
        .args([
            "log",
            "quarantine",
            "--reason",
            "killed between write and newline",
            "--operator",
            "qa-patel",
            "--yes",
        ])
        .output()
        .expect("run ledger");
    assert!(
        quarantine.status.success(),
        "quarantine failed: {}",
        String::from_utf8_lossy(&quarantine.stderr)
    );
    assert_eq!(
        fs::read(&path).expect("log").len(),
        bytes.len() - tail.len(),
        "the torn tail is gone from the log"
    );
    let sidecars: Vec<PathBuf> = fs::read_dir(store.root())
        .expect("read dir")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.to_string_lossy().ends_with(".quarantine"))
        .collect();
    assert_eq!(sidecars.len(), 1, "the tail is preserved, not deleted");
    assert_eq!(fs::read(&sidecars[0]).expect("sidecar"), tail);

    let operator_log = fs::read_to_string(store.path(store::OPERATOR)).expect("operator log");
    assert!(operator_log.contains("qa-patel"));
    assert!(operator_log.contains("killed between write and newline"));

    // Branch 2 continues: the remaining 77 sends commit.
    let restarted = dispatch(&store, None);
    assert!(
        restarted.status.success(),
        "restart after quarantine failed: {}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    let all = sent_applications(&store);
    assert_eq!(all.len(), 147);
    let unique: BTreeSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), all.len());
    assert!(verify(&store).status.success());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_second_operator_cannot_hold_the_workspace() {
    let (store, dir) = workspace("lock");
    let held = store.lock().expect("first lock");

    let blocked = Command::new(ledger())
        .arg("--data")
        .arg(store.root())
        .args(["governance", "set", "--sla-hours", "24"])
        .output()
        .expect("run ledger");
    assert_eq!(
        blocked.status.code(),
        Some(4),
        "lock contention is exit 4: {}",
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("locked by"));

    drop(held);
    // Read-only commands never need the lock.
    let status = Command::new(ledger())
        .arg("--data")
        .arg(store.root())
        .args(["status"])
        .output()
        .expect("run ledger");
    assert!(status.status.success());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_atomic_write_leaves_no_partial_json() {
    let (store, dir) = workspace("atomic");
    let before = fs::read_to_string(store.path(store::POLICY)).expect("policy");
    let policy = ledger::policy::Policy::load(&store).expect("load");
    ledger::policy::Policy::save(&policy, &store).expect("save");
    let after = fs::read_to_string(store.path(store::POLICY)).expect("policy");
    assert_eq!(before, after);
    assert!(
        !Path::new(&store.path("policy.json.tmp")).exists(),
        "the temp file is renamed away"
    );
    let _ = fs::remove_dir_all(&dir);
}
