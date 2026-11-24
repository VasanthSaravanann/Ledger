//! RBAC and signature verification through the real CLI (V5 Phase 3).
//!
//! These drive the binary, because the guarantee is about what a role may *run*,
//! not about what a library function does when called directly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ledger::store::{self, Store};

const REQUISITION: &str = "REQ-2026-014";

fn fixture(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-rbac-{tag}-{}-{}",
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

fn ledger(store: &Store, operator: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--data")
        .arg(store.root())
        .arg("--operator")
        .arg(operator)
        .args(args)
        .output()
        .expect("run ledger")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn roles_gate_the_commands_they_should() {
    let (store, dir) = fixture("roles");

    let auditor = ledger(
        &store,
        "aud-bell",
        &["dispatch", "--requisition", REQUISITION, "--preview"],
    );
    assert_eq!(auditor.status.code(), Some(3), "{}", stderr(&auditor));
    assert!(stderr(&auditor).contains("auditor"));

    let coordinator = ledger(
        &store,
        "rec-rui",
        &["gate", "approve", "--actor", "J", "--role", "legal"],
    );
    assert_eq!(
        coordinator.status.code(),
        Some(3),
        "{}",
        stderr(&coordinator)
    );
    assert!(stderr(&coordinator).contains("coordinator"));

    let approver = ledger(&store, "j-chen", &["governance", "show"]);
    assert!(approver.status.success(), "{}", stderr(&approver));

    // The auditor keeps read access.
    let read = ledger(&store, "aud-bell", &["verify-signatures"]);
    assert!(read.status.success(), "{}", stderr(&read));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_disposition_is_signed_by_the_acting_operator() {
    let (store, dir) = fixture("sign");

    let created = ledger(
        &store,
        "rec-rui",
        &[
            "disposition",
            "record",
            "--requisition",
            REQUISITION,
            "--application",
            "APP-1150",
            "--first-name",
            "Mary",
            "--email",
            "mary@example.com",
            "--recruiter",
            "rec-rui",
            "--decision-maker",
            "dm-chen",
            "--category",
            "FIELD",
        ],
    );
    assert!(created.status.success(), "{}", stderr(&created));
    let stdout = String::from_utf8_lossy(&created.stdout);
    assert!(stdout.contains("signed by rec-rui"), "{stdout}");

    let verified = ledger(&store, "aud-bell", &["verify-signatures"]);
    assert!(verified.status.success(), "{}", stderr(&verified));
    assert!(String::from_utf8_lossy(&verified.stdout).contains("unsigned records: 0"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_tampered_signature_is_detected() {
    let (store, dir) = fixture("tamper");
    let text = fs::read_to_string(store.path(store::DISPOSITIONS)).expect("log");
    let broken = text.replacen("\"signature\":\"", "\"signature\":\"00", 1);
    assert_ne!(text, broken, "the fixture carries a signature");
    fs::write(store.path(store::DISPOSITIONS), broken).expect("write");

    let verified = ledger(&store, "aud-bell", &["verify-signatures"]);
    assert_eq!(verified.status.code(), Some(3), "{}", stderr(&verified));
    assert!(String::from_utf8_lossy(&verified.stdout).contains("INVALID"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bootstrapping_needs_no_identity_but_everything_else_does() {
    let dir = std::env::temp_dir().join(format!("ledger-rbac-bootstrap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");

    // No registry: `init` is allowed, `import` is not.
    let init = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--data")
        .arg(store.root())
        .arg("init")
        .output()
        .expect("init");
    assert!(init.status.success(), "{}", stderr(&init));

    let blocked = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--data")
        .arg(store.root())
        .args([
            "ats",
            "import",
            "--requisition",
            REQUISITION,
            "--file",
            "/dev/null",
        ])
        .output()
        .expect("import");
    assert_eq!(blocked.status.code(), Some(3));
    assert!(stderr(&blocked).contains("register an operator"));

    // Registering the first operator is itself allowed.
    let registered = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--data")
        .arg(store.root())
        .args([
            "operator",
            "register",
            "--name",
            "qa-patel",
            "--role",
            "coordinator",
        ])
        .output()
        .expect("register");
    assert!(registered.status.success(), "{}", stderr(&registered));

    let listed = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--data")
        .arg(store.root())
        .args(["operator", "list"])
        .output()
        .expect("list");
    assert!(String::from_utf8_lossy(&listed.stdout).contains("qa-patel"));

    let keyfile = Path::new(&store.path("operators")).join("qa-patel.key");
    assert!(keyfile.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&keyfile)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "private key is owner-only");
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn editing_approved_copy_blocks_dispatch_until_reapproved() {
    let (store, dir) = fixture("drift");

    // Contract: the fixture is ready, and every sign-off verifies.
    let ready = ledger(
        &store,
        "qa-patel",
        &["dispatch", "--requisition", REQUISITION, "--preview"],
    );
    assert!(ready.status.success(), "{}", stderr(&ready));

    // The approver edits one block, which is a new copy.
    let bumped = ledger(
        &store,
        "j-chen",
        &[
            "gate",
            "bump-version",
            "--gate",
            "GATE-04",
            "--version",
            "v1.1",
        ],
    );
    assert!(bumped.status.success(), "{}", stderr(&bumped));

    // Signatures are still cryptographically valid, but no longer cover the copy.
    let verified = ledger(&store, "aud-bell", &["verify-signatures"]);
    assert_eq!(verified.status.code(), Some(3), "{}", stderr(&verified));
    let stdout = String::from_utf8_lossy(&verified.stdout);
    assert!(stdout.contains("STALE"), "{stdout}");
    assert!(
        stdout.contains("signed records:"),
        "the human summary reports the verified count: {stdout}"
    );

    // Dispatch refuses, and says which role signed the previous copy.
    let blocked = ledger(
        &store,
        "qa-patel",
        &["dispatch", "--requisition", REQUISITION, "--preview"],
    );
    assert_eq!(blocked.status.code(), Some(3));
    assert!(stderr(&blocked).contains("copy changed after hiring-manager signed"));

    // Re-approving the new copy restores dispatch.
    for (actor, role, operator) in [
        ("J. Chen", "hiring-manager", "j-chen"),
        ("M. Okafor", "legal", "m-okafor"),
    ] {
        let approved = ledger(
            &store,
            operator,
            &["gate", "approve", "--actor", actor, "--role", role],
        );
        assert!(approved.status.success(), "{}", stderr(&approved));
    }
    let reapproved = ledger(
        &store,
        "qa-patel",
        &["dispatch", "--requisition", REQUISITION, "--preview"],
    );
    assert!(reapproved.status.success(), "{}", stderr(&reapproved));
    assert!(
        ledger(&store, "aud-bell", &["verify-signatures"])
            .status
            .success()
    );

    let _ = fs::remove_dir_all(&dir);
}
