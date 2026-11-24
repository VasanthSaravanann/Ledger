//! Tier 0 deterministic gates, versioned copy, and their requisition facts.
//!
//! V2 constrains the five gates more tightly than v1 did: `GATE-02` is about
//! work-rights duration versus the role's duration (not "visa type"), `GATE-03`
//! must cite a regulator, `GATE-04` must justify why the licence is inherent, and
//! `GATE-05` must carry the location satisfaction and waiver text. Each block
//! therefore declares the requisition facts it needs, and a gate whose facts are
//! missing is unavailable — the disposition must fall back to `FIELD`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::approval::Approval;
use crate::error::{Error, Result};
use crate::id::TextVersion;

/// The default gate copy version.
pub const DEFAULT_VERSION: &str = "v1.0";

/// The five standard knockout gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateKind {
    /// `GATE-01`: lawful right to work in Australia.
    RightToWork,
    /// `GATE-02`: work-rights duration against the role's duration.
    WorkRightsDuration,
    /// `GATE-03`: legislated registration or accreditation.
    MandatoryRegistration,
    /// `GATE-04`: licence that is inherent to the duties.
    Licence,
    /// `GATE-05`: location and work-location requirement.
    Location,
}

impl GateKind {
    /// Every gate, in copy-library order.
    pub const ALL: [Self; 5] = [
        Self::RightToWork,
        Self::WorkRightsDuration,
        Self::MandatoryRegistration,
        Self::Licence,
        Self::Location,
    ];

    /// The immutable block code, `GATE-01` to `GATE-05`.
    pub fn code(self) -> &'static str {
        match self {
            Self::RightToWork => "GATE-01",
            Self::WorkRightsDuration => "GATE-02",
            Self::MandatoryRegistration => "GATE-03",
            Self::Licence => "GATE-04",
            Self::Location => "GATE-05",
        }
    }

    /// The lowercase slug used by the CLI and by `KO_ANSWER_0n` column mapping.
    pub fn slug(self) -> &'static str {
        match self {
            Self::RightToWork => "right-to-work",
            Self::WorkRightsDuration => "work-rights-duration",
            Self::MandatoryRegistration => "mandatory-registration",
            Self::Licence => "licence",
            Self::Location => "location",
        }
    }

    /// The gate's one-based position, which is also its `KO_ANSWER_0n` number.
    pub fn ordinal(self) -> usize {
        Self::ALL
            .iter()
            .position(|gate| *gate == self)
            .unwrap_or_default()
            + 1
    }

    /// A short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::RightToWork => "Right to work",
            Self::WorkRightsDuration => "Work-rights duration vs role duration",
            Self::MandatoryRegistration => "Mandatory registration or accreditation",
            Self::Licence => "Licence",
            Self::Location => "Location and work-location requirements",
        }
    }

    /// The answers the conditional `GATE_CODE` dropdown offers for this gate.
    pub fn allowed_answers(self) -> &'static [GateAnswer] {
        match self {
            Self::RightToWork => &[GateAnswer::NotHeld],
            Self::WorkRightsDuration => &[GateAnswer::ShorterThanRole, GateAnswer::Restricted],
            Self::MandatoryRegistration => &[GateAnswer::NotRegistered, GateAnswer::Expired],
            Self::Licence => &[GateAnswer::NotHeld, GateAnswer::Expired],
            Self::Location => &[GateAnswer::OutsideRequiredLocation],
        }
    }

    /// The requisition facts this gate's copy needs before the req can post.
    pub fn required_facts(self) -> &'static [FactKey] {
        match self {
            Self::RightToWork => &[],
            Self::WorkRightsDuration => &[
                FactKey::RoleEndOrMinTerm,
                FactKey::MinTerm,
                FactKey::RoleDurationReason,
            ],
            Self::MandatoryRegistration => &[FactKey::RegulatorUrl],
            Self::Licence => &[FactKey::InherentRequirementReason],
            Self::Location => &[
                FactKey::LocationRequirement,
                FactKey::LocationSatisfactionText,
                FactKey::WaiverText,
            ],
        }
    }
}

