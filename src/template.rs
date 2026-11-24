//! The three master merge templates (Task 2.4).
//!
//! A template may only use the merge slots in [`SLOTS`], and each template kind
//! must use exactly the slots its disposition can supply. The only candidate-level
//! slot is the name, so "zero per-candidate tailoring" is structural rather than a
//! reviewer instruction: there is no slot a recruiter could put a judgement into.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::approval::{Approval, SignOffRole};
use crate::digest::sha256_hex;
use crate::disposition::DispositionCategory;
use crate::error::{Error, Result};
use crate::gate::GateCopy;
use crate::id::{Mailbox, TemplateKind};
use crate::requisition::Requisition;
use crate::text;

/// Every merge slot a template may use.
pub const SLOTS: [&str; 9] = [
    "candidate_name",
    "role_title",
    "employer",
    "reply_to",
    "gate_requirement",
    "gate_satisfies",
    "gate_flexibility",
    "gate_answer",
    "comparative_paragraph",
];

/// The administrative template's slot, listed separately because it is only
/// meaningful for closures.
const CLOSURE_SLOT: &str = "closure_reason";

/// Longest template body the pilot accepts.
const MAX_BODY_CHARS: usize = 8_000;

/// The slots each template kind must contain.
fn required_slots(kind: TemplateKind) -> &'static [&'static str] {
    match kind {
        TemplateKind::FailedGate => &[
            "candidate_name",
            "role_title",
            "employer",
            "gate_requirement",
            "gate_answer",
            "gate_satisfies",
            "gate_flexibility",
            "reply_to",
        ],
        TemplateKind::Comparative => &[
            "candidate_name",
            "role_title",
            "employer",
            "comparative_paragraph",
            "reply_to",
        ],
        TemplateKind::Administrative => &[
            "candidate_name",
            "role_title",
            "employer",
            "closure_reason",
            "reply_to",
        ],
    }
}

/// One approved-or-draft email template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailTemplate {
    /// Which disposition category this template answers.
    pub kind: TemplateKind,
    /// The subject line, which may use merge slots.
    pub subject: String,
    /// The body, which may use merge slots.
    pub body: String,
    /// Hiring-manager and legal sign-off for this wording.
    pub approval: Approval,
}

impl EmailTemplate {
    /// Validates and builds a template.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for a blank or over-long subject or body, an
    /// unknown merge slot, or a required slot that is missing; and
    /// [`Error::UnclosedSlot`] for an unterminated slot.
    pub fn try_new(
        kind: TemplateKind,
        subject: impl Into<String>,
        body: impl Into<String>,
    ) -> Result<Self> {
        let subject = text::required("template subject", subject.into(), 200)?;
        if subject.contains('\n') || subject.contains('\r') {
            return Err(Error::invalid("template subject", "must be a single line"));
        }
        let body = text::required("template body", body.into(), MAX_BODY_CHARS)?;

        let mut used = parse_slots(&subject)?;
        used.extend(parse_slots(&body)?);
        for slot in &used {
            if !is_known_slot(slot) {
                return Err(Error::UnknownSlot((*slot).to_owned()));
            }
        }
        for slot in required_slots(kind) {
            if !used.contains(slot) {
                return Err(Error::invalid(
                    "template",
                    format!("{} template must contain {{{{{slot}}}}}", kind),
                ));
            }
        }

        Ok(Self {
            kind,
            subject,
            body,
            approval: Approval::default(),
        })
    }
}

/// Whether `slot` is a permitted merge slot.
fn is_known_slot(slot: &str) -> bool {
    SLOTS.contains(&slot) || slot == CLOSURE_SLOT
}

/// Returns every slot name used in `text`.
fn parse_slots(text: &str) -> Result<Vec<&str>> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::UnclosedSlot(text.len() - rest.len()));
        };
        names.push(after[..end].trim());
        rest = &after[end + 2..];
    }
    Ok(names)
}

/// The three master templates plus their sign-off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Templates {
    /// The three templates, one per disposition category.
    pub items: Vec<EmailTemplate>,
}

