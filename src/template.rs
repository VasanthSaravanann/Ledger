//! The three master templates and the candidate-value signal (Task 2.4).
//!
//! `Template A` renders the gate text block, `Template B` renders the role-level
//! paragraph and must retain the paragraph declining individual feedback,
//! `Template C` renders one of the three exact administrative strings. Every
//! template carries a `TEXT_VERSION` and must be signed off once, in the same
//! single pass as the gate library and the playbook.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::approval::{Approval, SignOffRole};
use crate::digest::sha256_hex;
use crate::disposition::Disposition;
use crate::error::{Error, Result};
use crate::gate::GateText;
use crate::id::{Mailbox, TemplateKind, TextVersion};
use crate::requisition::Requisition;
use crate::text;

/// Every merge slot a template may use.
pub const SLOTS: [&str; 12] = [
    "CANDIDATE_NAME",
    "ROLE_TITLE",
    "EMPLOYER",
    "SENDER_NAME",
    "SENDER_TITLE",
    "REPLY_TO",
    "GATE_TEXT_BLOCK",
    "ROLE_LEVEL_PARAGRAPH",
    "ADMIN_REASON",
    "REENGAGEMENT_LINE",
    "VALUE_LINKS",
    "FEEDBACK_DECLINE",
];

/// The fixed paragraph that declines individual feedback on `FIELD` emails.
pub const FEEDBACK_DECLINE: &str = "We do not provide individual feedback for this role: this \
                                     decision identified no individual shortfall. The paragraph \
                                     above describes what the shortlisted field demonstrated.";

/// The fixed re-engagement line.
pub const REENGAGEMENT_LINE: &str = "If you would like to be considered for future roles, reply \
                                     to this email and we will keep your details on file with \
                                     your consent.";

/// Longest template body the pilot accepts.
const MAX_BODY_CHARS: usize = 8_000;

/// The slots each template kind must contain.
fn required_slots(kind: TemplateKind) -> &'static [&'static str] {
    match kind {
        TemplateKind::Gate => &[
            "CANDIDATE_NAME",
            "ROLE_TITLE",
            "EMPLOYER",
            "SENDER_NAME",
            "SENDER_TITLE",
            "REPLY_TO",
            "GATE_TEXT_BLOCK",
            "VALUE_LINKS",
        ],
        TemplateKind::Field => &[
            "CANDIDATE_NAME",
            "ROLE_TITLE",
            "EMPLOYER",
            "SENDER_NAME",
            "SENDER_TITLE",
            "REPLY_TO",
            "ROLE_LEVEL_PARAGRAPH",
            "FEEDBACK_DECLINE",
            "REENGAGEMENT_LINE",
            "VALUE_LINKS",
        ],
        TemplateKind::Admin => &[
            "CANDIDATE_NAME",
            "ROLE_TITLE",
            "EMPLOYER",
            "SENDER_NAME",
            "SENDER_TITLE",
            "REPLY_TO",
            "ADMIN_REASON",
            "REENGAGEMENT_LINE",
            "VALUE_LINKS",
        ],
    }
}

/// One versioned, approvable email template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailTemplate {
    /// Which disposition this template answers.
    pub kind: TemplateKind,
    /// Immutable `TEXT_VERSION` tag.
    pub version: TextVersion,
    /// The subject line.
    pub subject: String,
    /// The body.
    pub body: String,
    /// Hiring-manager and legal sign-off.
    pub approval: Approval,
}

impl EmailTemplate {
    /// The SHA-256 of the template's copy, excluding its approval.
    pub fn content_hash(&self) -> String {
        let projection = serde_json::json!({
            "kind": self.kind.code(),
            "version": self.version.as_str(),
            "subject": self.subject,
            "body": self.body,
        });
        serde_json::to_vec(&projection)
            .map(|bytes| crate::digest::sha256_hex(&bytes))
            .unwrap_or_default()
    }