impl fmt::Display for GateKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for GateKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|gate| gate.slug() == needle || gate.code().to_lowercase() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "gate",
                    format!("expected GATE-01..GATE-05 or a gate slug, got {value:?}"),
                )
            })
    }
}

/// A requisition fact that a gate block needs before the requisition posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactKey {
    /// When the role ends, or its minimum term, in the requisition's own words.
    RoleEndOrMinTerm,
    /// The minimum work-rights duration the role requires.
    MinTerm,
    /// Why the role's duration creates the requirement.
    RoleDurationReason,
    /// The regulator's URL, verified before launch.
    RegulatorUrl,
    /// Why the licence is inherent to the duties.
    InherentRequirementReason,
    /// The role's location requirement.
    LocationRequirement,
    /// How a candidate can satisfy the location requirement.
    LocationSatisfactionText,
    /// The waiver or relocation text.
    WaiverText,
}

impl FactKey {
    /// The `{{SLOT}}` name the fact fills in copy.
    pub fn slot(self) -> &'static str {
        match self {
            Self::RoleEndOrMinTerm => "ROLE_END_OR_MIN_TERM",
            Self::MinTerm => "MIN_TERM",
            Self::RoleDurationReason => "ROLE_DURATION_REASON",
            Self::RegulatorUrl => "REGULATOR_URL",
            Self::InherentRequirementReason => "INHERENT_REQUIREMENT_REASON",
            Self::LocationRequirement => "LOCATION_REQUIREMENT",
            Self::LocationSatisfactionText => "LOCATION_SATISFACTION_TEXT",
            Self::WaiverText => "WAIVER_TEXT",
        }
    }

    /// What the operator has to supply.
    pub fn description(self) -> &'static str {
        match self {
            Self::RoleEndOrMinTerm => "when the role ends, or its minimum term",
            Self::MinTerm => "the minimum work-rights duration required",
            Self::RoleDurationReason => "why the role duration creates the requirement",
            Self::RegulatorUrl => "the regulator's https URL, verified before launch",
            Self::InherentRequirementReason => {
                "why the licence is inherent to the role's concrete duties"
            }
            Self::LocationRequirement => "the role's location requirement",
            Self::LocationSatisfactionText => "how a candidate satisfies the location requirement",
            Self::WaiverText => "the waiver or relocation condition",
        }
    }

    /// Whether the fact must be a verified https URL.
    pub fn is_url(self) -> bool {
        matches!(self, Self::RegulatorUrl)
    }
}

impl FromStr for FactKey {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_ascii_uppercase().replace('-', "_");
        [
            Self::RoleEndOrMinTerm,
            Self::MinTerm,
            Self::RoleDurationReason,
            Self::RegulatorUrl,
            Self::InherentRequirementReason,
            Self::LocationRequirement,
            Self::LocationSatisfactionText,
            Self::WaiverText,
        ]
        .into_iter()
        .find(|fact| fact.slot() == needle)
        .ok_or_else(|| Error::invalid("requisition fact", format!("unknown fact {value:?}")))
    }
}

impl fmt::Display for FactKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slot())
    }
}

/// The candidate's own answer, from a closed vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateAnswer {
    /// The candidate does not hold the requirement.
    NotHeld,
    /// The candidate holds it, but it has expired.
    Expired,
    /// The candidate holds it subject to limits the role cannot accept.
    Restricted,
    /// The candidate's work rights are shorter than the role's duration.
    ShorterThanRole,
    /// The candidate's work rights do not apply at the required location.
    OutsideRequiredLocation,
    /// The candidate is not registered or accredited.
    NotRegistered,
}

