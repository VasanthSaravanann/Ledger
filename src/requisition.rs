//! Requisitions, their documented facts, and the linted comparative paragraph.
//!
//! V2 moves the gate copy's variables onto the requisition: `GATE-02` needs the
//! role's end date or minimum term, `GATE-03` needs a regulator URL, `GATE-04`
//! needs an inherent-requirement justification, and `GATE-05` needs the location
//! and waiver text — all documented *before* the requisition posts. A gate whose
//! facts are missing is unavailable, and the disposition must fall back to `FIELD`.
//!
//! The comparative paragraph is elicited from the Hiring Manager through four
//! questions and is rejected at sign-off when it contains a relative-superiority
//! claim, an implied deficit, a subjective proxy, or a years-of-experience bar.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::approval::{Approval, SignOffRole};
use crate::error::{Error, Result};
use crate::gate::{FactKey, GateKind, GateLibrary};
use crate::id::{Mailbox, RequisitionId, TextVersion};
use crate::text;

/// The named human who signs the email, as `{{SENDER_NAME}}` / `{{SENDER_TITLE}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    /// The sender's name.
    pub name: String,
    /// The sender's title.
    pub title: String,
}

impl Sender {
    /// Builds a sender, rejecting a blank name or title.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for blank or over-long values.
    pub fn try_new(name: impl Into<String>, title: impl Into<String>) -> Result<Self> {
        Ok(Self {
            name: text::required("sender name", name.into(), 120)?,
            title: text::required("sender title", title.into(), 120)?,
        })
    }
}

/// One lint finding on a comparative paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintIssue {
    /// The rule that fired.
    pub rule: &'static str,
    /// The phrase that triggered it.
    pub phrase: String,
}

impl std::fmt::Display for LintIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {:?}", self.rule, self.phrase)
    }
}

/// Banned phrasing, grouped by the audit rule it breaks.
const BANNED: [(&str, &[&str]); 4] = [
    (
        "relative superiority claim",
        &[
            "more experience than",
            "stronger than most",
            "better than most",
            "more than most",
            "more of the following",
            "stronger field",
            "outperformed",
            "best candidate",
        ],
    ),
    (
        "implied candidate deficit",
        &[
            "lacked",
            "typically lacked",
            "missing",
            "failed to show",
            "did not have",
            "weak",
            "poor",
        ],
    ),
    (
        "subjective proxy",
        &[
            "cultural fit",
            "right attitude",
            "team player",
            "personality",
            "vibe",
        ],
    ),
    (
        "years-of-experience bar",
        &[
            "years of experience",
            "years' experience",
            "years experience",
            "years of relevant experience",
        ],
    ),
];

/// The static role-level paragraph for the `FIELD` disposition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparative {
    /// `{{ATTRIBUTE_1}}`.
    pub attribute_1: String,
    /// `{{ATTRIBUTE_2}}`.
    pub attribute_2: String,
    /// `{{ATTRIBUTE_3}}`.
    pub attribute_3: String,
    /// `{{EVIDENCE_PATTERN}}`.
    pub evidence_pattern: String,
    /// `{{NON_FACTORS}}`.
    pub non_factors: String,
    /// Immutable version of this wording.
    pub version: TextVersion,
    /// Sign-off for this wording.
    pub approval: Approval,
}

impl Comparative {
    /// The SHA-256 of the paragraph's copy, excluding its approval.
    pub fn content_hash(&self) -> String {
        let projection = serde_json::json!({
            "attribute_1": self.attribute_1,
            "attribute_2": self.attribute_2,
            "attribute_3": self.attribute_3,
            "evidence_pattern": self.evidence_pattern,
            "non_factors": self.non_factors,
            "version": self.version.as_str(),
        });
        serde_json::to_vec(&projection)
            .map(|bytes| crate::digest::sha256_hex(&bytes))
            .unwrap_or_default()
    }

    /// Builds the paragraph from the Hiring Manager's elicited answers.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for a blank or over-long answer, and
    /// [`Error::ComparativeRejected`] when the wording fails the lint.
    pub fn try_new(
        attributes: [String; 3],
        evidence_pattern: impl Into<String>,
        non_factors: impl Into<String>,
        version: TextVersion,
    ) -> Result<Self> {
        let [attribute_1, attribute_2, attribute_3] = attributes;
        let comparative = Self {
            attribute_1: text::required("attribute 1", attribute_1, 200)?,
            attribute_2: text::required("attribute 2", attribute_2, 200)?,
            attribute_3: text::required("attribute 3", attribute_3, 200)?,
            evidence_pattern: text::required("evidence pattern", evidence_pattern.into(), 400)?,
            non_factors: text::required("non-factors", non_factors.into(), 400)?,
            version,
            approval: Approval::default(),
        };
        let issues = comparative.lint();
        if !issues.is_empty() {
            let joined = issues
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join("; ");
            return Err(Error::ComparativeRejected { issues: joined });
        }
        Ok(comparative)
    }

