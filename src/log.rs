//! The immutable send log and the blocked-send log (Tasks 3.2 and 3.3).
//!
//! Every dispatched email is appended as one JSON line whose hash covers the
//! previous line's hash, so removing, reordering, or editing a line breaks the
//! chain and [`SendLog::open`] refuses to append to a broken log. The log stores
//! the rendered subject and body verbatim, together with the disposition capture
//! time, so the exact text and the exact decision are provably joined.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::disposition::{DispositionCategory, DispositionRecord};
use crate::error::{Error, Result};
use crate::gate::GateKind;
use crate::id::{ApplicationId, CandidateEmail, RecruiterId, RequisitionId, TemplateKind};
use crate::jsonl::AppendLog;
use crate::store::{self, Store};
use crate::template::RenderedEmail;

/// The `prev_hash` of the first entry in a chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One dispatched feedback email, with the decision and the exact text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendEntry {
    /// Zero-based position in the chain.
    pub seq: u64,
    /// The previous entry's `entry_hash`, or [`GENESIS_HASH`] for the first entry.
    pub prev_hash: String,
    /// SHA-256 over this entry's hashed fields, including `prev_hash`.
    pub entry_hash: String,
    /// When the disposition was captured.
    pub disposition_captured_at: DateTime<Utc>,
    /// When the email was dispatched.
    pub sent_at: DateTime<Utc>,
    /// The application the email belongs to.
    pub application_id: ApplicationId,
    /// The recipient address.
    pub candidate_email: CandidateEmail,
    /// The recipient name used as a merge value.
    pub candidate_name: String,
    /// The requisition.
    pub requisition: RequisitionId,
    /// The recruiter who captured the disposition.
    pub recruiter: RecruiterId,
    /// The exact category that was captured.
    pub category: DispositionCategory,
    /// Which template produced the text.
    pub template: TemplateKind,
    /// The merged subject line, exactly as sent.
    pub subject: String,
    /// The merged body, exactly as sent.
    pub body: String,
    /// SHA-256 of the exact subject and body bytes.
    pub text_digest: String,
}

/// The fields covered by the chain hash, in a fixed order.
#[derive(Serialize)]
struct Hashed<'a> {
    seq: u64,
    prev_hash: &'a str,
    disposition_captured_at: DateTime<Utc>,
    sent_at: DateTime<Utc>,
    application_id: &'a ApplicationId,
    candidate_email: &'a CandidateEmail,
    candidate_name: &'a str,
    requisition: &'a RequisitionId,
    recruiter: &'a RecruiterId,
    category: &'a DispositionCategory,
    template: TemplateKind,
    subject: &'a str,
    body: &'a str,
    text_digest: &'a str,
}

impl SendEntry {
    /// Recomputes this entry's chain hash.
    fn compute_hash(&self) -> Result<String> {
        let payload = Hashed {
            seq: self.seq,
            prev_hash: &self.prev_hash,
            disposition_captured_at: self.disposition_captured_at,
            sent_at: self.sent_at,
            application_id: &self.application_id,
            candidate_email: &self.candidate_email,
            candidate_name: &self.candidate_name,
            requisition: &self.requisition,
            recruiter: &self.recruiter,
            category: &self.category,
            template: self.template,
            subject: &self.subject,
            body: &self.body,
            text_digest: &self.text_digest,
        };
        Ok(sha256_hex(&serde_json::to_vec(&payload)?))
    }
}

/// What the pipeline hands to the log when a send is permitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendDraft {
    /// The captured disposition that authorised the send.
    pub disposition: DispositionRecord,
    /// The template that was used.
    pub template: TemplateKind,
    /// The merged text.
    pub rendered: RenderedEmail,
}

/// Whether the send log is intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogVerification {
    /// How many entries were checked.
    pub entries: usize,
    /// The hash of the newest entry, or [`GENESIS_HASH`] when the log is empty.
    pub head: String,
}