impl GateAnswer {
    /// Every answer, for parsing and reporting.
    pub const ALL: [Self; 6] = [
        Self::NotHeld,
        Self::Expired,
        Self::Restricted,
        Self::ShorterThanRole,
        Self::OutsideRequiredLocation,
        Self::NotRegistered,
    ];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::NotHeld => "not-held",
            Self::Expired => "expired",
            Self::Restricted => "restricted",
            Self::ShorterThanRole => "shorter-than-role",
            Self::OutsideRequiredLocation => "outside-required-location",
            Self::NotRegistered => "not-registered",
        }
    }

    /// The clause the email quotes after "In your application you indicated".
    pub fn phrase(self) -> &'static str {
        match self {
            Self::NotHeld => "that you do not hold the stated requirement",
            Self::Expired => "that you hold the stated requirement but it has expired",
            Self::Restricted => {
                "that you hold the stated requirement subject to restrictions the role cannot accept"
            }
            Self::ShorterThanRole => {
                "that your work rights are shorter than the duration of this role"
            }
            Self::OutsideRequiredLocation => {
                "that your work rights do not apply at the required work location"
            }
            Self::NotRegistered => "that you are not registered or accredited as required",
        }
    }
}

impl fmt::Display for GateAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for GateAnswer {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|answer| answer.code() == needle)
            .ok_or_else(|| Error::invalid("gate answer", format!("unknown answer {value:?}")))
    }
}

/// Whether a gate can be met after application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateFlexibility {
    /// Strict: the disposition is final.
    NotWaivable,
    /// The candidate may be reconsidered once a condition holds.
    Waivable {
        /// Pre-approved waiver wording; may contain fact slots.
        condition: String,
    },
    /// Met only if it holds for a stated window.
    TimeLimited {
        /// Days the requirement must hold for.
        window_days: u32,
    },
}

impl GateFlexibility {
    /// Renders the flexibility sentence, substituting any fact slots.
    fn render(&self, facts: &BTreeMap<FactKey, String>) -> Result<String> {
        match self {
            Self::NotWaivable => Ok("This requirement is not waivable for this role.".to_owned()),
            Self::Waivable { condition } => Ok(format!(
                "This requirement can be waived: {}.",
                substitute(condition, facts)?
            )),
            Self::TimeLimited { window_days } => Ok(format!(
                "This requirement is time-limited: it must be met for at least {window_days} days \
                 from the start date."
            )),
        }
    }
}

/// One versioned copy block for one gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateCopy {
    /// Which gate this block covers.
    pub gate: GateKind,
    /// Immutable `TEXT_VERSION` tag, for example `v1.0`.
    pub version: TextVersion,
    /// The requirement, stated as the role's requirement.
    pub requirement: String,
    /// What satisfies the requirement.
    pub what_satisfies: String,
    /// Whether the requirement is waivable or time-limited.
    pub flexibility: GateFlexibility,
}

/// A gate block merged with its requisition facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateText {
    /// `{{GATE_TEXT_BLOCK}}`: requirement, satisfaction, flexibility.
    pub block: String,
    /// The requirement on its own, for the single-line render.
    pub requirement: String,
    /// What satisfies it.
    pub satisfies: String,
    /// The flexibility sentence.
    pub flexibility: String,
}

impl GateCopy {
    /// Builds a copy block, checking that every slot is a known fact.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for blank text or an unknown slot, and
    /// [`Error::UnclosedSlot`] for an unterminated slot.
    pub fn try_new(
        gate: GateKind,
        version: TextVersion,
        requirement: impl Into<String>,
        what_satisfies: impl Into<String>,
        flexibility: GateFlexibility,
    ) -> Result<Self> {
        let requirement = requirement.into().trim().to_owned();
        let what_satisfies = what_satisfies.into().trim().to_owned();
        for (kind, value) in [
            ("gate requirement", &requirement),
            ("gate satisfaction", &what_satisfies),
        ] {
            if value.is_empty() {
                return Err(Error::invalid(kind, "must not be empty"));
            }
            for slot in slots(value)? {
                FactKey::from_str(slot)?;
            }
        }
        if let GateFlexibility::Waivable { condition } = &flexibility {
            for slot in slots(condition)? {
                FactKey::from_str(slot)?;
            }
        }
        Ok(Self {
            gate,
            version,
            requirement,
            what_satisfies,
            flexibility,
        })
    }