    /// The rendered `{{ROLE_LEVEL_PARAGRAPH}}`.
    pub fn text(&self) -> String {
        format!(
            "Shortlisted applicants demonstrated {}, {}, and {}. The clearest evidence was {}. \
             Applicants did not need {} to be considered.",
            self.attribute_1,
            self.attribute_2,
            self.attribute_3,
            self.evidence_pattern,
            self.non_factors
        )
    }

    /// Every lint finding on the rendered paragraph.
    pub fn lint(&self) -> Vec<LintIssue> {
        let text = self.text().to_lowercase();
        let mut issues = Vec::new();
        for (rule, phrases) in BANNED {
            for phrase in phrases {
                if text.contains(phrase) {
                    issues.push(LintIssue {
                        rule,
                        phrase: (*phrase).to_owned(),
                    });
                }
            }
        }
        if has_years_bar(&text) {
            issues.push(LintIssue {
                rule: "years-of-experience bar",
                phrase: "a numeric years requirement".to_owned(),
            });
        }
        issues
    }
}

/// Whether the text states a numeric years bar, for example "3 years".
fn has_years_bar(lowered: &str) -> bool {
    let bytes = lowered.as_bytes();
    let mut from = 0;
    while let Some(found) = lowered[from..].find("year") {
        let at = from + found;
        let window_start = at.saturating_sub(12);
        let window = bytes.get(window_start..at).unwrap_or_default();
        if window.iter().any(u8::is_ascii_digit) {
            return true;
        }
        from = at + 4;
    }
    false
}

/// Whether a gate can be used on this requisition, and why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateAvailability {
    /// The gate is screened on and its facts are documented.
    Available,
    /// The gate is screened on but its facts are not documented yet.
    MissingFacts {
        /// The facts still missing.
        missing: Vec<FactKey>,
    },
}

impl GateAvailability {
    /// Whether the gate can carry a `GATE` disposition.
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

/// One open requisition and the copy approved for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requisition {
    /// The requisition's ATS id.
    pub id: RequisitionId,
    /// The pilot employer.
    pub employer: String,
    /// The advertised role title.
    pub title: String,
    /// The monitored reply-to mailbox.
    pub reply_to: Mailbox,
    /// The named human who signs the emails.
    pub sender: Sender,
    /// The knockout gates this requisition screens on.
    pub gates: Vec<GateKind>,
    /// Facts the gate blocks need, documented before the requisition posts.
    pub facts: BTreeMap<FactKey, String>,
    /// The static comparative paragraph, once elicited.
    pub comparative: Option<Comparative>,
    /// Hiring-manager and legal sign-off for the requisition's static copy.
    pub approval: Approval,
    /// When the requisition was created.
    pub created_at: DateTime<Utc>,
}

/// The fields a requisition is created from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequisitionDraft {
    /// The ATS requisition id.
    pub id: RequisitionId,
    /// The employer.
    pub employer: String,
    /// The role title.
    pub title: String,
    /// The monitored reply-to mailbox.
    pub reply_to: Mailbox,
    /// The named sender.
    pub sender: Sender,
    /// The knockout gates.
    pub gates: Vec<GateKind>,
    /// Facts documented before posting.
    pub facts: BTreeMap<FactKey, String>,
}

impl Requisition {
    /// Builds a requisition that screens on at least one gate.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for a blank field, an empty gate list, or a
    /// malformed regulator URL.
    pub fn new(draft: RequisitionDraft, created_at: DateTime<Utc>) -> Result<Self> {
        let employer = text::required("employer", draft.employer, 120)?;
        let title = text::required("role title", draft.title, 120)?;
        let mut unique = Vec::new();
        for gate in draft.gates {
            if !unique.contains(&gate) {
                unique.push(gate);
            }
        }
        unique.sort();
        if unique.is_empty() {
            return Err(Error::invalid(
                "requisition gates",
                "a requisition must screen on at least one gate",
            ));
        }
        let mut requisition = Self {
            id: draft.id,
            employer,
            title,
            reply_to: draft.reply_to,
            sender: draft.sender,
            gates: unique,
            facts: BTreeMap::new(),
            comparative: None,
            approval: Approval::default(),
            created_at,
        };
        for (key, value) in draft.facts {
            requisition.set_fact(key, value)?;
        }
        Ok(requisition)
    }

