//! Golden fixtures for the on-disk format (V4 4.3, 4.4).
//!
//! The fixture is deterministic, so these hashes are a compatibility contract:
//! a `serde` rename, a field-order change that leaks into the digest, a canonical
//! encoder regression, or a fixture change all fail here before they can silently
//! invalidate a real deployment's log.
//!
//! When a change is intentional, regenerate the fixture with `ledger demo seed`,
//! update the constants below in the same commit, together with `FORMAT_SPEC.md`.

use std::fs;
use std::path::PathBuf;

use ledger::digest::sha256_hex;
use ledger::store::{self, Store};

/// The fixture's chain head, which every page and the verifier quote.
const LOG_HEAD: &str = "d347a639fff01dadf90822d5f833b0b76ba5d7b3d8fec39d0a78491d7b6c897b";

/// SHA-256 of `send_log.jsonl` byte-for-byte.
const SEND_LOG_SHA256: &str = "d9acaeb0df07c6e00b8a219e798556b553a079519be2a54996bcc52ef1ffc56d";

/// SHA-256 of `dispositions.jsonl` byte-for-byte.
const DISPOSITIONS_SHA256: &str =
    "147f6b2b00f026f1ed873a31011e3a5b86303cd556d0e6e4bd49ca38e10b5888";

/// SHA-256 over every workspace file except `demo-summary.json`.
const FINGERPRINT: &str = "64f9e1f9e8b4b9ce30c38f2f7c82c904906e5c4c2d9d23caf5eca6df73833ca2";

const SENDS: usize = 147;
const BLOCKED: usize = 3;
const STALE: usize = 1;

