//! Retention enforcement and the purge of expired PII (V5 Phase 2.2).
//!
//! Two records hold candidate PII: the disposition log and the imported ATS
//! snapshot. The send log also holds it, but its `candidate_first_name`,
//! `candidate_email` and `body` are inside the hash chain — redacting them would
//! either break the chain or require re-chaining, which is exactly the tamper
//! evidence the pilot exists to produce. The approved position is therefore:
//!
//! * dispositions and ATS snapshots are purged in place once past
//!   `policy.retention_days`, replaced by stable tombstones;
//! * the send log is **retained** for the full period and erased by deleting the
//!   whole workspace at the end of it, not by selective edits.
//!
//! Every purge writes a `PURGE` event to `operator.jsonl`.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::ats::Roster;
use crate::digest::sha256_hex;
use crate::disposition::{DispositionRecord, Dispositions};
use crate::error::{Error, Result};
use crate::id::CandidateEmail;
use crate::jsonl::AppendLog;
use crate::operator::{OperatorLog, OperatorNote};
use crate::policy::Policy;
use crate::store::{self, Store};

/// What a purge did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeReport {
    /// The cutoff: records captured before this instant are expired.
    pub cutoff: DateTime<Utc>,
    /// Dispositions whose PII was replaced.
    pub dispositions_purged: usize,
    /// Dispositions left untouched.
    pub dispositions_retained: usize,
    /// ATS snapshots whose PII was replaced.
    pub rosters_purged: usize,
    /// Send-log entries retained, because they are chained.
    pub send_log_entries_retained: usize,
}

/// A stable tombstone for a purged identifier: `[PURGED:ab12cd34]`.
pub fn tombstone(value: &str) -> String {
    format!("[PURGED:{}]", &sha256_hex(value.as_bytes())[0..8])
}

/// A purged email must still parse as an address, so it becomes a reserved one.
fn purged_email(original: &str) -> Result<CandidateEmail> {
    let handle = &sha256_hex(original.as_bytes())[0..8];
    CandidateEmail::try_from(format!("purged-{handle}@purged.invalid"))
}