    /// Documents one requisition fact.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the value is blank, and when a regulator
    /// URL is not an `https` URL.
    pub fn set_fact(&mut self, key: FactKey, value: impl Into<String>) -> Result<()> {
        let value = text::required("requisition fact", value.into(), 400)?;
        if key.is_url() && !value.starts_with("https://") {
            return Err(Error::invalid(
                "requisition fact",
                format!("{key} must be an https URL"),
            ));
        }
        self.facts.insert(key, value);
        Ok(())
    }

    /// Whether `gate` can carry a `GATE` disposition.
    pub fn availability(&self, library: &GateLibrary, gate: GateKind) -> GateAvailability {
        if !self.gates.contains(&gate) {
            return GateAvailability::Available;
        }
        let missing = library.missing_facts(gate, &self.facts);
        if missing.is_empty() {
            GateAvailability::Available
        } else {
            GateAvailability::MissingFacts { missing }
        }
    }

    /// Every gate this requisition screens on, with its availability.
    pub fn availability_report(&self, library: &GateLibrary) -> Vec<(GateKind, GateAvailability)> {
        self.gates
            .iter()
            .map(|gate| (*gate, self.availability(library, *gate)))
            .collect()
    }

    /// Replaces the comparative paragraph, clearing every approval it covered.
    ///
    /// # Errors
    ///
    /// Propagates [`Comparative::try_new`] errors.
    pub fn set_comparative(
        &mut self,
        attributes: [String; 3],
        evidence_pattern: impl Into<String>,
        non_factors: impl Into<String>,
        version: TextVersion,
    ) -> Result<&Comparative> {
        self.comparative = Some(Comparative::try_new(
            attributes,
            evidence_pattern,
            non_factors,
            version,
        )?);
        // The requisition's own copy did not change, so its sign-off stands; the new
        // paragraph simply starts unsigned.
        self.comparative
            .as_ref()
            .ok_or_else(|| Error::invalid("comparative paragraph", "was not stored"))
    }

    /// The SHA-256 of the requisition's copy, excluding approvals and the
    /// comparative paragraph (which is approved separately).
    pub fn content_hash(&self) -> String {
        let facts: BTreeMap<String, &String> = self
            .facts
            .iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        let projection = serde_json::json!({
            "id": self.id.as_str(),
            "employer": self.employer,
            "title": self.title,
            "reply_to": self.reply_to.as_str(),
            "sender": { "name": self.sender.name, "title": self.sender.title },
            "gates": self.gates.iter().map(|gate| gate.code()).collect::<Vec<&str>>(),
            "facts": facts,
        });
        serde_json::to_vec(&projection)
            .map(|bytes| crate::digest::sha256_hex(&bytes))
            .unwrap_or_default()
    }

    /// Records a single-pass signature bound to the current copy.
    ///
    /// # Errors
    ///
    /// Propagates [`Approval::record`] errors.
    pub fn approve(
        &mut self,
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
    ) -> Result<()> {
        let hash = self.content_hash();
        self.approval.record(actor, role, at, hash)?;
        Ok(())
    }

    /// Records a single-pass signature on the comparative paragraph, refusing
    /// wording that fails the audit lint.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when no paragraph has been elicited,
    /// [`Error::ComparativeRejected`] when the lint fails, and propagates
    /// [`Approval::record`] errors.
    pub fn approve_comparative(
        &mut self,
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
    ) -> Result<()> {
        let paragraph = self.comparative.as_mut().ok_or_else(|| {
            Error::invalid(
                "comparative paragraph",
                "elicit the paragraph before collecting sign-off",
            )
        })?;
        let issues = paragraph.lint();
        if !issues.is_empty() {
            return Err(Error::ComparativeRejected {
                issues: issues
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join("; "),
            });
        }
        let hash = paragraph.content_hash();
        paragraph.approval.record(actor, role, at, hash)?;
        Ok(())
    }