impl Templates {
    /// The seeded draft copy, pending sign-off.
    ///
    /// The wording follows the plan: it names the requirement (never an
    /// allegation), states what would satisfy it, offers a monitored human
    /// reply path, and makes no individual adverse statement.
    pub fn seed() -> Self {
        let failed_gate_body = "\
Dear {{candidate_name}},

Thank you for applying for {{role_title}} at {{employer}}.

Your application did not meet a requirement that was stated in the advertisement:
{{gate_requirement}}

In your application you indicated {{gate_answer}}.
What would meet the requirement: {{gate_satisfies}}.
{{gate_flexibility}}

This decision was made by a person on the information in your application, and was not \
produced by an automated assessment. If the information above is wrong, or you would like to \
discuss it, reply to this email and a person will respond: {{reply_to}}.

Regards,
{{employer}} recruitment team";

        let comparative_body = "\
Dear {{candidate_name}},

Thank you for applying for {{role_title}} at {{employer}}.

You met the requirements for this role. We received a strong field of applications and \
appointed a candidate whose application showed more of the following:

{{comparative_paragraph}}

Your application was assessed only against the requirements and the attributes described above. \
This decision was made by a person and was not produced by an automated assessment. If you would \
like to discuss the feedback, reply to this email and a person will respond: {{reply_to}}.

Regards,
{{employer}} recruitment team";

        let administrative_body = "\
Dear {{candidate_name}},

Thank you for your interest in {{role_title}} at {{employer}}.

This application was closed for an administrative reason: {{closure_reason}}. Your application \
was not assessed for merit, and this decision was not produced by an automated assessment.

If you would like to be considered for future roles, reply to this email and a person will \
respond: {{reply_to}}.

Regards,
{{employer}} recruitment team";

        Self {
            items: vec![
                EmailTemplate {
                    kind: TemplateKind::FailedGate,
                    subject: "Update on your application for {{role_title}}".to_owned(),
                    body: failed_gate_body.to_owned(),
                    approval: Approval::default(),
                },
                EmailTemplate {
                    kind: TemplateKind::Comparative,
                    subject: "Update on your application for {{role_title}}".to_owned(),
                    body: comparative_body.to_owned(),
                    approval: Approval::default(),
                },
                EmailTemplate {
                    kind: TemplateKind::Administrative,
                    subject: "Update on your application for {{role_title}}".to_owned(),
                    body: administrative_body.to_owned(),
                    approval: Approval::default(),
                },
            ],
        }
    }

    /// Returns the template of `kind`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingTemplate`] when the set does not contain it.
    pub fn require(&self, kind: TemplateKind) -> Result<&EmailTemplate> {
        self.items
            .iter()
            .find(|item| item.kind == kind)
            .ok_or(Error::MissingTemplate(kind))
    }

    /// Records a single-pass signature on all three templates.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingTemplate`] when a kind is absent, a validation
    /// error for a blank actor, and [`Error::AlreadySignedOff`] on a repeat.
    pub fn approve_all(
        &mut self,
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
    ) -> Result<()> {
        let actor = actor.into();
        for kind in TemplateKind::ALL {
            let template = self
                .items
                .iter_mut()
                .find(|item| item.kind == kind)
                .ok_or(Error::MissingTemplate(kind))?;
            template.approval.record(actor.trim(), role, at)?;
        }
        Ok(())
    }

    /// Whether every template exists and has both required signatures.
    pub fn is_approved(&self) -> bool {
        TemplateKind::ALL.into_iter().all(|kind| {
            self.require(kind)
                .is_ok_and(|item| item.approval.is_approved())
        })
    }

    /// Saves the templates to the workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &crate::store::Store) -> Result<()> {
        store.save(crate::store::TEMPLATES, self)
    }

    /// Whether the set holds all three templates, approved or not.
    pub fn is_complete(&self) -> bool {
        TemplateKind::ALL
            .into_iter()
            .all(|kind| self.require(kind).is_ok())
    }
}

/// Everything one rendered email is allowed to know about the candidate.
#[derive(Debug)]
pub struct MergeContext<'a> {
    /// The candidate's name, the only candidate-specific value.
    pub candidate_name: &'a str,
    /// The requisition, supplying title, employer, mailbox, and comparative text.
    pub requisition: &'a Requisition,
    /// The captured category, supplying the gate answer or closure reason.
    pub disposition: &'a DispositionCategory,
    /// The pre-approved gate copy, required for a failed-gate email.
    pub gate_copy: Option<&'a GateCopy>,
}

