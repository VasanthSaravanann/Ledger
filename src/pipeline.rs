//! Manual dispatch under the three circuit-breakers (Tasks 3.3 and 4.1).
//!
//! Before an email is rendered the pipeline enforces, in order: a locked
//! disposition exists; the disposition agrees with the ATS export; the gate is
//! active and its copy resolves; the second-person QA check is recorded; and the
//! text resolves to a signed-off version. Every refusal is logged, which is what
//! makes the form-block and gate-mismatch metrics measurable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ats::Roster;
use crate::disposition::{Disposition, Dispositions};
use crate::error::{Error, Result};
use crate::gate::GateLibrary;
use crate::id::{
    ApplicationId, CandidateEmail, DispositionId, RequisitionId, TemplateKind, TextVersion,
};
use crate::log::{
    BlockReason, BlockedAttempt, BlockedLog, DeliveryStatus, SendDraft, SendEntry, SendLog,
};
use crate::playbook::Playbook;
use crate::qa::{QaLog, requires_second_person, scope_for};
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
    /// The recipient's first name.
    pub candidate_first_name: String,
    /// The disposition record that authorises the send.
    pub disposition_id: DispositionId,
    /// The disposition itself.
    pub disposition: Disposition,
    /// The version of the copy that produced the text.
    pub text_version: TextVersion,
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
    /// The signed-off gate copy library.
    pub library: &'a GateLibrary,
    /// The signed-off templates.
    pub templates: &'a Templates,
    /// The signed-off reply playbook.
    pub playbook: &'a Playbook,
    /// The imported ATS roster.
    pub roster: &'a Roster,
    /// Captured dispositions.
    pub dispositions: &'a Dispositions,
    /// Recorded second-person checks.
    pub qa: &'a QaLog,
    /// The delivery status to record on each send.
    pub delivery_status: DeliveryStatus,
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
    /// Attempts refused by the circuit-breakers.
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

    /// How many emails were rendered in preview mode.
    pub fn preview_count(&self) -> usize {
        self.previews.len()
    }
}

/// Dispatches feedback for every applicant that has not already been sent to.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the roster belongs to another requisition,
/// [`Error::RequisitionNotReady`] when the requisition's copy is unsigned,
/// [`Error::TemplatesNotApproved`] or [`Error::PlaybookNotApproved`] before
/// sign-off, and propagates template and log errors.
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
    if !job.playbook.is_approved() {
        return Err(Error::PlaybookNotApproved);
    }

    let mut report = DispatchReport::default();
    for applicant in &job.roster.applicants {
        let application_id = &applicant.application_id;
        if send_log.already_sent(&job.requisition.id, application_id) {
            continue;
        }

        let Some(record) = job.dispositions.head(&job.requisition.id, application_id) else {
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

        if record.form.candidate_email != applicant.email {
            block(
                job,
                blocked_log,
                &mut report,
                application_id,
                Some(&applicant.email),
                BlockReason::EmailMismatch {
                    export: applicant.email.clone(),
                    form: record.form.candidate_email.clone(),
                },
            )?;
            continue;
        }

        let gate_text = match &record.form.disposition {
            Disposition::Gate { gate, answer } => {
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
                // Failure: the gate's requisition facts are undocumented.
                if !job
                    .library
                    .missing_facts(*gate, &job.requisition.facts)
                    .is_empty()
                {
                    block(
                        job,
                        blocked_log,
                        &mut report,
                        application_id,
                        Some(&applicant.email),
                        BlockReason::GateUnavailable { gate: *gate },
                    )?;
                    continue;
                }

                // Failure: the form contradicts the candidate's own ATS answer.
                // Nested deliberately: `if let ... && ...` needs Rust 1.88 and the
                // crate's declared MSRV is 1.85.
                if let Some(declared) = applicant.knockout(*gate) {
                    if declared != *answer {
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

                match job.library.copy_for(*gate) {
                    Some(copy) => Some(copy.render(&job.requisition.facts)?),
                    None => {
                        block(
                            job,
                            blocked_log,
                            &mut report,
                            application_id,
                            Some(&applicant.email),
                            BlockReason::GateUnavailable { gate: *gate },
                        )?;
                        continue;
                    }
                }
            }
            _ => None,
        };

        if requires_second_person(record) && !job.qa.approved(&job.requisition.id, application_id) {
            block(
                job,
                blocked_log,
                &mut report,
                application_id,
                Some(&applicant.email),
                BlockReason::AwaitingSecondPersonQa {
                    scope: scope_for(&record.form.disposition),
                },
            )?;
            continue;
        }

        let template_kind = record.form.disposition.kind().template_kind();
        let template = job.templates.require(template_kind)?;
        let rendered = render(
            template,
            &MergeContext {
                candidate_first_name: &record.form.candidate_first_name,
                requisition: job.requisition,
                disposition: &record.form.disposition,
                gate_text: gate_text.as_ref(),
            },
        )?;

        // Version resolution: the logged TEXT_VERSION is the copy that rendered.
        let text_version = match &record.form.disposition {
            Disposition::Gate { gate, .. } => job.library.require(*gate)?.version.clone(),
            Disposition::Field => job
                .requisition
                .comparative
                .as_ref()
                .ok_or_else(|| {
                    Error::invalid(
                        "comparative paragraph",
                        "the requisition has no role-level paragraph",
                    )
                })?
                .version
                .clone(),
            Disposition::Admin { .. } => template.version.clone(),
        };

        match job.mode {
            Mode::Preview => report.previews.push(Preview {
                application_id: record.form.application_id.clone(),
                candidate_email: record.form.candidate_email.clone(),
                candidate_first_name: record.form.candidate_first_name.clone(),
                disposition_id: record.id.clone(),
                disposition: record.form.disposition.clone(),
                text_version,
                rendered,
            }),
            Mode::Execute => {
                let draft = SendDraft {
                    disposition: record.clone(),
                    text_version,
                    rendered,
                    delivery_status: job.delivery_status,
                };
                report.sent.push(send_log.append(draft, job.now)?);
                maybe_abort(report.sent.len());
            }
        }

        // Both branches name `text_version`; keep the binding used exactly once.
        debug_assert!(template_kind == record.form.disposition.kind().template_kind());
    }

    Ok(report)
}

/// Test-only crash injection: `LEDGER_ABORT_AFTER_SENDS=n` aborts the process once
/// `n` sends are committed, so the recovery path can be tested against a real
/// half-finished dispatch rather than a simulation.
fn maybe_abort(completed: usize) {
    if let Ok(limit) = std::env::var("LEDGER_ABORT_AFTER_SENDS") {
        if let Ok(limit) = limit.parse::<usize>() {
            if completed >= limit {
                std::process::abort();
            }
        }
    }
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

/// Whether the roster belongs to `requisition`.
pub fn roster_matches(roster: &Roster, requisition: &RequisitionId) -> bool {
    &roster.requisition == requisition
}

/// The template the disposition requires, for report rendering.
pub fn template_for(disposition: &Disposition) -> TemplateKind {
    disposition.kind().template_kind()
}
