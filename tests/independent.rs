//! The independent verifier (V4 8.1).
//!
//! `verify_log.py` shares no code with the crate: standard library only. This test
//! runs it against a real fixture and against a one-byte-altered copy, so the two
//! implementations cannot silently drift apart.
//!
//! Skipped when `python3` is not installed; CI runs on Ubuntu, where it is.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ledger::store::{self, Store};

fn fixture(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-independent-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");
    ledger::demo::seed(&store, false, true).expect("fixture");
    (store, dir)
}

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn verify(path: &std::path::Path) -> std::process::Output {
    Command::new("python3")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/verify_log.py"))
        .arg(path)
        .output()
        .expect("run verify_log.py")
}

#[test]
fn the_python_verifier_agrees_with_the_rust_chain() {
    if !python_available() {
        eprintln!("python3 not installed: skipping the independent verifier test");
        return;
    }
    let (store, dir) = fixture("agree");
    let log = store.path(store::SEND_LOG);

    let output = verify(&log);
    assert!(
        output.status.success(),
        "independent verifier rejected a valid log: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("147 entries verified"), "{stdout}");
    assert!(
        stdout.contains("d347a639fff01dadf90822d5f833b0b76ba5d7b3d8fec39d0a78491d7b6c897b"),
        "head hash must match the golden fixture: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_python_verifier_rejects_a_single_altered_byte() {
    if !python_available() {
        return;
    }
    let (store, dir) = fixture("tamper");
    let log = store.path(store::SEND_LOG);
    let mut bytes = fs::read(&log).expect("read");
    let at = bytes.len() / 2;
    if let Some(byte) = bytes.get_mut(at) {
        *byte = if *byte == b'a' { b'b' } else { b'a' };
    }
    fs::write(&log, &bytes).expect("write");

    let output = verify(&log);
    assert!(
        !output.status.success(),
        "the verifier must reject an altered log"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("FAIL"), "{combined}");

    let _ = fs::remove_dir_all(&dir);
}
