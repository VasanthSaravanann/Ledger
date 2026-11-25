//! Schema detection, snapshots, and migration (V5 Phase 2.3).
//!
//! The rule is absolute: **migrations never rewrite a chained log**. A migration
//! may rewrite unchained state documents, and it always snaps the workspace first.
//! If the workspace carries a schema version this build does not know, it fails
//! closed rather than guessing.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::log::{SCHEMA_VERSION, SendLog};
use crate::operator::{OperatorLog, OperatorNote};
use crate::store::{self, STATE_SCHEMA_VERSION, Store};

/// Every fixed-name state document this build knows how to version.
const STATE_FILES: [&str; 9] = [
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

/// What a migration run found and did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrateReport {
    /// The snapshot the run took first.
    pub snapshot: PathBuf,
    /// The oldest schema version found in the workspace; 0 when any document
    /// predates versioning, and also 0 for an empty workspace.
    pub found_version: u32,
    /// The version this build writes.
    pub target_version: u32,
    /// What was changed. Empty means the workspace was already current.
    pub changes: Vec<String>,
}

/// Reports the schema versions present without changing anything.
///
/// # Errors
///
/// Returns [`Error::LogCorrupt`] when the send log does not verify, and
/// [`Error::Invalid`] when an entry declares a newer schema than this build.
pub fn inspect(store: &Store) -> Result<(u32, usize)> {
    let log = SendLog::open(store)?;
    let entries = log.entries()?;
    let mut oldest = u32::MAX;
    for entry in &entries {
        if entry.schema_version > SCHEMA_VERSION {
            return Err(Error::invalid(
                "schema",
                format!(
                    "workspace uses schema_version {} but this build knows {SCHEMA_VERSION}; \
                     upgrade ledger rather than downgrading the data",
                    entry.schema_version
                ),
            ));
        }
        oldest = oldest.min(entry.schema_version);
    }
    for name in state_files(store)? {
        let Some(version) = store.raw_version(&name)? else {
            continue;
        };
        if version > STATE_SCHEMA_VERSION {
            return Err(Error::invalid(
                "schema",
                format!(
                    "{name} declares schema_version {version} but this build knows \
                     {STATE_SCHEMA_VERSION}; upgrade ledger rather than downgrading the data"
                ),
            ));
        }
        oldest = oldest.min(version);
    }
    Ok((if oldest == u32::MAX { 0 } else { oldest }, entries.len()))
}

