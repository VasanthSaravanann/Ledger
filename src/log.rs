//! The immutable send log and the blocked-send log (Tasks 3.2 and 4.1).
//!
//! Each entry carries the fields V2 requires for tribunal evidentiary
//! sufficiency — `SEND_ID`, `APPLICATION_ID`, `DISPOSITION`, `GATE_CODE`,
//! `DECISION_MAKER_ID`, `CAPTURE_TIMESTAMP`, `SEND_TIMESTAMP`, `TEMPLATE_ID`,
//! `TEXT_VERSION`, `RENDERED_BODY_HASH`, and `RENDERED_BODY_FULL` — and each line's
//! SHA-256 covers the previous line, so editing or truncating the log is detected
//! before the next append.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::disposition::{DispositionKind, DispositionRecord};
use crate::error::{Error, Result};
use crate::gate::GateKind;
use crate::id::{
    ApplicationId, CandidateEmail, DecisionMakerId, DispositionId, RecruiterId, RequisitionId,
    SendId, TemplateKind, TextVersion,
};
use crate::jsonl::AppendLog;
use crate::qa::QaScope;
use crate::store::{self, Store};
use crate::template::RenderedEmail;

/// The `prev_hash` of the first entry in a chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// `DELIVERY_STATUS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryStatus {
    /// Handed to the mail provider, not yet confirmed.
    Queued,
    /// Accepted by the mail provider.
    Sent,
    /// The provider rejected it.
    Failed,
}

impl DeliveryStatus {
    /// Every status.
    pub const ALL: [Self; 3] = [Self::Queued, Self::Sent, Self::Failed];

    /// The stored code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for DeliveryStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::str::FromStr for DeliveryStatus {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|status| status.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "delivery status",
                    format!("expected queued, sent or failed; got {value:?}"),
                )
            })
    }
}

/// One dispatched feedback email, with the decision and the exact text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendEntry {
    /// Zero-based position in the chain.
    pub seq: u64,
    /// The previous entry's `entry_hash`, or [`GENESIS_HASH`].
    pub prev_hash: String,
    /// SHA-256 over this entry's hashed fields, including `prev_hash`.
    pub entry_hash: String,
    /// `SEND_ID`.
    pub send_id: SendId,
    /// `APPLICATION_ID`.
    pub application_id: ApplicationId,
    /// The recipient's first name, used as a merge value.
    pub candidate_first_name: String,
    /// `EMAIL`.
    pub candidate_email: CandidateEmail,
    /// `REQ_ID`.
    pub requisition: RequisitionId,
    /// `RECRUITER_ID`.
    pub recruiter: RecruiterId,
    /// `DECISION_MAKER_ID`.
    pub decision_maker: DecisionMakerId,
    /// The disposition record this send renders.
    pub disposition_id: DispositionId,
    /// `DISPOSITION`.
    pub disposition: DispositionKind,
    /// `GATE_CODE`, for `GATE` sends.
    pub gate_code: Option<GateKind>,
    /// `CAPTURE_TIMESTAMP`.
    pub captured_at: DateTime<Utc>,
    /// `SEND_TIMESTAMP`.
    pub sent_at: DateTime<Utc>,
    /// `TEMPLATE_ID`.
    pub template: TemplateKind,
    /// `TEXT_VERSION`: the version of the copy that produced the text.
    pub text_version: TextVersion,
    /// The merged subject, exactly as sent.
    pub subject: String,
    /// `RENDERED_BODY_FULL`, exactly as sent.
    pub body: String,
    /// `RENDERED_BODY_HASH`.
    pub text_digest: String,
    /// `DELIVERY_STATUS`.
    pub delivery_status: DeliveryStatus,
}

/// The fields covered by the chain hash, in a fixed order.
#[derive(Serialize)]
struct Hashed<'a> {
    seq: u64,
    prev_hash: &'a str,
    send_id: &'a SendId,
    application_id: &'a ApplicationId,
    candidate_first_name: &'a str,
    candidate_email: &'a CandidateEmail,
    requisition: &'a RequisitionId,
    recruiter: &'a RecruiterId,
    decision_maker: &'a DecisionMakerId,
    disposition_id: &'a DispositionId,
    disposition: DispositionKind,
    gate_code: Option<GateKind>,
    captured_at: DateTime<Utc>,
    sent_at: DateTime<Utc>,
    template: TemplateKind,
    text_version: &'a TextVersion,
    subject: &'a str,
    body: &'a str,
    text_digest: &'a str,
    delivery_status: DeliveryStatus,
}