/// The append-only, hash-chained log of dispatched emails.
#[derive(Debug)]
pub struct SendLog {
    path: PathBuf,
    log: AppendLog<SendEntry>,
    tail: Option<SendEntry>,
    sent: BTreeSet<(RequisitionId, ApplicationId)>,
}

impl SendLog {
    /// Opens and verifies the log before anything is appended.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LogCorrupt`] when the chain does not verify, so a send can
    /// never extend a tampered log.
    pub fn open(store: &Store) -> Result<Self> {
        let log = store.log(store::SEND_LOG);
        let entries: Vec<SendEntry> = log.read_all()?;
        verify_chain(log.path(), &entries)?;
        let sent = entries
            .iter()
            .map(|entry| (entry.requisition.clone(), entry.application_id.clone()))
            .collect();
        Ok(Self {
            path: log.path().to_path_buf(),
            log,
            tail: entries.last().cloned(),
            sent,
        })
    }

    /// Appends one dispatch and returns the stored entry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Json`] when the entry cannot be serialized and
    /// [`Error::Io`] when it cannot be written.
    pub fn append(&mut self, draft: SendDraft, sent_at: DateTime<Utc>) -> Result<SendEntry> {
        let text_digest = draft.rendered.digest();
        let prev_hash = self
            .tail
            .as_ref()
            .map_or_else(|| GENESIS_HASH.to_owned(), |tail| tail.entry_hash.clone());
        let seq = self.tail.as_ref().map_or(0, |tail| tail.seq + 1);
        let mut entry = SendEntry {
            seq,
            prev_hash,
            entry_hash: String::new(),
            disposition_captured_at: draft.disposition.captured_at,
            sent_at,
            application_id: draft.disposition.application_id,
            candidate_email: draft.disposition.candidate_email,
            candidate_name: draft.disposition.candidate_name,
            requisition: draft.disposition.requisition,
            recruiter: draft.disposition.recruiter,
            category: draft.disposition.category,
            template: draft.template,
            subject: draft.rendered.subject,
            body: draft.rendered.body,
            text_digest,
        };
        entry.entry_hash = entry.compute_hash()?;
        self.log.append(&entry)?;
        self.sent
            .insert((entry.requisition.clone(), entry.application_id.clone()));
        self.tail = Some(entry.clone());
        Ok(entry)
    }

    /// Every entry, in send order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn entries(&self) -> Result<Vec<SendEntry>> {
        self.log.read_all()
    }

    /// The number of entries currently known to this handle.
    pub fn len(&self) -> usize {
        self.tail.as_ref().map_or(0, |tail| tail.seq as usize + 1)
    }

    /// Whether nothing has been sent yet.
    pub fn is_empty(&self) -> bool {
        self.tail.is_none()
    }

    /// Whether a send was already logged for this application.
    pub fn already_sent(
        &self,
        requisition: &RequisitionId,
        application_id: &ApplicationId,
    ) -> bool {
        self.sent
            .contains(&(requisition.clone(), application_id.clone()))
    }

    /// Re-verifies the whole chain.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LogCorrupt`] identifying the first entry that fails.
    pub fn verification(&self) -> Result<LogVerification> {
        let entries = self.entries()?;
        verify_chain(&self.path, &entries)?;
        Ok(LogVerification {
            entries: entries.len(),
            head: entries
                .last()
                .map_or_else(|| GENESIS_HASH.to_owned(), |entry| entry.entry_hash.clone()),
        })
    }
}