/// Replaces expired PII with tombstones.
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Json`] when the workspace cannot be rewritten.
pub fn purge(store: &Store, operator: &str, now: DateTime<Utc>) -> Result<PurgeReport> {
    let policy = Policy::load(store)?;
    let cutoff = now - Duration::days(i64::from(policy.retention_days));

    let dispositions = Dispositions::open(store)?;
    let mut records: Vec<DispositionRecord> = dispositions.records().to_vec();
    let mut purged = 0;
    for record in &mut records {
        if record.form.captured_at >= cutoff {
            continue;
        }
        purged += 1;
        record.form.candidate_first_name = tombstone(&record.form.candidate_first_name);
        record.form.candidate_email = purged_email(record.form.candidate_email.as_str())?;
    }
    if purged > 0 {
        rewrite_log(store, store::DISPOSITIONS, &records)?;
    }
    let retained = records.len().saturating_sub(purged);

    let mut rosters_purged = 0;
    let root = store.root().to_path_buf();
    let mut roster_paths: Vec<_> = std::fs::read_dir(&root)
        .map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .map(|name| {
                    let name = name.to_string_lossy();
                    name.starts_with("roster-") && name.ends_with(".json")
                })
                .unwrap_or(false)
        })
        .collect();
    roster_paths.sort();

    for path in roster_paths {
        let Some(mut roster) =
            store.load::<Roster>(&path.file_name().unwrap_or_default().to_string_lossy())?
        else {
            continue;
        };
        let expired = roster
            .applicants
            .iter()
            .filter(|applicant| applicant.applied_at.is_none_or(|applied| applied < cutoff))
            .count();
        if expired == 0 {
            continue;
        }
        for applicant in &mut roster.applicants {
            let expired = applicant.applied_at.is_none_or(|applied| applied < cutoff);
            if !expired {
                continue;
            }
            applicant.first_name = tombstone(&applicant.first_name);
            applicant.email = purged_email(applicant.email.as_str())?;
        }
        rosters_purged += 1;
        store.save(
            &path.file_name().unwrap_or_default().to_string_lossy(),
            &roster,
        )?;
    }

    let send_log_entries_retained = crate::log::SendLog::open(store)?.len();

    let mut details = std::collections::BTreeMap::new();
    details.insert("cutoff".to_owned(), cutoff.to_rfc3339());
    details.insert("dispositions_purged".to_owned(), purged.to_string());
    details.insert("rosters_purged".to_owned(), rosters_purged.to_string());
    details.insert(
        "send_log_retained".to_owned(),
        send_log_entries_retained.to_string(),
    );
    OperatorLog::open(store).record(OperatorNote {
        at: now,
        operator: operator.to_owned(),
        command: "purge".to_owned(),
        note: format!(
            "retention {} days; send log retained because it is chained",
            policy.retention_days
        ),
        details,
    })?;

    Ok(PurgeReport {
        cutoff,
        dispositions_purged: purged,
        dispositions_retained: retained,
        rosters_purged,
        send_log_entries_retained,
    })
}

/// Atomically rewrites an append-only log of unchained records.
fn rewrite_log<T: Serialize>(store: &Store, name: &str, records: &[T]) -> Result<()> {
    let path = store.path(name);
    let temp = path.with_extension("purge.tmp");
    let mut text = String::new();
    for record in records {
        text.push_str(&serde_json::to_string(record)?);
        text.push('\n');
    }
    std::fs::write(&temp, text).map_err(|source| Error::Io {
        path: temp.clone(),
        source,
    })?;
    store::restrict(&temp, 0o600)?;
    std::fs::rename(&temp, &path).map_err(|source| Error::Io { path, source })?;
    crate::jsonl::sync_dir(store.root())
}

/// Reads the log for a dry run without writing.
///
/// # Errors
///
/// Propagates read errors.
pub fn preview(store: &Store, now: DateTime<Utc>) -> Result<PurgeReport> {
    let policy = Policy::load(store)?;
    let cutoff = now - Duration::days(i64::from(policy.retention_days));
    let dispositions = Dispositions::open(store)?;
    let expired = dispositions
        .records()
        .iter()
        .filter(|record| record.form.captured_at < cutoff)
        .count();
    Ok(PurgeReport {
        cutoff,
        dispositions_purged: expired,
        dispositions_retained: dispositions.len().saturating_sub(expired),
        rosters_purged: 0,
        send_log_entries_retained: crate::log::SendLog::open(store)?.len(),
    })
}

/// A `Vec<DispositionRecord>` round-trip through the append-only encoding, used by
/// `purge` and its test.
pub fn encode_dispositions(records: &[DispositionRecord]) -> Result<String> {
    let mut text = String::new();
    for record in records {
        text.push_str(&serde_json::to_string(record)?);
        text.push('\n');
    }
    Ok(text)
}

/// Appends to a log, exposed so `purge` does not need a mutable handle.
pub fn append_note<T: Serialize>(store: &Store, name: &str, record: &T) -> Result<()> {
    let mut log = AppendLog::<T>::open(store.path(name));
    log.append(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn workspace(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-purge-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");
        crate::demo::seed(&store, false, true).expect("fixture");
        (store, dir)
    }

    #[test]
    fn nothing_expires_under_the_default_retention() {
        let (store, dir) = workspace("fresh");
        let report = purge(&store, "qa-patel", Utc::now()).expect("purge");
        assert_eq!(report.dispositions_purged, 0);
        assert_eq!(report.send_log_entries_retained, 147);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expired_dispositions_are_tombstoned_and_the_event_is_logged() {
        let (store, dir) = workspace("expired");
        let far_future = Utc::now() + Duration::days(365 * 20);
        let report = purge(&store, "qa-patel", far_future).expect("purge");
        assert_eq!(report.dispositions_purged, 149);
        assert_eq!(report.dispositions_retained, 0);

        let text = std::fs::read_to_string(store.path(store::DISPOSITIONS)).expect("log");
        assert!(!text.contains("@example.com"), "emails are gone");
        assert!(text.contains("[PURGED:"));
        assert!(
            !text.contains("\"candidate_first_name\":\"Ada\""),
            "names are gone"
        );

        // The send log is chained, so it is retained untouched.
        let log = crate::log::SendLog::open(&store).expect("send log");
        assert_eq!(log.len(), 147);
        assert!(log.verification().is_ok());

        let notes = crate::operator::OperatorLog::open(&store)
            .all()
            .expect("operator log");
        assert!(notes.iter().any(|note| note.command == "purge"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn purge_keeps_record_identity_and_chain_links() {
        let (store, dir) = workspace("identity");
        let before = Dispositions::open(&store).expect("dispositions");
        let ids: Vec<String> = before
            .records()
            .iter()
            .map(|record| record.id.to_string())
            .collect();
        let supersedes: Vec<Option<String>> = before
            .records()
            .iter()
            .map(|record| record.form.supersedes.as_ref().map(ToString::to_string))
            .collect();

        purge(&store, "qa-patel", Utc::now() + Duration::days(365 * 20)).expect("purge");

        let after = Dispositions::open(&store).expect("dispositions");
        assert_eq!(
            after
                .records()
                .iter()
                .map(|record| record.id.to_string())
                .collect::<Vec<String>>(),
            ids
        );
        assert_eq!(
            after
                .records()
                .iter()
                .map(|record| record.form.supersedes.as_ref().map(ToString::to_string))
                .collect::<Vec<Option<String>>>(),
            supersedes
        );
        assert_eq!(after.len(), before.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