    /// Fills the block in with the requisition facts.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingFact`] when a slot has no value.
    pub fn render(&self, facts: &BTreeMap<FactKey, String>) -> Result<GateText> {
        let requirement = substitute(&self.requirement, facts)?;
        let satisfies = substitute(&self.what_satisfies, facts)?;
        let flexibility = self.flexibility.render(facts)?;
        let block = format!("{requirement}\n{satisfies}\n{flexibility}");
        Ok(GateText {
            block,
            requirement,
            satisfies,
            flexibility,
        })
    }
}

/// The approved copy library, with per-block versions and one sign-off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateLibrary {
    /// Sign-off for the library as a whole.
    pub approval: Approval,
    /// One block per gate, each with its own version.
    pub copies: Vec<GateCopy>,
}

impl GateLibrary {
    /// The seeded Australian copy, at `v1.0`, pending sign-off.
    pub fn seed() -> Self {
        let version = TextVersion::parse(DEFAULT_VERSION).expect("default version is valid");
        Self {
            approval: Approval::default(),
            copies: vec![
                GateCopy {
                    gate: GateKind::RightToWork,
                    version: version.clone(),
                    requirement: "This role requires lawful work rights in Australia that are not \
                                  subject to restrictions. This requirement is about the right to \
                                  work, not citizenship or national origin."
                        .to_owned(),
                    what_satisfies: "an Australian or New Zealand passport, permanent residency, \
                                     or a current visa granting unrestricted work rights"
                        .to_owned(),
                    flexibility: GateFlexibility::NotWaivable,
                },
                GateCopy {
                    gate: GateKind::WorkRightsDuration,
                    version: version.clone(),
                    requirement: "This role runs until {{ROLE_END_OR_MIN_TERM}} and requires work \
                                  rights covering at least {{MIN_TERM}}. \
                                  {{ROLE_DURATION_REASON}}"
                        .to_owned(),
                    what_satisfies: "a visa or bridging arrangement whose work rights cover the \
                                     role's full term, confirmed in writing by the Department of \
                                     Home Affairs"
                        .to_owned(),
                    flexibility: GateFlexibility::NotWaivable,
                },
                GateCopy {
                    gate: GateKind::MandatoryRegistration,
                    version: version.clone(),
                    requirement: "This role requires current registration or accreditation with \
                                  the legislated regulator named in the position description \
                                  ({{REGULATOR_URL}})."
                        .to_owned(),
                    what_satisfies:
                        "current registration or accreditation at appointment, with no \
                                     conditions that limit the duties of the role"
                            .to_owned(),
                    flexibility: GateFlexibility::NotWaivable,
                },
                GateCopy {
                    gate: GateKind::Licence,
                    version: version.clone(),
                    requirement: "This role requires a licence that is inherent to its duties: \
                                  {{INHERENT_REQUIREMENT_REASON}}"
                        .to_owned(),
                    what_satisfies: "a current licence of the class stated in the position \
                                     description"
                        .to_owned(),
                    flexibility: GateFlexibility::Waivable {
                        condition: "the licence may be obtained within the first three months of \
                                    appointment if the panel agrees"
                            .to_owned(),
                    },
                },
                GateCopy {
                    gate: GateKind::Location,
                    version,
                    requirement: "This role is based at {{LOCATION_REQUIREMENT}} and is not a \
                                  remote position."
                        .to_owned(),
                    what_satisfies: "{{LOCATION_SATISFACTION_TEXT}}".to_owned(),
                    flexibility: GateFlexibility::Waivable {
                        condition: "{{WAIVER_TEXT}}".to_owned(),
                    },
                },
            ],
        }
    }