/// Checks sequence numbers, chaining, and recomputed hashes.
fn verify_chain(path: &Path, entries: &[SendEntry]) -> Result<()> {
    let mut expected_prev = GENESIS_HASH.to_owned();
    for (index, entry) in entries.iter().enumerate() {
        let seq = index as u64;
        let corrupt = |reason: String| Error::LogCorrupt {
            path: path.to_path_buf(),
            seq,
            reason,
        };
        if entry.seq != seq {
            return Err(corrupt(format!(
                "expected sequence {seq}, found {}",
                entry.seq
            )));
        }
        if entry.prev_hash != expected_prev {
            return Err(corrupt(
                "prev_hash does not match the previous entry".to_owned(),
            ));
        }
        if entry.compute_hash()? != entry.entry_hash {
            return Err(corrupt(
                "entry hash does not cover the stored fields".to_owned(),
            ));
        }
        expected_prev = entry.entry_hash.clone();
    }
    Ok(())
}

/// Why the gate-blocking protocol refused to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum BlockReason {
    /// No completed, timestamped disposition exists for the application.
    MissingDisposition,
    /// The gate on the disposition is not one this requisition screens on.
    GateNotInRequisition {
        /// The gate named by the disposition.
        gate: GateKind,
    },
    /// The disposition's gate answer contradicts the ATS knockout answer.
    AnswerMismatch {
        /// The gate in question.
        gate: GateKind,
        /// The answer the candidate gave in the ATS.
        declared: crate::gate::GateAnswer,
        /// The answer captured on the form.
        recorded: crate::gate::GateAnswer,
    },
    /// The form's candidate email does not match the ATS export.
    EmailMismatch {
        /// The address on the ATS export.
        export: CandidateEmail,
        /// The address captured on the form.
        form: CandidateEmail,
    },
}

impl std::fmt::Display for BlockReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDisposition => {
                formatter.write_str("no completed disposition record for this application")
            }
            Self::GateNotInRequisition { gate } => {
                write!(
                    formatter,
                    "gate {gate} is not screened on by this requisition"
                )
            }
            Self::AnswerMismatch {
                gate,
                declared,
                recorded,
            } => write!(
                formatter,
                "form answer {recorded} for {gate} contradicts the application answer {declared}"
            ),
            Self::EmailMismatch { export, form } => write!(
                formatter,
                "form email {form} does not match the ATS export email {export}"
            ),
        }
    }
}

/// One blocked send, recorded as evidence of compliance friction (metric 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedAttempt {
    /// When the attempt was blocked.
    pub at: DateTime<Utc>,
    /// The requisition.
    pub requisition: RequisitionId,
    /// The application that could not be sent to.
    pub application_id: ApplicationId,
    /// The address the send would have used, when known.
    pub candidate_email: Option<CandidateEmail>,
    /// Why the send was refused.
    pub reason: BlockReason,
}

/// The append-only log of blocked sends.
#[derive(Debug)]
pub struct BlockedLog {
    log: AppendLog<BlockedAttempt>,
    attempts: Vec<BlockedAttempt>,
}

impl BlockedLog {
    /// Opens the blocked-send log.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the log cannot be read.
    pub fn open(store: &Store) -> Result<Self> {
        let log = store.log(store::BLOCKED_LOG);
        let attempts = log.read_all()?;
        Ok(Self { log, attempts })
    }

    /// Records a blocked send once.
    ///
    /// A retry that hits the same block is not recorded again, so compliance
    /// friction is counted per application, not per attempt. Returns whether the
    /// attempt was new.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the record cannot be written.
    pub fn record(&mut self, attempt: BlockedAttempt) -> Result<bool> {
        if self.blocked_before(&attempt) {
            return Ok(false);
        }
        self.log.append(&attempt)?;
        self.attempts.push(attempt);
        Ok(true)
    }

    /// Whether this application already has the same block on file.
    pub fn blocked_before(&self, attempt: &BlockedAttempt) -> bool {
        self.attempts.iter().any(|existing| {
            existing.requisition == attempt.requisition
                && existing.application_id == attempt.application_id
                && existing.reason == attempt.reason
        })
    }

    /// Every blocked attempt, in order.
    pub fn attempts(&self) -> &[BlockedAttempt] {
        &self.attempts
    }

