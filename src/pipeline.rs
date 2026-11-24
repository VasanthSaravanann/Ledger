//! Manual dispatch under the gate-blocking protocol (Tasks 3.3 and 4.1).
//!
//! Feedback is merged and "sent" one application at a time. A send only happens
//! when a completed, timestamped disposition exists, it matches the requisition,
//! and — where the ATS export carries the candidate's own answer — the form agrees
//! with it. Every refusal is logged, which is what makes metric 2 measurable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ats::Roster;
use crate::disposition::{DispositionCategory, Dispositions};
use crate::error::{Error, Result};
use crate::gate::GateLibrary;
use crate::id::{ApplicationId, CandidateEmail};
use crate::log::{BlockReason, BlockedAttempt, BlockedLog, SendDraft, SendEntry, SendLog};
use crate::requisition::Requisition;
use crate::template::{MergeContext, RenderedEmail, Templates, render};

/// Whether this run writes, or only shows what would be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Append to the send log and the blocked log.
    Execute,
    /// Render and report, writing nothing.
    Preview,
}

/// One email as it would be sent, in [`Mode::Preview`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    /// The application.
    pub application_id: ApplicationId,
    /// The recipient address.
    pub candidate_email: CandidateEmail,
    /// The recipient name used as a merge value.
    pub candidate_name: String,
    /// The category that authorised the send.
    pub category: DispositionCategory,
    /// The merged text.
    pub rendered: RenderedEmail,
}

/// Everything one dispatch run needs.
#[derive(Debug)]
pub struct Dispatch<'a> {
    /// Execute or preview only.
    pub mode: Mode,
    /// The requisition being actioned.
    pub requisition: &'a Requisition,
    /// The gate copy library.
    pub library: &'a GateLibrary,
    /// The approved master templates.
    pub templates: &'a Templates,
    /// The imported ATS roster.
    pub roster: &'a Roster,
    /// Captured dispositions.
    pub dispositions: &'a Dispositions,
    /// The clock to stamp sends with.
    pub now: DateTime<Utc>,
}

/// What one dispatch run did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchReport {
    /// Emails appended to the send log.
    pub sent: Vec<SendEntry>,
    /// Emails rendered but not written.
    pub previews: Vec<Preview>,
    /// Attempts refused by the gate-blocking protocol.
    pub blocked: Vec<BlockedAttempt>,
}

impl DispatchReport {
    /// How many emails the run actually sent.
    pub fn sent_count(&self) -> usize {
        self.sent.len()
    }

    /// How many attempts the run refused.
    pub fn blocked_count(&self) -> usize {
        self.blocked.len()
    }
}

/// Dispatches feedback for every applicant that has not already been sent to.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the roster belongs to another requisition,
/// [`Error::RequisitionNotReady`] when the requisition's copy is unsigned,
/// [`Error::TemplatesNotApproved`] before sign-off, and propagates template and
/// log errors.
pub fn dispatch(
    job: &Dispatch<'_>,
    send_log: &mut SendLog,
    blocked_log: &mut BlockedLog,
) -> Result<DispatchReport> {
    if job.roster.requisition != job.requisition.id {
        return Err(Error::invalid(
            "roster",
            format!(
                "roster belongs to {}, not {}",
                job.roster.requisition, job.requisition.id
            ),
        ));
    }
    if !job.requisition.is_ready(job.library) {
        return Err(Error::RequisitionNotReady {
            issues: job.requisition.blocking_issues(job.library).join("; "),
        });
    }
    if !job.templates.is_approved() {
        return Err(Error::TemplatesNotApproved);
    }

    let mut report = DispatchReport::default();
    for applicant in &job.roster.applicants {
        let application_id = &applicant.application_id;
        if send_log.already_sent(&job.requisition.id, application_id) {
            continue;
        }

        let Some(record) = job.dispositions.find(&job.requisition.id, application_id) else {
            block(
                job,
                blocked_log,
                &mut report,
                application_id,
                Some(&applicant.email),
                BlockReason::MissingDisposition,
            )?;
            continue;
        };

        if record.candidate_email != applicant.email {
            block(
                job,
                blocked_log,
                &mut report,
                application_id,
                Some(&applicant.email),
                BlockReason::EmailMismatch {
                    export: applicant.email.clone(),
                    form: record.candidate_email.clone(),
                },
            )?;
            continue;
        }

        if let DispositionCategory::FailedGate { gate, answer } = &record.category {
            if !job.requisition.gates.contains(gate) {
                block(
                    job,
                    blocked_log,
                    &mut report,
                    application_id,
                    Some(&applicant.email),
                    BlockReason::GateNotInRequisition { gate: *gate },
                )?;
                continue;
            }
            if let Some(declared) = applicant.knockout(*gate)
                && declared != *answer
            {
                block(
                    job,
                    blocked_log,
                    &mut report,
                    application_id,
                    Some(&applicant.email),
                    BlockReason::AnswerMismatch {
                        gate: *gate,
                        declared,
                        recorded: *answer,
                    },
                )?;
                continue;
            }
        }
        record.category.validate_against(job.requisition)?;

        let gate_copy = match &record.category {
            DispositionCategory::FailedGate { gate, .. } => Some(job.library.require(*gate)?),
            _ => None,
        };
        let context = MergeContext {
            candidate_name: &applicant.name,
            requisition: job.requisition,
            disposition: &record.category,
            gate_copy,
        };
        let template_kind = record.category.template_kind();
        let template = job.templates.require(template_kind)?;
        let rendered = render(template, &context)?;

        match job.mode {
            Mode::Preview => report.previews.push(Preview {
                application_id: record.application_id.clone(),
                candidate_email: record.candidate_email.clone(),
                candidate_name: record.candidate_name.clone(),
                category: record.category.clone(),
                rendered,
            }),
            Mode::Execute => {
                let draft = SendDraft {
                    disposition: record.clone(),
                    template: template_kind,
                    rendered,
                };
                report.sent.push(send_log.append(draft, job.now)?);
            }
        }
    }

    Ok(report)
}

/// Records one refusal, and writes it to the log when executing.
fn block(
    job: &Dispatch<'_>,
    blocked_log: &mut BlockedLog,
    report: &mut DispatchReport,
    application_id: &ApplicationId,
    candidate_email: Option<&CandidateEmail>,
    reason: BlockReason,
) -> Result<()> {
    let attempt = BlockedAttempt {
        at: job.now,
        requisition: job.requisition.id.clone(),
        application_id: application_id.clone(),
        candidate_email: candidate_email.cloned(),
        reason,
    };
    let new_block = match job.mode {
        Mode::Execute => blocked_log.record(attempt.clone())?,
        Mode::Preview => !blocked_log.blocked_before(&attempt),
    };
    if new_block {
        report.blocked.push(attempt);
    }
    Ok(())
}
