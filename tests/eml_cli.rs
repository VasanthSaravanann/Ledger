//! The `.eml` dispatch path (V5 Phase 4.5).
//!
//! The point of this test is the honesty of the record: drafts are written *and*
//! logged as `queued`, never as `sent`, and the manifest pins each draft's bytes.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ledger::log::SendLog;
use ledger::store::Store;

fn fixture(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-eml-cli-{tag}-{}-{}",
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

#[test]
fn a_drafted_batch_is_logged_as_queued_and_its_bytes_are_pinned() {
    let (store, dir) = fixture("batch");
    let batch_dir = dir.join("batch");

    let output = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .args(["--data", store.root().to_str().expect("path")])
        .args(["--operator", "qa-patel"])
        .args(["dispatch", "--requisition", "REQ-2026-014"])
        .arg("--eml")
        .arg(&batch_dir)
        .arg("--allow-blocked")
        .output()
        .expect("dispatch");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = ledger::eml::read_manifest(&batch_dir).expect("manifest");
    assert_eq!(manifest.files.len(), 147);
    assert_eq!(ledger::eml::verify_batch(&batch_dir).expect("verify"), 147);

    // Every entry is queued, not sent: a draft is not a delivery.
    let entries = SendLog::open(&store)
        .expect("log")
        .entries()
        .expect("entries");
    assert_eq!(entries.len(), 147);
    assert!(
        entries
            .iter()
            .all(|entry| entry.delivery_status == ledger::log::DeliveryStatus::Queued),
        "a drafted batch must be queued"
    );

    // The manifest hash is the file's hash.
    let first = &manifest.files[0];
    let bytes = fs::read(batch_dir.join(&first.file)).expect("draft");
    let expected = ledger::digest::sha256_hex(&bytes);
    assert_eq!(first.sha256, expected);

    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("Reply-To: careers-feedback@example.com.au"));
    assert!(text.contains("From: R. Patel, Recruitment Coordinator"));
    assert!(text.contains("X-Ledger-Send-Id: SND-000001"));

    // Editing a draft after the fact is caught.
    let mut tampered = bytes.clone();
    tampered.extend_from_slice(b" ");
    fs::write(batch_dir.join(&first.file), tampered).expect("write");
    assert!(ledger::eml::verify_batch(&batch_dir).is_err());

    let _ = fs::remove_dir_all(&dir);
}