    /// How many attempts were blocked for `requisition`.
    pub fn count_for(&self, requisition: &RequisitionId) -> usize {
        self.attempts
            .iter()
            .filter(|attempt| &attempt.requisition == requisition)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disposition::DispositionRecord;
    use crate::gate::GateAnswer;
    use crate::id::Mailbox;
    use crate::requisition::Requisition;
    use chrono::TimeZone;
    use std::fs;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 9, minute, 0)
            .single()
            .expect("valid timestamp")
    }

    fn disposition() -> DispositionRecord {
        let requisition = Requisition::new(
            RequisitionId::try_from("REQ-1").expect("id"),
            "Example Health",
            "Registered Nurse",
            Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
            [GateKind::VisaRestrictions],
            at(0),
        )
        .expect("requisition");
        DispositionRecord::try_new(
            &requisition,
            ApplicationId::try_from("APP-1").expect("application"),
            CandidateEmail::try_from("ada@example.com").expect("email"),
            "Ada Lovelace",
            RecruiterId::try_from("rec-rui").expect("recruiter"),
            at(1),
            DispositionCategory::FailedGate {
                gate: GateKind::VisaRestrictions,
                answer: GateAnswer::Restricted,
            },
        )
        .expect("record")
    }

    fn draft() -> SendDraft {
        SendDraft {
            disposition: disposition(),
            template: TemplateKind::FailedGate,
            rendered: RenderedEmail {
                subject: "Update on your application".to_owned(),
                body: "Dear Ada Lovelace,\n\nYou indicated that ...".to_owned(),
            },
        }
    }

    fn temp_store(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ledger-sendlog-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");
        (store, dir)
    }

    #[test]
    fn chain_links_and_verifies() {
        let (store, dir) = temp_store("chain");
        let mut log = SendLog::open(&store).expect("log");
        assert!(log.is_empty());
        let first = log.append(draft(), at(2)).expect("first send");
        let second = log.append(draft(), at(3)).expect("second send");
        assert_eq!(first.seq, 0);
        assert_eq!(first.prev_hash, GENESIS_HASH);
        assert_eq!(second.prev_hash, first.entry_hash);
        assert_eq!(log.verification().expect("verify").entries, 2);
        assert_eq!(
            first.text_digest,
            sha256_hex(b"Update on your application\nDear Ada Lovelace,\n\nYou indicated that ...")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reopening_continues_the_chain() {
        let (store, dir) = temp_store("reopen");
        {
            let mut log = SendLog::open(&store).expect("log");
            log.append(draft(), at(2)).expect("send");
        }
        let mut log = SendLog::open(&store).expect("reopen");
        assert_eq!(log.len(), 1);
        let second = log.append(draft(), at(3)).expect("send");
        assert_eq!(second.seq, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn edits_are_detected_and_block_further_appends() {
        let (store, dir) = temp_store("tamper");
        {
            let mut log = SendLog::open(&store).expect("log");
            log.append(draft(), at(2)).expect("send");
        }
        let path = store.path(store::SEND_LOG);
        let text = fs::read_to_string(&path).expect("read log");
        let tampered = text.replace("Dear Ada Lovelace", "Dear Somebody Else");
        fs::write(&path, tampered).expect("write log");

        assert!(matches!(
            SendLog::open(&store),
            Err(Error::LogCorrupt { .. })
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn removed_entries_are_detected() {
        let (store, dir) = temp_store("truncate");
        {
            let mut log = SendLog::open(&store).expect("log");
            log.append(draft(), at(2)).expect("first");
            log.append(draft(), at(3)).expect("second");
        }
        let path = store.path(store::SEND_LOG);
        let mut lines: Vec<String> = fs::read_to_string(&path)
            .expect("read")
            .lines()
            .map(str::to_owned)
            .collect();
        lines.remove(0);
        fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write");

        assert!(matches!(
            SendLog::open(&store),
            Err(Error::LogCorrupt { .. })
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