/// A fully merged email, ready to dispatch and to log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedEmail {
    /// The merged subject line.
    pub subject: String,
    /// The merged body.
    pub body: String,
}

impl RenderedEmail {
    /// The SHA-256 of the exact bytes sent, for the send log.
    pub fn digest(&self) -> String {
        sha256_hex(format!("{}\n{}", self.subject, self.body).as_bytes())
    }
}

impl fmt::Display for RenderedEmail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Subject: {}\n\n{}", self.subject, self.body)
    }
}

/// Merges one template with one captured disposition.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the template does not match the disposition's
/// category, [`Error::MissingGateCopy`] when a failed-gate email has no approved
/// gate copy, [`Error::UnknownSlot`] for an unknown slot, [`Error::UnclosedSlot`]
/// for an unterminated slot, and [`Error::MissingSlotValue`] when a slot cannot be
/// filled from this disposition.
pub fn render(template: &EmailTemplate, context: &MergeContext<'_>) -> Result<RenderedEmail> {
    if template.kind != context.disposition.template_kind() {
        return Err(Error::invalid(
            "template",
            format!(
                "{} template cannot render a {} disposition",
                template.kind,
                context.disposition.code()
            ),
        ));
    }
    Ok(RenderedEmail {
        subject: merge(&template.subject, context)?,
        body: merge(&template.body, context)?,
    })
}

/// Single-pass slot substitution.
fn merge(text: &str, context: &MergeContext<'_>) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(start) = rest.find("{{") else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::UnclosedSlot(text.len() - rest.len()));
        };
        let slot = after[..end].trim();
        out.push_str(&slot_value(slot, context)?);
        rest = &after[end + 2..];
    }
}

/// Resolves one slot to the exact text it contributes.
fn slot_value(slot: &str, context: &MergeContext<'_>) -> Result<String> {
    let requisition = context.requisition;
    let value = match slot {
        "candidate_name" => context.candidate_name.to_owned(),
        "role_title" => requisition.title.clone(),
        "employer" => requisition.employer.clone(),
        "reply_to" => mailbox_text(&requisition.reply_to),
        "gate_requirement" => gate_copy(context)?.requirement.clone(),
        "gate_satisfies" => gate_copy(context)?.what_satisfies.clone(),
        "gate_flexibility" => gate_copy(context)?.flexibility.sentence(),
        "gate_answer" => match context.disposition {
            DispositionCategory::FailedGate { answer, .. } => format!("that {}", answer.phrase()),
            _ => return Err(missing_slot(slot)),
        },
        "comparative_paragraph" => match &requisition.comparative {
            Some(paragraph) => paragraph.text.clone(),
            None => {
                return Err(Error::invalid(
                    "comparative paragraph",
                    "the requisition has no approved comparative paragraph",
                ));
            }
        },
        CLOSURE_SLOT => match context.disposition {
            DispositionCategory::AdministrativeClosure { reason } => reason.phrase().to_owned(),
            _ => return Err(missing_slot(slot)),
        },
        other => return Err(Error::UnknownSlot(other.to_owned())),
    };
    Ok(value)
}

/// Returns the gate copy, or a validation error when this email needs one.
fn gate_copy<'a>(context: &MergeContext<'a>) -> Result<&'a GateCopy> {
    match (context.gate_copy, context.disposition) {
        (Some(copy), _) => Ok(copy),
        (None, DispositionCategory::FailedGate { gate, .. }) => Err(Error::MissingGateCopy(*gate)),
        (None, _) => Err(Error::invalid(
            "gate copy",
            "a gate slot was used by a non-gate disposition",
        )),
    }
}

fn missing_slot(slot: &str) -> Error {
    Error::MissingSlotValue {
        slot: slot.to_owned(),
    }
}