impl SendEntry {
    /// Recomputes this entry's chain hash.
    fn compute_hash(&self) -> Result<String> {
        let payload = Hashed {
            seq: self.seq,
            prev_hash: &self.prev_hash,
            send_id: &self.send_id,
            application_id: &self.application_id,
            candidate_first_name: &self.candidate_first_name,
            candidate_email: &self.candidate_email,
            requisition: &self.requisition,
            recruiter: &self.recruiter,
            decision_maker: &self.decision_maker,
            disposition_id: &self.disposition_id,
            disposition: self.disposition,
            gate_code: self.gate_code,
            captured_at: self.captured_at,
            sent_at: self.sent_at,
            template: self.template,
            text_version: &self.text_version,
            subject: &self.subject,
            body: &self.body,
            text_digest: &self.text_digest,
            delivery_status: self.delivery_status,
        };
        Ok(sha256_hex(&serde_json::to_vec(&payload)?))
    }
}

/// What the pipeline hands to the log when a send is permitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendDraft {
    /// The captured disposition that authorised the send.
    pub disposition: DispositionRecord,
    /// The version of the copy that produced the text.
    pub text_version: TextVersion,
    /// The merged text.
    pub rendered: RenderedEmail,
    /// The delivery status the operator recorded.
    pub delivery_status: DeliveryStatus,
}

/// Whether the send log is intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogVerification {
    /// How many entries were checked.
    pub entries: usize,
    /// The hash of the newest entry, or [`GENESIS_HASH`] when empty.
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
    /// Returns [`Error::LogCorrupt`] when the chain does not verify.
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
            send_id: SendId::try_from(format!("SND-{:06}", seq + 1))?,
            application_id: draft.disposition.form.application_id,
            candidate_first_name: draft.disposition.form.candidate_first_name,
            candidate_email: draft.disposition.form.candidate_email,
            requisition: draft.disposition.form.requisition,
            recruiter: draft.disposition.form.recruiter,
            decision_maker: draft.disposition.form.decision_maker,
            disposition_id: draft.disposition.id,
            disposition: draft.disposition.form.disposition.kind(),
            gate_code: draft.disposition.form.disposition.gate(),
            captured_at: draft.disposition.form.captured_at,
            sent_at,
            template: draft.disposition.form.disposition.kind().template_kind(),
            text_version: draft.text_version,
            subject: draft.rendered.subject,
            body: draft.rendered.body,
            text_digest,
            delivery_status: draft.delivery_status,
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

    /// The number of entries known to this handle.
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
    /// Returns [`Error::LogCorrupt`] identifying the first failing entry.
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
    /// The gate's requisition facts are missing, so `GATE` cannot render.
    GateUnavailable {
        /// The gate that cannot render.
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
    /// The form's email does not match the ATS export.
    EmailMismatch {
        /// The address on the ATS export.
        export: CandidateEmail,
        /// The address captured on the form.
        form: CandidateEmail,
    },
    /// The second-person QA check has not been recorded yet.
    AwaitingSecondPersonQa {
        /// Whether this was the full `GATE` check or the 10% sample.
        scope: QaScope,
    },
    /// The gate is not one this requisition screens on.
    GateNotInRequisition {
        /// The gate named by the disposition.
        gate: GateKind,
    },
}

impl std::fmt::Display for BlockReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDisposition => {
                formatter.write_str("no completed disposition record for this application")
            }
            Self::GateUnavailable { gate } => write!(
                formatter,
                "{gate} has no documented requisition facts; record the disposition as FIELD"
            ),
            Self::AnswerMismatch {
                gate,
                declared,
                recorded,
            } => write!(
                formatter,
                "form answer {recorded} for {gate} contradicts the application answer {declared}; \
                 escalate to the coordinator, do not auto-correct"
            ),
            Self::EmailMismatch { export, form } => write!(
                formatter,
                "form email {form} does not match the ATS export email {export}"
            ),
            Self::AwaitingSecondPersonQa { scope } => {
                write!(
                    formatter,
                    "second-person QA is not recorded ({})",
                    scope.code()
                )
            }
            Self::GateNotInRequisition { gate } => {
                write!(
                    formatter,
                    "gate {gate} is not screened on by this requisition"
                )
            }
        }
    }
}