    /// Everything that still blocks dispatch, in the order it should be fixed.
    pub fn blocking_issues(&self, library: &GateLibrary) -> Vec<String> {
        let mut issues = Vec::new();
        if !library.is_approved() {
            let missing: Vec<String> = library
                .approval
                .missing()
                .iter()
                .map(ToString::to_string)
                .collect();
            if !library.is_complete() {
                issues.push("gate copy library is incomplete".to_owned());
            } else if !missing.is_empty() {
                issues.push(format!(
                    "gate copy library still needs sign-off from {}",
                    missing.join(", ")
                ));
            }
            issues.extend(library.approval.drift(&library.content_hash()));
        }
        for (gate, availability) in self.availability_report(library) {
            if let GateAvailability::MissingFacts { missing } = availability {
                let names: Vec<String> = missing.iter().map(ToString::to_string).collect();
                issues.push(format!(
                    "{gate} ({}) has no documented facts ({}); GATE dispositions must be \
                     recorded as FIELD until they are set",
                    gate.label(),
                    names.join(", ")
                ));
            }
        }
        issues.extend(self.approval.drift(&self.content_hash()));
        match &self.comparative {
            None => issues.push("comparative paragraph is not elicited".to_owned()),
            Some(paragraph) => {
                let lint = paragraph.lint();
                if !lint.is_empty() {
                    issues.push(format!(
                        "comparative paragraph fails the audit lint: {}",
                        lint.iter()
                            .map(ToString::to_string)
                            .collect::<Vec<String>>()
                            .join("; ")
                    ));
                }
                issues.extend(paragraph.approval.drift(&paragraph.content_hash()));
            }
        }
        issues
    }

    /// Whether the requisition's own copy (not the paragraph) is signed as it stands.
    pub fn is_bound_copy_approved(&self) -> bool {
        self.approval.is_bound_to(&self.content_hash())
    }

    /// Whether the requisition may be used to dispatch feedback.
    pub fn is_ready(&self, library: &GateLibrary) -> bool {
        self.blocking_issues(library).is_empty()
    }
}

/// Every requisition the pilot knows about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequisitionBook {
    items: Vec<Requisition>,
}