fn seeded(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-golden-{tag}-{}-{}",
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

fn sha_of(store: &Store, name: &str) -> String {
    sha256_hex(&fs::read(store.path(name)).expect("read artefact"))
}

#[test]
fn the_fixture_matches_the_committed_golden_hashes() {
    let (store, dir) = seeded("hashes");
    let summary = ledger::demo::summary(&store).expect("summary");

    assert_eq!(summary.log_head, LOG_HEAD, "chain head moved");
    assert_eq!(summary.fingerprint, FINGERPRINT, "workspace bytes moved");
    assert_eq!(
        sha_of(&store, store::SEND_LOG),
        SEND_LOG_SHA256,
        "send log bytes moved"
    );
    assert_eq!(
        sha_of(&store, store::DISPOSITIONS),
        DISPOSITIONS_SHA256,
        "dispositions bytes moved"
    );

    let verification = ledger::log::SendLog::open(&store)
        .expect("log")
        .verification()
        .expect("verification");
    assert_eq!(verification.entries, SENDS);
    assert_eq!(verification.head, LOG_HEAD);
    assert_eq!(summary.blocked, BLOCKED);
    assert_eq!(summary.stale_versions, STALE);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reordering_json_keys_still_verifies_because_layout_is_not_digest_covered() {
    // FORMAT_SPEC.md §4: the digest covers values, not layout. Reordering a stored
    // line's keys must therefore verify; editing a covered value must not.
    let (store, dir) = seeded("reorder");
    let path = store.path(store::SEND_LOG);
    let text = fs::read_to_string(&path).expect("read log");

    let mut reordered = String::new();
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("json");
        let mut keys: Vec<&String> = value.as_object().expect("object").keys().collect();
        keys.sort();
        keys.reverse();
        let mut object = serde_json::Map::new();
        for key in keys {
            object.insert(key.clone(), value[key].clone());
        }
        reordered.push_str(&serde_json::to_string(&object).expect("serialize"));
        reordered.push('\n');
    }
    assert_ne!(reordered, text, "the file really changed");
    fs::write(&path, &reordered).expect("write reordered log");
    assert!(
        ledger::log::SendLog::open(&store).is_ok(),
        "key order is layout, not digest input"
    );

    // Now edit one covered value: the chain must break at that entry.
    let broken = reordered.replace("\"candidate_first_name\":", "\"candidate_first_name\":\"X");
    fs::write(&path, broken).expect("write edited log");
    assert!(
        ledger::log::SendLog::open(&store).is_err(),
        "a covered-value edit must fail verification"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_entry_with_an_unknown_schema_version_fails_closed() {
    let (store, dir) = seeded("schema");
    let path = store.path(store::SEND_LOG);
    let text = fs::read_to_string(&path).expect("read log");
    let bumped = text.replacen("\"schema_version\":1", "\"schema_version\":99", 1);
    assert_ne!(text, bumped);
    fs::write(&path, bumped).expect("write log");

    match ledger::log::SendLog::open(&store) {
        Err(ledger::error::Error::LogCorrupt { reason, .. }) => {
            assert!(reason.contains("unknown schema_version"), "{reason}");
        }
        other => panic!("expected a schema failure, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_chain_head_and_verifier_agree_after_a_restore() {
    let (store, dir) = seeded("restore");
    let backup = dir.join("backup");
    fs::create_dir_all(&backup).expect("backup dir");
    for entry in fs::read_dir(store.root()).expect("read dir") {
        let entry = entry.expect("entry");
        if entry.path().is_file() {
            fs::copy(entry.path(), backup.join(entry.file_name())).expect("copy");
        }
    }

    let restored = Store::open(dir.join("restored")).expect("restored workspace");
    for entry in fs::read_dir(&backup).expect("read backup") {
        let entry = entry.expect("entry");
        fs::copy(
            entry.path(),
            restored.path(&entry.file_name().to_string_lossy()),
        )
        .expect("restore");
    }
    let verification = ledger::log::SendLog::open(&restored)
        .expect("restored log")
        .verification()
        .expect("verifies after restore");
    assert_eq!(verification.entries, SENDS);
    assert_eq!(verification.head, LOG_HEAD);

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------
// Delivery confirmation (P0-1)
//
// A queued draft becomes `confirmed` only by appending a DELIVERY_CONFIRMED entry
// to its own chain. The send log stays byte-for-byte identical, so every head this
// repository has published remains valid.

/// The first confirmation's `entry_hash` in `queued_fixture`.
const DELIVERY_HEAD: &str = "777907a6079faa1e836943137ffc2b6943184c85d95ca745bfa2df268f08b986";

/// The queued send log's bytes, before and after its confirmation.
const QUEUED_SEND_LOG_SHA256: &str =
    "626b75e9662c441bb49cbb18df74de713db4844cffd35fd2c4561010182a57d5";

fn at(minute: u32) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 11, 24, 9, minute, 0)
        .single()
        .expect("timestamp")
}

fn queued_fixture(tag: &str) -> (Store, PathBuf) {
    use ledger::disposition::{Disposition, DispositionRecord, NewDisposition};
    use ledger::gate::{GateAnswer, GateKind};
    use ledger::id::{ApplicationId, CandidateEmail, DispositionId};
    use ledger::log::{DeliveryStatus, SendDraft, SendLog};
    use ledger::requisition::{Requisition, RequisitionDraft, Sender};
    use ledger::template::RenderedEmail;
    use std::collections::BTreeMap;

    let dir = std::env::temp_dir().join(format!(
        "ledger-golden-delivery-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");

    let requisition = Requisition::new(
        RequisitionDraft {
            id: ledger::id::RequisitionId::try_from("REQ-1").expect("id"),
            employer: "Example Health".to_owned(),
            title: "Registered Nurse".to_owned(),
            reply_to: ledger::id::Mailbox::try_from("careers-feedback@example.com.au")
                .expect("mailbox"),
            sender: Sender::try_new("R. Patel", "Coordinator").expect("sender"),
            gates: vec![GateKind::RightToWork],
            facts: BTreeMap::new(),
        },
        at(0),
    )
    .expect("requisition");
    let form = NewDisposition::try_new(
        &requisition,
        &ledger::gate::GateLibrary::seed(),
        ledger::disposition::FormSubmission {
            application_id: ApplicationId::try_from("APP-1").expect("application"),
            candidate_first_name: "Ada".to_owned(),
            candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
            recruiter: ledger::id::RecruiterId::try_from("rec-rui").expect("recruiter"),
            decision_maker: ledger::id::DecisionMakerId::try_from("dm-chen").expect("dm"),
            disposition: Disposition::Gate {
                gate: GateKind::RightToWork,
                answer: GateAnswer::NotHeld,
            },
            supersedes: None,
        },
        at(1),
    )
    .expect("form");
    let disposition = DispositionRecord {
        id: DispositionId::try_from("DSP-0001").expect("id"),
        locked: true,
        form,
    };

    let mut log = SendLog::open(&store).expect("send log");
    log.append(
        SendDraft {
            disposition,
            text_version: ledger::id::TextVersion::parse("v1.0").expect("version"),
            rendered: RenderedEmail {
                subject: "Update on your application".to_owned(),
                body: "Dear Ada,\n\nYou indicated ...".to_owned(),
            },
            delivery_status: DeliveryStatus::Queued,
        },
        at(2),
    )
    .expect("queued send");
    (store, dir)
}

#[test]
fn a_confirmation_moves_a_queued_send_to_confirmed_without_touching_the_send_log() {
    use ledger::delivery::DeliveryLog;
    use ledger::id::SendId;
    use ledger::log::SendLog;
    use ledger::metrics::Metrics;

    let (store, dir) = queued_fixture("queued");
    let send_log = SendLog::open(&store).expect("send log");
    let sends = send_log.entries().expect("entries");
    assert_eq!(
        sha_of(&store, store::SEND_LOG),
        QUEUED_SEND_LOG_SHA256,
        "the queued send's bytes moved"
    );

    let before = Metrics::compute(&sends, &[], &[], &[], &[]);
    assert_eq!(before.sends, 1);
    assert_eq!((before.confirmed, before.queued), (0, 1));

    let mut deliveries = DeliveryLog::open(&store).expect("delivery log");
    assert!(deliveries.is_empty(), "nothing is confirmed yet");
    let send_id = SendId::try_from("SND-000001").expect("send id");
    let entry = deliveries
        .confirm(&send_log, &send_id, "M. Okafor", &"ab".repeat(32), at(3))
        .expect("confirm");
    assert_eq!(entry.entry_hash, DELIVERY_HEAD, "delivery chain head moved");
    assert_eq!(
        sha_of(&store, store::SEND_LOG),
        QUEUED_SEND_LOG_SHA256,
        "confirming changed a send-log byte"
    );

    let confirmations = deliveries.entries().expect("confirmations");
    let after = Metrics::compute(&sends, &[], &[], &[], &confirmations);
    assert_eq!((after.confirmed, after.queued), (1, 0));
    assert_eq!(
        DeliveryLog::open(&store)
            .expect("reopen")
            .verification()
            .expect("verifies")
            .head,
        DELIVERY_HEAD
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_demo_fixture_reports_its_recorded_sends_as_confirmed() {
    use ledger::delivery::DeliveryLog;
    use ledger::log::SendLog;
    use ledger::metrics::Metrics;

    let (store, dir) = seeded("delivery-metrics");
    let sends = SendLog::open(&store)
        .expect("send log")
        .entries()
        .expect("entries");
    let metrics = Metrics::compute(&sends, &[], &[], &[], &[]);
    assert_eq!(metrics.sends, SENDS as u64);
    assert_eq!((metrics.confirmed, metrics.queued), (SENDS as u64, 0));
    assert!(
        DeliveryLog::open(&store).expect("delivery log").is_empty(),
        "the fixture keeps the head it published: no confirmation was needed"
    );

    let _ = fs::remove_dir_all(&dir);
}