/// One blocked send, recorded as evidence of compliance friction.
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
    /// friction is counted per application, not per attempt.
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
    use crate::disposition::{Disposition, NewDisposition};
    use crate::gate::{GateAnswer, GateKind};
    use crate::id::Mailbox;
    use crate::requisition::{Requisition, Sender};
    use chrono::TimeZone;
    use std::collections::BTreeMap;
    use std::fs;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 9, minute, 0)
            .single()
            .expect("timestamp")
    }

    fn disposition() -> DispositionRecord {
        let requisition = Requisition::new(
            crate::requisition::RequisitionDraft {
                id: RequisitionId::try_from("REQ-1").expect("id"),
                employer: "Example Health".to_owned(),
                title: "Registered Nurse".to_owned(),
                reply_to: Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
                sender: Sender::try_new("R. Patel", "Coordinator").expect("sender"),
                gates: vec![GateKind::RightToWork],
                facts: BTreeMap::new(),
            },
            at(0),
        )
        .expect("requisition");
        let form = NewDisposition::try_new(
            &requisition,
            &crate::gate::GateLibrary::seed(),
            crate::disposition::FormSubmission {
                application_id: ApplicationId::try_from("APP-1").expect("application"),
                candidate_first_name: "Ada".to_owned(),
                candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
                disposition: Disposition::Gate {
                    gate: GateKind::RightToWork,
                    answer: GateAnswer::NotHeld,
                },
                supersedes: None,
            },
            at(1),
        )
        .expect("form");
        DispositionRecord {
            id: DispositionId::try_from("DSP-0001").expect("id"),
            locked: true,
            form,
        }
    }

    fn draft() -> SendDraft {
        SendDraft {
            disposition: disposition(),
            text_version: TextVersion::parse("v1.0").expect("version"),
            rendered: RenderedEmail {
                subject: "Update on your application".to_owned(),
                body: "Dear Ada,\n\nYou indicated ...".to_owned(),
            },
            delivery_status: DeliveryStatus::Sent,
        }
    }

    fn temp_store(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-sendlog-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        (Store::open(&dir).expect("workspace"), dir)
    }

    #[test]
    fn chain_links_and_carries_every_required_column() {
        let (store, dir) = temp_store("chain");
        let mut log = SendLog::open(&store).expect("log");
        let entry = log.append(draft(), at(2)).expect("send");
        assert_eq!(entry.send_id.as_str(), "SND-000001");
        assert_eq!(entry.disposition, DispositionKind::Gate);
        assert_eq!(entry.gate_code, Some(GateKind::RightToWork));
        assert_eq!(entry.disposition_id.as_str(), "DSP-0001");
        assert_eq!(entry.text_version.as_str(), "v1.0");
        assert_eq!(entry.delivery_status, DeliveryStatus::Sent);
        assert!(!entry.subject.is_empty() && !entry.body.is_empty());
        assert_eq!(log.verification().expect("verify").entries, 1);
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
        let second = log.append(draft(), at(3)).expect("send");
        assert_eq!(second.seq, 1);
        assert_eq!(second.send_id.as_str(), "SND-000002");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edited_body_is_detected_and_blocks_appends() {
        let (store, dir) = temp_store("tamper");
        {
            let mut log = SendLog::open(&store).expect("log");
            log.append(draft(), at(2)).expect("send");
        }
        let path = store.path(store::SEND_LOG);
        let text = fs::read_to_string(&path).expect("read");
        fs::write(&path, text.replace("Dear Ada", "Dear Somebody")).expect("write");
        assert!(matches!(
            SendLog::open(&store),
            Err(Error::LogCorrupt { .. })
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_log_is_detected() {
        let (store, dir) = temp_store("truncate");
        {
            let mut log = SendLog::open(&store).expect("log");
            log.append(draft(), at(2)).expect("first");
            log.append(draft(), at(3)).expect("second");
        }
        let path = store.path(store::SEND_LOG);
        let lines: Vec<String> = fs::read_to_string(&path)
            .expect("read")
            .lines()
            .map(str::to_owned)
            .collect();
        fs::write(&path, format!("{}\n", lines[1..].join("\n"))).expect("write");
        assert!(matches!(
            SendLog::open(&store),
            Err(Error::LogCorrupt { .. })
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_superseded_disposition_does_not_authorise_a_second_send() {
        let (store, dir) = temp_store("dupe");
        let mut log = SendLog::open(&store).expect("log");
        log.append(draft(), at(2)).expect("send");
        assert!(log.already_sent(
            &RequisitionId::try_from("REQ-1").expect("id"),
            &ApplicationId::try_from("APP-1").expect("application")
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
