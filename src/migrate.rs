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
use crate::store::{self, Store};

/// What a migration run found and did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrateReport {
    /// The snapshot the run took first.
    pub snapshot: PathBuf,
    /// The newest schema version found in the workspace.
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
    let mut newest = 0;
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
        newest = newest.max(entry.schema_version);
    }
    Ok((newest, entries.len()))
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
    if found_version == 0 {
        // Nothing to migrate: either an empty workspace or a pre-versioned log,
        // whose entries default to version 1 on read.
        changes.push(format!(
            "no rewrite required: entries default to schema_version {SCHEMA_VERSION}"
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

    Ok(MigrateReport {
        snapshot,
        found_version,
        target_version: SCHEMA_VERSION,
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