impl RequisitionBook {
    /// Loads the book, or an empty one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn load(store: &crate::store::Store) -> Result<Self> {
        Ok(store.load(crate::store::REQUISITIONS)?.unwrap_or_default())
    }

    /// Adds a requisition, rejecting a duplicate id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the id is already in the book.
    pub fn insert(&mut self, requisition: Requisition) -> Result<()> {
        if self.get(&requisition.id).is_some() {
            return Err(Error::invalid(
                "requisition id",
                format!("{} is already in the book", requisition.id),
            ));
        }
        self.items.push(requisition);
        Ok(())
    }

    /// Returns the requisition with `id`.
    pub fn get(&self, id: &RequisitionId) -> Option<&Requisition> {
        self.items.iter().find(|item| &item.id == id)
    }

    /// Returns the requisition with `id` for mutation.
    pub fn get_mut(&mut self, id: &RequisitionId) -> Option<&mut Requisition> {
        self.items.iter_mut().find(|item| &item.id == id)
    }

    /// Returns the requisition with `id`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownRequisition`] when the id is not in the book.
    pub fn require(&self, id: &RequisitionId) -> Result<&Requisition> {
        self.get(id)
            .ok_or_else(|| Error::UnknownRequisition(id.clone()))
    }

    /// Every requisition, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Requisition> {
        self.items.iter()
    }

    /// Whether the book has no requisitions.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{FactKey, GateKind};
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 9, 0, 0)
            .single()
            .expect("timestamp")
    }

    fn version() -> TextVersion {
        TextVersion::parse("v1.0").expect("version")
    }

    fn facts() -> BTreeMap<FactKey, String> {
        BTreeMap::from([
            (FactKey::RoleEndOrMinTerm, "30 June 2027".to_owned()),
            (FactKey::MinTerm, "12 months".to_owned()),
            (
                FactKey::RoleDurationReason,
                "The appointment is fixed-term.".to_owned(),
            ),
            (FactKey::RegulatorUrl, "https://www.ahpra.gov.au".to_owned()),
            (
                FactKey::InherentRequirementReason,
                "the role requires unaccompanied multi-site travel".to_owned(),
            ),
            (
                FactKey::LocationRequirement,
                "the Townsville site".to_owned(),
            ),
            (
                FactKey::LocationSatisfactionText,
                "residence within 45 minutes of the site".to_owned(),
            ),
            (
                FactKey::WaiverText,
                "relocation within 90 days of the start date".to_owned(),
            ),
        ])
    }

    fn requisition() -> Requisition {
        Requisition::new(
            RequisitionDraft {
                id: RequisitionId::try_from("REQ-1").expect("id"),
                employer: "Example Health".to_owned(),
                title: "Registered Nurse".to_owned(),
                reply_to: Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
                sender: Sender::try_new("R. Patel", "Recruitment Coordinator").expect("sender"),
                gates: vec![GateKind::RightToWork, GateKind::MandatoryRegistration],
                facts: facts(),
            },
            at(),
        )
        .expect("requisition")
    }

    fn good_comparative(req: &mut Requisition) -> Comparative {
        req.set_comparative(
            [
                "assessment of deteriorating patients".to_owned(),
                "escalation to a registrar within their first hour".to_owned(),
                "documentation that a covering clinician could act on".to_owned(),
            ],
            "a de-identified case note attached to the application",
            "a post-graduate qualification or a specific employer",
            version(),
        )
        .expect("comparative")
        .clone()
    }

    #[test]
    fn lint_rejects_relative_superiority_and_proxies() {
        let mut req = requisition();
        for banned in [
            "more experience than other applicants",
            "cultural fit with the team",
            "candidates typically lacked depth",
        ] {
            let result = req.set_comparative(
                [
                    banned.to_owned(),
                    "escalation within an hour".to_owned(),
                    "clear documentation".to_owned(),
                ],
                "a case note",
                "a post-graduate qualification",
                version(),
            );
            assert!(
                matches!(result, Err(Error::ComparativeRejected { .. })),
                "{banned:?} should be rejected"
            );
        }
    }

    #[test]
    fn lint_rejects_a_numeric_years_bar() {
        let mut req = requisition();
        let result = req.set_comparative(
            [
                "at least 3 years in acute care".to_owned(),
                "escalation within an hour".to_owned(),
                "clear documentation".to_owned(),
            ],
            "a case note",
            "a post-graduate qualification",
            version(),
        );
        assert!(matches!(result, Err(Error::ComparativeRejected { .. })));
    }

    #[test]
    fn a_regulator_fact_must_be_an_https_url() {
        let mut req = requisition();
        assert!(
            req.set_fact(FactKey::RegulatorUrl, "http://www.ahpra.gov.au")
                .is_err()
        );
        req.set_fact(FactKey::RegulatorUrl, "https://www.ahpra.gov.au")
            .expect("https is accepted");
    }

    #[test]
    fn a_gate_without_facts_is_unavailable() {
        let library = GateLibrary::seed();
        let mut req = requisition();
        assert_eq!(
            req.availability(&library, GateKind::MandatoryRegistration),
            GateAvailability::Available
        );
        req.facts.remove(&FactKey::RegulatorUrl);
        assert_eq!(
            req.availability(&library, GateKind::MandatoryRegistration),
            GateAvailability::MissingFacts {
                missing: vec![FactKey::RegulatorUrl]
            }
        );
        assert!(
            req.blocking_issues(&library)
                .iter()
                .any(|issue| issue.contains("must be recorded as FIELD"))
        );
    }

    #[test]
    fn readiness_needs_approved_library_and_both_signatures() {
        let library = {
            let mut library = GateLibrary::seed();
            for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
                library
                    .approve("Approver", role, at())
                    .expect("library sign-off");
            }
            library
        };
        let mut req = requisition();
        assert!(!req.is_ready(&library));
        good_comparative(&mut req);
        for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
            req.approve("J. Chen", role, at()).expect("sign-off");
            req.approve_comparative("J. Chen", role, at())
                .expect("sign-off");
        }
        assert!(
            req.is_ready(&library),
            "{:?}",
            req.blocking_issues(&library)
        );
    }

    #[test]
    fn changing_the_paragraph_voids_the_paragraph_approval() {
        let mut req = requisition();
        good_comparative(&mut req);
        for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
            req.approve("J. Chen", role, at()).expect("sign-off");
            req.approve_comparative("J. Chen", role, at())
                .expect("sign-off");
        }
        assert!(req.is_bound_copy_approved());
        assert!(
            req.comparative
                .as_ref()
                .expect("paragraph")
                .approval
                .is_bound_to(&req.comparative.as_ref().expect("paragraph").content_hash())
        );

        good_comparative(&mut req);
        // The requisition's own copy did not change, so its sign-off stands.
        assert!(req.is_bound_copy_approved());
        // The new paragraph starts unsigned.
        let paragraph = req.comparative.expect("paragraph");
        assert!(!paragraph.approval.is_approved());
        assert!(paragraph.content_hash().len() == 64);
    }
}