    /// Validates and builds a template.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for a blank or over-long subject or body, an
    /// unknown slot, or a missing required slot; and [`Error::UnclosedSlot`] for
    /// an unterminated slot.
    pub fn try_new(
        kind: TemplateKind,
        version: TextVersion,
        subject: impl Into<String>,
        body: impl Into<String>,
    ) -> Result<Self> {
        let subject = text::required("template subject", subject.into(), 200)?;
        if subject.contains('\n') || subject.contains('\r') {
            return Err(Error::invalid("template subject", "must be a single line"));
        }
        let body = text::required("template body", body.into(), MAX_BODY_CHARS)?;

        let mut used = slots(&subject)?;
        used.extend(slots(&body)?);
        for slot in &used {
            if !SLOTS.contains(slot) {
                return Err(Error::UnknownSlot((*slot).to_owned()));
            }
        }
        for slot in required_slots(kind) {
            if !used.contains(slot) {
                return Err(Error::invalid(
                    "template",
                    format!("Template {} must contain {{{{{slot}}}}}", kind.letter()),
                ));
            }
        }

        Ok(Self {
            kind,
            version,
            subject,
            body,
            approval: Approval::default(),
        })
    }
}

/// Returns every slot name used in `text`.
fn slots(text: &str) -> Result<Vec<&str>> {
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

/// The three master templates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Templates {
    /// One template per disposition.
    pub items: Vec<EmailTemplate>,
}

impl Templates {
    /// The seeded copy, at `v1.0`, pending sign-off.
    pub fn seed() -> Self {
        let version = TextVersion::parse("v1.0").expect("default version is valid");
        let gate_body = "\
Dear {{CANDIDATE_NAME}},

Thank you for applying for {{ROLE_TITLE}} at {{EMPLOYER}}.

Your application did not meet a requirement stated in the advertisement:
{{GATE_TEXT_BLOCK}}

In your application you indicated a position that does not meet this requirement. If the \
information above is wrong, reply to this email and a person will correct the record and \
reconsider your application: {{REPLY_TO}}.

This decision was made by a person on the information in your application, and was not produced \
by an automated assessment.

{{VALUE_LINKS}}

Regards,
{{SENDER_NAME}}
{{SENDER_TITLE}}, {{EMPLOYER}}";

        let field_body = "\
Dear {{CANDIDATE_NAME}},

Thank you for applying for {{ROLE_TITLE}} at {{EMPLOYER}}.

You met the requirements stated for this role. We appointed a candidate whose application \
demonstrated more of the following:

{{ROLE_LEVEL_PARAGRAPH}}

{{FEEDBACK_DECLINE}}

This decision was made by a person on the information in your application, and was not produced \
by an automated assessment.

{{REENGAGEMENT_LINE}}

{{VALUE_LINKS}}

Regards,
{{SENDER_NAME}}
{{SENDER_TITLE}}, {{EMPLOYER}}";

        let admin_body = "\
Dear {{CANDIDATE_NAME}},

Thank you for your interest in {{ROLE_TITLE}} at {{EMPLOYER}}.

This application was closed for a reason unrelated to you: {{ADMIN_REASON}}. Your application \
was not assessed for merit, and this decision was not produced by an automated assessment.

{{REENGAGEMENT_LINE}}

{{VALUE_LINKS}}

Regards,
{{SENDER_NAME}}
{{SENDER_TITLE}}, {{EMPLOYER}}";

        Self {
            items: vec![
                EmailTemplate {
                    kind: TemplateKind::Gate,
                    version: version.clone(),
                    subject: "Update on your application for {{ROLE_TITLE}}".to_owned(),
                    body: gate_body.to_owned(),
                    approval: Approval::default(),
                },
                EmailTemplate {
                    kind: TemplateKind::Field,
                    version: version.clone(),
                    subject: "Update on your application for {{ROLE_TITLE}}".to_owned(),
                    body: field_body.to_owned(),
                    approval: Approval::default(),
                },
                EmailTemplate {
                    kind: TemplateKind::Admin,
                    version,
                    subject: "Update on your application for {{ROLE_TITLE}}".to_owned(),
                    body: admin_body.to_owned(),
                    approval: Approval::default(),
                },
            ],
        }
    }