    /// Returns the block for `gate`.
    pub fn copy_for(&self, gate: GateKind) -> Option<&GateCopy> {
        self.copies.iter().find(|copy| copy.gate == gate)
    }

    /// Returns the block for `gate`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingGateCopy`] when the library has no block.
    pub fn require(&self, gate: GateKind) -> Result<&GateCopy> {
        self.copy_for(gate).ok_or(Error::MissingGateCopy(gate))
    }

    /// The gates the library does not cover.
    pub fn missing(&self) -> Vec<GateKind> {
        GateKind::ALL
            .into_iter()
            .filter(|gate| self.copy_for(*gate).is_none())
            .collect()
    }

    /// Whether every gate has a block.
    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }

    /// Whether the library is complete and signed off by HM and Legal.
    pub fn is_approved(&self) -> bool {
        self.is_complete() && self.approval.is_bound_to(&self.content_hash())
    }

    /// The facts `gate` still needs before its copy can render.
    pub fn missing_facts(&self, gate: GateKind, facts: &BTreeMap<FactKey, String>) -> Vec<FactKey> {
        gate.required_facts()
            .iter()
            .copied()
            .filter(|key| facts.get(key).is_none_or(|value| value.trim().is_empty()))
            .collect()
    }

    /// Whether `gate` can be rendered with these facts.
    pub fn is_available(&self, gate: GateKind, facts: &BTreeMap<FactKey, String>) -> bool {
        self.copy_for(gate).is_some() && self.missing_facts(gate, facts).is_empty()
    }

    /// Bumps one block's version, which voids the library's sign-off.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingGateCopy`] for an absent block, and
    /// [`Error::Invalid`] when the version is not newer than the current one.
    pub fn bump_version(&mut self, gate: GateKind, version: TextVersion) -> Result<&GateCopy> {
        let copy = self
            .copies
            .iter_mut()
            .find(|copy| copy.gate == gate)
            .ok_or(Error::MissingGateCopy(gate))?;
        if version <= copy.version {
            return Err(Error::invalid(
                "text version",
                format!(
                    "{} must be newer than the current {} for {gate}",
                    version, copy.version
                ),
            ));
        }
        copy.version = version;
        // Old sign-offs are kept on purpose: they no longer cover the new hash, so
        // `is_approved` is false and `drift` can say exactly why. Re-approval
        // appends a new sign-off for the new copy.
        self.copies
            .iter()
            .find(|copy| copy.gate == gate)
            .ok_or(Error::MissingGateCopy(gate))
    }

    /// The SHA-256 of the copy blocks, excluding the approval itself.
    ///
    /// Sign-offs and operator signatures are bound to this value, so editing any
    /// block invalidates the approval instead of silently inheriting it.
    pub fn content_hash(&self) -> String {
        serde_json::to_vec(&self.copies)
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
        role: crate::approval::SignOffRole,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let hash = self.content_hash();
        self.approval.record(actor, role, at, hash)?;
        Ok(())
    }

    /// Saves the library to the workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &crate::store::Store) -> Result<()> {
        store.save(crate::store::GATE_LIBRARY, self)
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

