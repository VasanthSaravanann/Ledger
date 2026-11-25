//! State-document versioning (P1-6).
//!
//! A workspace from an older build used to be indistinguishable from a corrupt one.
//! Every state document now carries `schema_version`; a newer version fails closed in
//! `Store::load` and `migrate`, and an older one is upgraded by `migrate` — never by
//! rewriting a chained log.

use std::fs;
use std::path::PathBuf;

use ledger::store::{self, STATE_SCHEMA_VERSION, Store};

fn fixture(tag: &str) -> (Store, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ledger-schema-{tag}-{}-{}",
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

fn value(store: &Store, name: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(store.path(name)).expect("read")).expect("json")
}

fn write(store: &Store, name: &str, value: &serde_json::Value) {
    fs::write(store.path(name), format!("{value}\n")).expect("write");
}

fn version(store: &Store, name: &str) -> u32 {
    value(store, name)["schema_version"]
        .as_u64()
        .expect("schema_version") as u32
}

/// The demo writes the documents a pilot needs; these two are created on first use.
fn create_optional_documents(store: &Store) {
    ledger::review::Reviews::load(store)
        .expect("reviews")
        .save(store)
        .expect("save reviews");
    ledger::checklist::Checklist::load(store)
        .expect("checklist")
        .save(store)
        .expect("save checklist");
}

#[test]
fn schema_version_is_written_into_every_state_document() {
    let (store, dir) = fixture("every");
    create_optional_documents(&store);

    let fixed = [
        store::GATE_LIBRARY,
        store::TEMPLATES,
        store::PLAYBOOK,
        store::REQUISITIONS,
        store::POLICY,
        store::REVIEW,
        store::CHECKLIST,
        store::OPERATORS,
        store::DEMO_SUMMARY,
    ];
    for name in fixed {
        assert_eq!(
            version(&store, name),
            STATE_SCHEMA_VERSION,
            "{name} is not stamped"
        );
        assert!(
            value(&store, name).is_object(),
            "{name} must be an object to carry schema_version"
        );
    }

    let roster = fs::read_dir(store.root())
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .find(|name| name.starts_with("roster-"))
        .expect("a roster");
    assert_eq!(version(&store, &roster), STATE_SCHEMA_VERSION);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn schema_version_unknown_is_refused_before_anything_is_written() {
    let (store, dir) = fixture("unknown");
    let mut policy = value(&store, store::POLICY);
    policy["schema_version"] = serde_json::Value::from(99_u64);
    write(&store, store::POLICY, &policy);

    let load = ledger::policy::Policy::load(&store);
    let message = format!("{}", load.expect_err("a newer version must not load"));
    assert!(
        message.contains("schema_version") && message.contains("99"),
        "unhelpful refusal: {message}"
    );

    let migrate = ledger::migrate::migrate(&store, "test-operator");
    let message = format!("{}", migrate.expect_err("migrate must refuse"));
    assert!(message.contains("schema_version"), "{message}");
    assert_eq!(
        version(&store, store::POLICY),
        99,
        "a refused migration rewrote the document"
    );
    assert!(
        !store.path("backups").exists(),
        "a refused migration took a snapshot first"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn schema_version_zero_upgrades_to_the_current_version() {
    let (store, dir) = fixture("upgrade");
    create_optional_documents(&store);
    let send_log_before = fs::read(store.path(store::SEND_LOG)).expect("send log");

    // The three documents that used to be bare arrays, plus one object without a stamp.
    for (name, field) in [
        (store::OPERATORS, "operators"),
        (store::CHECKLIST, "completions"),
        (store::REQUISITIONS, "requisitions"),
    ] {
        let legacy = value(&store, name)[field].clone();
        write(&store, name, &legacy);
        assert_eq!(store.raw_version(name).expect("version"), Some(0));
    }
    let mut policy = value(&store, store::POLICY);
    policy
        .as_object_mut()
        .expect("object")
        .remove("schema_version");
    write(&store, store::POLICY, &policy);

    // A pre-versioning workspace still reads before the migration runs.
    assert!(
        !ledger::identity::Registry::load(&store)
            .expect("registry")
            .is_empty()
    );
    assert!(ledger::policy::Policy::load(&store).is_ok());
    assert_ne!(
        ledger::checklist::Checklist::load(&store)
            .expect("checklist")
            .progress()
            .1,
        0
    );
    assert!(
        !ledger::requisition::RequisitionBook::load(&store)
            .expect("book")
            .is_empty()
    );

    let report = ledger::migrate::migrate(&store, "test-operator").expect("migrate");
    for name in [
        store::OPERATORS,
        store::CHECKLIST,
        store::REQUISITIONS,
        store::POLICY,
    ] {
        assert_eq!(
            version(&store, name),
            STATE_SCHEMA_VERSION,
            "{name} was not upgraded"
        );
    }
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.starts_with(store::OPERATORS)),
        "the report does not name the upgrade: {:?}",
        report.changes
    );
    assert!(
        !ledger::identity::Registry::load(&store)
            .expect("registry")
            .operators()
            .is_empty(),
        "the upgraded registry lost its operators"
    );
    assert_eq!(
        fs::read(store.path(store::SEND_LOG)).expect("send log"),
        send_log_before,
        "migration rewrote the chained send log"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn schema_version_migration_is_idempotent() {
    let (store, dir) = fixture("again");
    let report = ledger::migrate::migrate(&store, "test-operator").expect("first");
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.contains("no rewrite")),
        "a current workspace reported changes: {:?}",
        report.changes
    );

    let before = fs::read(store.path(store::POLICY)).expect("policy");
    let report = ledger::migrate::migrate(&store, "test-operator").expect("second");
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.contains("no rewrite")),
        "the second run reported changes: {:?}",
        report.changes
    );
    assert_eq!(
        fs::read(store.path(store::POLICY)).expect("policy"),
        before,
        "a no-op migration rewrote a document"
    );

    let _ = fs::remove_dir_all(&dir);
}