    /// Returns the template for `kind`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingTemplate`] when the set lacks it.
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
    /// Returns [`Error::MissingTemplate`] for an absent kind, [`Error::Invalid`]
    /// for a blank actor, and [`Error::AlreadySignedOff`] on a repeat.
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
            let hash = template.content_hash();
            template.approval.record(actor.trim(), role, at, hash)?;
        }
        Ok(())
    }

    /// Attaches an operator signature to every template's sign-off for `role`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingTemplate`] for an absent kind, and propagates
    /// signature errors.
    pub fn sign_all(
        &mut self,
        registry: &crate::identity::Registry,
        store: &crate::store::Store,
        role: SignOffRole,
        operator: &str,
    ) -> Result<()> {
        for kind in TemplateKind::ALL {
            let template = self
                .items
                .iter_mut()
                .find(|item| item.kind == kind)
                .ok_or(Error::MissingTemplate(kind))?;
            template.approval.sign(role, registry, store, operator)?;
        }
        Ok(())
    }

    /// Whether every template is signed off by HM and Legal.
    pub fn is_approved(&self) -> bool {
        TemplateKind::ALL.into_iter().all(|kind| {
            self.require(kind)
                .is_ok_and(|item| item.approval.is_bound_to(&item.content_hash()))
        })
    }

    /// Whether the set holds all three templates.
    pub fn is_complete(&self) -> bool {
        TemplateKind::ALL
            .into_iter()
            .all(|kind| self.require(kind).is_ok())
    }

    /// Saves the templates to the workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &crate::store::Store) -> Result<()> {
        store.save(crate::store::TEMPLATES, self)
    }
}

/// Everything one rendered email is allowed to know about the candidate.
#[derive(Debug)]
pub struct MergeContext<'a> {
    /// The candidate's first name, the only candidate-specific value.
    pub candidate_first_name: &'a str,
    /// The requisition, supplying title, employer, sender, and mailbox.
    pub requisition: &'a Requisition,
    /// The captured disposition.
    pub disposition: &'a Disposition,
    /// The gate block, required for a `GATE` email.
    pub gate_text: Option<&'a GateText>,
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
    /// The SHA-256 of the exact bytes sent, for `RENDERED_BODY_HASH`.
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
/// Returns [`Error::Invalid`] when the template does not match the disposition,
/// [`Error::MissingSlotValue`] when a slot cannot be filled, and
/// [`Error::UnknownSlot`] for an unknown slot.
pub fn render(template: &EmailTemplate, context: &MergeContext<'_>) -> Result<RenderedEmail> {
    let expected = match context.disposition {
        Disposition::Gate { .. } => TemplateKind::Gate,
        Disposition::Field => TemplateKind::Field,
        Disposition::Admin { .. } => TemplateKind::Admin,
    };
    if template.kind != expected {
        return Err(Error::invalid(
            "template",
            format!(
                "Template {} cannot render a {} disposition",
                template.kind.letter(),
                context.disposition.kind()
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
        "CANDIDATE_NAME" => context.candidate_first_name.to_owned(),
        "ROLE_TITLE" => requisition.title.clone(),
        "EMPLOYER" => requisition.employer.clone(),
        "SENDER_NAME" => requisition.sender.name.clone(),
        "SENDER_TITLE" => requisition.sender.title.clone(),
        "REPLY_TO" => requisition.reply_to.as_str().to_owned(),
        "GATE_TEXT_BLOCK" => match context.gate_text {
            Some(block) => block.block.clone(),
            None => return Err(missing_slot(slot)),
        },
        "ROLE_LEVEL_PARAGRAPH" => match &requisition.comparative {
            Some(paragraph) => paragraph.text(),
            None => {
                return Err(Error::invalid(
                    "comparative paragraph",
                    "the requisition has no raised role-level paragraph",
                ));
            }
        },
        "ADMIN_REASON" => match context.disposition {
            Disposition::Admin { reason } => reason.sentence().to_owned(),
            _ => return Err(missing_slot(slot)),
        },
        "REENGAGEMENT_LINE" => REENGAGEMENT_LINE.to_owned(),
        "FEEDBACK_DECLINE" => FEEDBACK_DECLINE.to_owned(),
        "VALUE_LINKS" => value_links(&requisition.reply_to),
        other => return Err(Error::UnknownSlot(other.to_owned())),
    };
    Ok(value)
}

/// Builds the one-click value-signal block. No free text, no per-candidate text.
fn value_links(mailbox: &Mailbox) -> String {
    let address = mailbox.as_str();
    let link = |label: &str, signal: &str| {
        format!("{label}: mailto:{address}?subject=ledger-signal-{signal}")
    };
    format!(
        "Was this useful for your next application?\n{}\n{}\n{}",
        link("Yes", "yes"),
        link("Somewhat", "somewhat"),
        link("No", "no")
    )
}

fn missing_slot(slot: &str) -> Error {
    Error::MissingSlotValue {
        slot: slot.to_owned(),
    }
}
