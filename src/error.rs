//! The crate's single error type.

use std::path::PathBuf;

use crate::gate::{FactKey, GateAnswer, GateKind};
use crate::id::{ApplicationId, DispositionId, RequisitionId, TemplateKind};

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while running the pilot.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A value failed validation.
    #[error("invalid {kind}: {reason}")]
    Invalid {
        /// What was being validated, for example `"candidate email"`.
        kind: &'static str,
        /// Why the value was rejected.
        reason: String,
    },

    /// The referenced requisition is not in the requisition book.
    #[error("unknown requisition {0}")]
    UnknownRequisition(RequisitionId),

    /// A gate has no pre-approved copy block, so no email can quote it.
    #[error("no pre-approved gate copy for {0}")]
    MissingGateCopy(GateKind),

    /// A disposition was captured twice for the same application.
    #[error(
        "application {} in requisition {requisition} already has disposition {existing}; \
         submit a correction that supersedes it",
        crate::redact::id(application.as_str())
    )]
    DuplicateDisposition {
        /// The requisition the application belongs to.
        requisition: RequisitionId,
        /// The application that already has a disposition.
        application: ApplicationId,
        /// The current record for that application.
        existing: DispositionId,
    },

    /// A requisition fact a gate block needs has no value.
    #[error("missing requisition fact {fact}: {fact}")]
    MissingFact {
        /// The absent fact.
        fact: FactKey,
    },

    /// The gate cannot be used because its requisition facts are missing.
    #[error(
        "gate {gate} is unavailable until its facts are documented ({missing:?}); record the \
         disposition as FIELD instead"
    )]
    GateUnavailable {
        /// The gate that cannot render.
        gate: GateKind,
        /// The facts still missing.
        missing: Vec<FactKey>,
    },

    /// A disposition record id is not in the log.
    #[error("unknown disposition record {0}")]
    UnknownDisposition(DispositionId),

    /// A correction tried to supersede a record that is not the current head.
    #[error("disposition {0} has already been superseded; correct the current record")]
    AlreadySuperseded(DispositionId),

    /// A correction tried to supersede another application's record.
    #[error(
        "disposition {record} belongs to application {}, not {}",
        crate::redact::id(owner.as_str()),
        crate::redact::id(application.as_str())
    )]
    SupersedeMismatch {
        /// The record being superseded.
        record: DispositionId,
        /// The application that owns it.
        owner: ApplicationId,
        /// The application on the new record.
        application: ApplicationId,
    },

    /// The comparative paragraph failed the sign-off lint.
    #[error("comparative paragraph failed sign-off: {issues}")]
    ComparativeRejected {
        /// Pre-joined lint findings.
        issues: String,
    },

    /// The disposition names a gate that the requisition does not screen on.
    #[error("gate {gate} is not a knockout gate of requisition {requisition}")]
    GateNotInRequisition {
        /// The gate named by the disposition.
        gate: GateKind,
        /// The requisition the disposition was captured against.
        requisition: RequisitionId,
    },

    /// The gate answer is not one of the answers that gate offers.
    #[error("{answer} is not an allowed answer for gate {gate}")]
    DisallowedAnswer {
        /// The gate the answer was submitted for.
        gate: GateKind,
        /// The rejected answer.
        answer: GateAnswer,
    },

    /// The same approval role already signed off; the process is single-pass.
    #[error("{role} has already signed off")]
    AlreadySignedOff {
        /// The role that already signed.
        role: crate::approval::SignOffRole,
    },

    /// The requisition cannot be used for dispatch yet.
    #[error("requisition is not ready for dispatch: {issues}")]
    RequisitionNotReady {
        /// Pre-joined list of blocking issues.
        issues: String,
    },

    /// The three master templates have not both been approved.
    #[error("the master email templates are not approved")]
    TemplatesNotApproved,

    /// The five reply scripts have not both been approved.
    #[error("the reply playbook is not approved")]
    PlaybookNotApproved,

    /// A template kind has no draft.
    #[error("no template of kind {0}")]
    MissingTemplate(TemplateKind),

    /// A merge slot in a template is not one of the permitted slots.
    #[error("unknown merge slot {{{{{0}}}}}")]
    UnknownSlot(String),

    /// A merge slot was opened but never closed.
    #[error("unclosed merge slot near byte {0}")]
    UnclosedSlot(usize),

    /// A slot was requested but the disposition cannot supply it.
    #[error("slot {{{{{slot}}}}} has no value for this disposition")]
    MissingSlotValue {
        /// The slot name without braces.
        slot: String,
    },

    /// The ATS export lists the same application id twice.
    #[error("duplicate application id {0} in the ATS export")]
    DuplicateApplication(ApplicationId),

    /// The application id is not in the imported roster.
    #[error("application {} is not in the imported roster", crate::redact::id(.0.as_str()))]
    UnknownApplication(ApplicationId),

    /// A log ends mid-line, which means the writer died during an append.
    #[error(
        "log {path} ends with a partial line ({bytes} byte(s) after the last newline); run \
         `ledger log quarantine --reason <note>` to preserve it, or restore from backup"
    )]
    TornWrite {
        /// The file with the partial line.
        path: PathBuf,
        /// How many bytes follow the last newline.
        bytes: usize,
    },

    /// Another ledger process holds the workspace lock.
    #[error("workspace is locked by {holder}")]
    Locked {
        /// The PID and timestamp written by the holder.
        holder: String,
    },

    /// Dispatch committed nothing because every attempt was blocked.
    #[error("{count} attempt(s) were blocked by the circuit-breakers; nothing was sent")]
    Blocked {
        /// How many attempts were blocked.
        count: usize,
    },

    /// A stored append-only log failed its integrity check.
    #[error("log {path} is corrupt at entry {seq}: {reason}")]
    LogCorrupt {
        /// The log file.
        path: PathBuf,
        /// The sequence number where verification failed.
        seq: u64,
        /// What was wrong.
        reason: String,
    },

    /// The plan task id is not one of the tasks in `plan.md`.
    #[error("unknown plan task {0}")]
    UnknownPlanTask(String),

    /// A filesystem operation failed.
    #[error("io error on {path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },

    /// A stored document could not be parsed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The ATS export could not be parsed.
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
}

impl Error {
    /// The documented process exit code for this failure.
    ///
    /// `0` success, `2` circuit-breaker block, `3` invalid input or state,
    /// `4` workspace lock contention, `1` everything else.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Blocked { .. } => 2,
            Self::Locked { .. } => 4,
            Self::Invalid { .. }
            | Self::UnknownRequisition(_)
            | Self::MissingGateCopy(_)
            | Self::MissingFact { .. }
            | Self::GateUnavailable { .. }
            | Self::UnknownDisposition(_)
            | Self::AlreadySuperseded(_)
            | Self::SupersedeMismatch { .. }
            | Self::ComparativeRejected { .. }
            | Self::GateNotInRequisition { .. }
            | Self::DisallowedAnswer { .. }
            | Self::AlreadySignedOff { .. }
            | Self::RequisitionNotReady { .. }
            | Self::TemplatesNotApproved
            | Self::PlaybookNotApproved
            | Self::MissingTemplate(_)
            | Self::UnknownSlot(_)
            | Self::UnclosedSlot(_)
            | Self::MissingSlotValue { .. }
            | Self::DuplicateApplication(_)
            | Self::UnknownApplication(_)
            | Self::TornWrite { .. }
            | Self::UnknownPlanTask(_) => 3,
            _ => 1,
        }
    }

    /// Builds a validation error.
    pub fn invalid(kind: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            kind,
            reason: reason.into(),
        }
    }
}