/// Every state document present: the fixed names, plus one roster per requisition.
fn state_files(store: &Store) -> Result<Vec<String>> {
    let mut names: Vec<String> = STATE_FILES.iter().map(|name| (*name).to_owned()).collect();
    for entry in fs::read_dir(store.root()).map_err(|source| Error::Io {
        path: store.root().to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: store.root().to_path_buf(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("roster-") && name.ends_with(".json") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Rewrites one older state document through its typed form, then proves it reads back.
fn upgrade_document(store: &Store, name: &str) -> Result<()> {
    macro_rules! rewrite {
        ($kind:ty) => {{
            if let Some(document) = store.load::<$kind>(name)? {
                store.save(name, &document)?;
                store.load::<$kind>(name)?.ok_or_else(|| {
                    Error::invalid("schema", format!("{name} disappeared during migration"))
                })?;
            }
        }};
    }
    match name {
        store::GATE_LIBRARY => rewrite!(crate::gate::GateLibrary),
        store::TEMPLATES => rewrite!(crate::template::Templates),
        store::PLAYBOOK => rewrite!(crate::playbook::Playbook),
        store::REQUISITIONS => rewrite!(crate::requisition::RequisitionBook),
        store::POLICY => rewrite!(crate::policy::Policy),
        store::REVIEW => rewrite!(crate::review::Reviews),
        store::CHECKLIST => rewrite!(crate::checklist::Checklist),
        store::OPERATORS => rewrite!(crate::identity::Registry),
        store::DEMO_SUMMARY => rewrite!(crate::demo::DemoSummary),
        roster if roster.starts_with("roster-") => rewrite!(crate::ats::Roster),
        other => {
            return Err(Error::invalid(
                "schema",
                format!("{other} is not a state document this build can migrate"),
            ));
        }
    }
    Ok(())
}

/// Snapshots the workspace, then migrates what may be migrated.
///
/// # Errors
///
/// Returns [`Error::Io`] when the snapshot cannot be written, and propagates
/// [`inspect`] errors.
pub fn migrate(store: &Store, operator: &str) -> Result<MigrateReport> {
    let (found_version, _entries) = inspect(store)?;

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let snapshot = store.path("backups").join(&stamp);
    snapshot_workspace(store, &snapshot)?;

    let mut changes = Vec::new();
    for name in state_files(store)? {
        match store.raw_version(&name)? {
            None => {}
            Some(version) if version == STATE_SCHEMA_VERSION => {}
            Some(version) if version > STATE_SCHEMA_VERSION => {
                // `inspect` already refused this; re-check rather than write under it.
                return Err(Error::invalid(
                    "schema",
                    format!("{name} declares schema_version {version}; refusing to migrate"),
                ));
            }
            Some(version) => {
                upgrade_document(store, &name)?;
                changes.push(format!(
                    "{name}: schema_version {version} -> {STATE_SCHEMA_VERSION}"
                ));
            }
        }
    }
    if changes.is_empty() {
        changes.push(format!(
            "no rewrite required: state documents are at schema_version {STATE_SCHEMA_VERSION}"
        ));
    }

    let mut details = std::collections::BTreeMap::new();
    details.insert("snapshot".to_owned(), snapshot.display().to_string());
    details.insert("found_version".to_owned(), found_version.to_string());
    details.insert("target_version".to_owned(), SCHEMA_VERSION.to_string());
    OperatorLog::open(store).record(OperatorNote {
        at: Utc::now(),
        operator: operator.to_owned(),
        command: "migrate".to_owned(),
        note: "snapshot taken; chained logs never rewritten in place".to_owned(),
        details,
    })?;

    let target_version = STATE_SCHEMA_VERSION.max(SCHEMA_VERSION);
    Ok(MigrateReport {
        snapshot,
        found_version,
        target_version,
        changes,
    })
}

/// Copies every workspace file into `destination` with owner-only permissions.
///
/// # Errors
///
/// Returns [`Error::Io`] when a file cannot be read or written.
pub fn snapshot_workspace(store: &Store, destination: &Path) -> Result<()> {
    store::ensure_dir(destination)?;
    let root = store.root().to_path_buf();
    for entry in fs::read_dir(&root).map_err(|source| Error::Io {
        path: root.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let target = destination.join(entry.file_name());
        fs::copy(&path, &target).map_err(|source| Error::Io {
            path: target.clone(),
            source,
        })?;
        store::restrict(&target, 0o600)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-migrate-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");
        crate::demo::seed(&store, false, true).expect("fixture");
        (store, dir)
    }

    #[test]
    fn a_current_workspace_reports_its_version_and_snapshots() {
        let (store, dir) = workspace("current");
        let report = migrate(&store, "qa-patel").expect("migrate");
        assert_eq!(report.found_version, 1);
        assert_eq!(report.target_version, 1);
        assert!(report.snapshot.is_dir());
        assert!(report.snapshot.join(store::SEND_LOG).is_file());

        let notes = crate::operator::OperatorLog::open(&store)
            .all()
            .expect("operator log");
        assert!(notes.iter().any(|note| note.command == "migrate"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_schema_fails_closed_without_touching_files() {
        let (store, dir) = workspace("newer");
        let path = store.path(store::SEND_LOG);
        let text = fs::read_to_string(&path).expect("read");
        fs::write(
            &path,
            text.replacen("\"schema_version\":1", "\"schema_version\":9", 1),
        )
        .expect("write");

        let error = inspect(&store).expect_err("must fail closed");
        assert!(error.to_string().contains("upgrade ledger"));
        assert!(!store.path("backups").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