/// Renders the mailbox without its display wrapper, for the email body.
fn mailbox_text(mailbox: &Mailbox) -> String {
    mailbox.as_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{GateAnswer, GateKind, GateLibrary};
    use crate::id::RequisitionId;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 9, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn requisition() -> Requisition {
        let mut requisition = Requisition::new(
            RequisitionId::try_from("REQ-1").expect("id"),
            "Example Health",
            "Registered Nurse",
            Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
            [GateKind::VisaRestrictions],
            at(),
        )
        .expect("valid requisition");
        requisition
            .set_comparative("Shortlisted applicants had three years of acute care.")
            .expect("paragraph");
        requisition
    }

    fn context<'a>(
        category: &'a DispositionCategory,
        library: &'a GateLibrary,
        requisition: &'a Requisition,
    ) -> MergeContext<'a> {
        let gate_copy = match category {
            DispositionCategory::FailedGate { gate, .. } => library.copy_for(*gate),
            _ => None,
        };
        MergeContext {
            candidate_name: "Ada Lovelace",
            requisition,
            disposition: category,
            gate_copy,
        }
    }

    #[test]
    fn seeded_templates_validate() {
        let templates = Templates::seed();
        assert!(templates.is_complete());
        assert!(!templates.is_approved());
        for kind in TemplateKind::ALL {
            let template = templates.require(kind).expect("seeded template");
            assert_eq!(template.kind, kind);
        }
    }

    #[test]
    fn every_emails_reply_to_is_the_monitored_mailbox() {
        let library = GateLibrary::seed();
        let requisition = requisition();
        let templates = Templates::seed();
        let categories = [
            DispositionCategory::FailedGate {
                gate: GateKind::VisaRestrictions,
                answer: GateAnswer::Restricted,
            },
            DispositionCategory::MetTheBarStrongerField,
            DispositionCategory::AdministrativeClosure {
                reason: crate::disposition::ClosureReason::RoleChange,
            },
        ];
        for category in categories {
            let context = context(&category, &library, &requisition);
            let rendered = render(
                templates
                    .require(category.template_kind())
                    .expect("template"),
                &context,
            )
            .expect("renders");
            assert!(
                rendered.body.contains("careers-feedback@example.com.au"),
                "{}",
                rendered.body
            );
            assert!(
                !rendered.body.contains("{{"),
                "unmerged slot left in {}",
                rendered.body
            );
        }
    }

    #[test]
    fn comparative_emails_are_identical_for_every_candidate() {
        let library = GateLibrary::seed();
        let requisition = requisition();
        let templates = Templates::seed();
        let category = DispositionCategory::MetTheBarStrongerField;
        let template = templates
            .require(TemplateKind::Comparative)
            .expect("template");

        let mut first = context(&category, &library, &requisition);
        first.candidate_name = "Ada Lovelace";
        let mut second = context(&category, &library, &requisition);
        second.candidate_name = "Ada Lovelace";
        assert_eq!(
            render(template, &first).expect("first"),
            render(template, &second).expect("second")
        );
    }

    #[test]
    fn unknown_slots_are_rejected_at_construction() {
        let result = EmailTemplate::try_new(
            TemplateKind::Comparative,
            "Update",
            "Dear {{candidate_name}}, {{recruiter_opinion}}",
        );
        assert!(matches!(result, Err(Error::UnknownSlot(slot)) if slot == "recruiter_opinion"));
    }

    #[test]
    fn missing_required_slot_is_rejected() {
        let result = EmailTemplate::try_new(
            TemplateKind::FailedGate,
            "Update",
            "Dear {{candidate_name}}",
        );
        assert!(result.is_err());
    }

    #[test]
    fn template_kind_must_match_the_disposition() {
        let library = GateLibrary::seed();
        let requisition = requisition();
        let templates = Templates::seed();
        let category = DispositionCategory::MetTheBarStrongerField;
        let context = context(&category, &library, &requisition);
        let gate_template = templates
            .require(TemplateKind::FailedGate)
            .expect("template");
        assert!(render(gate_template, &context).is_err());
    }

    #[test]
    fn digest_changes_when_the_text_changes() {
        let first = RenderedEmail {
            subject: "Update".to_owned(),
            body: "Body".to_owned(),
        };
        let second = RenderedEmail {
            subject: "Update".to_owned(),
            body: "Body.".to_owned(),
        };
        assert_ne!(first.digest(), second.digest());
    }
}