/// Substitutes fact slots in `text`.
fn substitute(text: &str, facts: &BTreeMap<FactKey, String>) -> Result<String> {
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
        let key = FactKey::from_str(after[..end].trim())?;
        let value = facts
            .get(&key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or(Error::MissingFact { fact: key })?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> BTreeMap<FactKey, String> {
        BTreeMap::from([
            (FactKey::RoleEndOrMinTerm, "30 June 2027".to_owned()),
            (FactKey::MinTerm, "12 months".to_owned()),
            (
                FactKey::RoleDurationReason,
                "The role is a fixed-term 12-month appointment.".to_owned(),
            ),
            (FactKey::RegulatorUrl, "https://www.ahpra.gov.au".to_owned()),
            (
                FactKey::InherentRequirementReason,
                "the role requires unaccompanied multi-site travel where no public transport \
                 operates"
                    .to_owned(),
            ),
            (
                FactKey::LocationRequirement,
                "the Townsville site".to_owned(),
            ),
            (
                FactKey::LocationSatisfactionText,
                "residence within 45 minutes of the Townsville site, or a confirmed relocation \
                 date"
                    .to_owned(),
            ),
            (
                FactKey::WaiverText,
                "relocation within 90 days of the start date".to_owned(),
            ),
        ])
    }

    #[test]
    fn seed_covers_every_gate_and_starts_at_v1() {
        let library = GateLibrary::seed();
        assert!(library.is_complete());
        assert!(!library.is_approved());
        for gate in GateKind::ALL {
            let copy = library.require(gate).expect("seeded block");
            assert_eq!(copy.version.as_str(), DEFAULT_VERSION);
        }
    }

    #[test]
    fn every_gate_reference_is_a_known_fact() {
        let library = GateLibrary::seed();
        for gate in GateKind::ALL {
            library
                .require(gate)
                .expect("block")
                .render(&facts())
                .expect("renders with full facts");
        }
    }

    #[test]
    fn duration_gate_is_unavailable_without_its_facts_and_falls_back_to_field() {
        let library = GateLibrary::seed();
        let mut partial = facts();
        partial.remove(&FactKey::MinTerm);
        assert_eq!(
            library.missing_facts(GateKind::WorkRightsDuration, &partial),
            vec![FactKey::MinTerm]
        );
        assert!(!library.is_available(GateKind::WorkRightsDuration, &partial));
        assert!(library.is_available(GateKind::RightToWork, &partial));
    }

    #[test]
    fn missing_fact_is_an_error_not_a_blank() {
        let library = GateLibrary::seed();
        let copy = library
            .require(GateKind::MandatoryRegistration)
            .expect("block");
        assert!(matches!(
            copy.render(&BTreeMap::new()),
            Err(Error::MissingFact {
                fact: FactKey::RegulatorUrl
            })
        ));
    }

    #[test]
    fn bumping_a_version_voids_sign_off_and_must_increase() {
        let mut library = GateLibrary::seed();
        library
            .approve(
                "J. Chen",
                crate::approval::SignOffRole::HiringManager,
                chrono::Utc::now(),
            )
            .expect("sign");
        library
            .approve(
                "M. Okafor",
                crate::approval::SignOffRole::Legal,
                chrono::Utc::now(),
            )
            .expect("sign");
        assert!(library.is_approved());

        library
            .bump_version(
                GateKind::Licence,
                TextVersion::parse("v1.1").expect("version"),
            )
            .expect("bump");
        assert!(
            !library.is_approved(),
            "the old signature does not cover the edited copy"
        );
        assert!(
            library
                .approval
                .drift(&library.content_hash())
                .iter()
                .any(|issue| issue.contains("copy changed")),
            "drift says why, rather than erasing the signature"
        );
        assert!(
            library
                .bump_version(
                    GateKind::Licence,
                    TextVersion::parse("v1.0").expect("version")
                )
                .is_err()
        );
    }

    #[test]
    fn gate_codes_and_slugs_parse() {
        for gate in GateKind::ALL {
            assert_eq!(GateKind::from_str(gate.code()).expect("code"), gate);
            assert_eq!(GateKind::from_str(gate.slug()).expect("slug"), gate);
        }
        assert_eq!(GateKind::Location.ordinal(), 5);
    }

    #[test]
    fn a_gate_never_offers_an_answer_it_does_not_screen_on() {
        assert_eq!(
            GateKind::WorkRightsDuration.allowed_answers(),
            &[GateAnswer::ShorterThanRole, GateAnswer::Restricted]
        );
        assert_eq!(
            GateKind::Location.allowed_answers(),
            &[GateAnswer::OutsideRequiredLocation]
        );
    }
}
