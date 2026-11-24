//! Tier 0 deterministic gates and their pre-approved copy.
//!
//! Task 2.2 of the plan: five standard Australian knockout criteria, each with
//! fixed copy stating the requirement, what would satisfy it, and whether it is
//! waivable or time-limited. Nothing here is inferred per candidate; the only
//! per-candidate input is the answer the candidate already gave.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::approval::Approval;
use crate::error::{Error, Result};

/// The five standard knockout gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateKind {
    /// Lawful right to work in Australia.
    RightToWork,
    /// Visa conditions that limit work rights.
    VisaRestrictions,
    /// Registration or accreditation with a named body.
    MandatoryRegistration,
    /// Driver's licence or trade licence.
    Licence,
    /// Location or geographic eligibility.
    Location,
}

impl GateKind {
    /// Every gate the copy library must cover.
    pub const ALL: [Self; 5] = [
        Self::RightToWork,
        Self::VisaRestrictions,
        Self::MandatoryRegistration,
        Self::Licence,
        Self::Location,
    ];

    /// The stable code used in storage, CSV headers, and the CLI.
    pub fn code(self) -> &'static str {
        match self {
            Self::RightToWork => "right-to-work",
            Self::VisaRestrictions => "visa-restrictions",
            Self::MandatoryRegistration => "mandatory-registration",
            Self::Licence => "licence",
            Self::Location => "location",
        }
    }

    /// A short human label for reports.
    pub fn label(self) -> &'static str {
        match self {
            Self::RightToWork => "Right to work",
            Self::VisaRestrictions => "Visa type / restrictions",
            Self::MandatoryRegistration => "Mandatory registration / accreditation",
            Self::Licence => "Driver's licence / trade licence",
            Self::Location => "Location / geographic eligibility",
        }
    }

    /// The answers the disposition form's conditional dropdown offers for this gate.
    pub fn allowed_answers(self) -> &'static [GateAnswer] {
        match self {
            Self::RightToWork => &[GateAnswer::NotHeld],
            Self::VisaRestrictions => &[GateAnswer::Restricted, GateAnswer::Expired],
            Self::MandatoryRegistration => &[GateAnswer::NotRegistered, GateAnswer::Expired],
            Self::Licence => &[GateAnswer::NotHeld, GateAnswer::Expired],
            Self::Location => &[GateAnswer::OutsideRegion],
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
            .find(|gate| gate.code() == needle)
            .ok_or_else(|| {
                let allowed: Vec<&str> = Self::ALL.iter().map(|gate| gate.code()).collect();
                Error::invalid(
                    "gate",
                    format!("expected one of {}, got {value:?}", allowed.join(", ")),
                )
            })
    }
}

/// The candidate's own answer to a knockout question.
///
/// A closed vocabulary is what keeps "no free text, no Other" true on the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateAnswer {
    /// The candidate does not hold the required right, licence, or clearance.
    NotHeld,
    /// The candidate holds it, but it has expired.
    Expired,
    /// The candidate holds it subject to limits the role cannot accept.
    Restricted,
    /// The candidate lives outside the eligible region.
    OutsideRegion,
    /// The candidate is not registered or accredited.
    NotRegistered,
}

impl GateAnswer {
    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::NotHeld => "not-held",
            Self::Expired => "expired",
            Self::Restricted => "restricted",
            Self::OutsideRegion => "outside-region",
            Self::NotRegistered => "not-registered",
        }
    }

    /// The clause the email quotes after "In your application you indicated that".
    pub fn phrase(self) -> &'static str {
        match self {
            Self::NotHeld => "you do not hold the stated requirement",
            Self::Expired => "you hold the stated requirement but it has expired",
            Self::Restricted => {
                "you hold the stated requirement subject to restrictions the role cannot accept"
            }
            Self::OutsideRegion => "you live outside the eligible location",
            Self::NotRegistered => "you are not registered or accredited as required",
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
        let all = [
            Self::NotHeld,
            Self::Expired,
            Self::Restricted,
            Self::OutsideRegion,
            Self::NotRegistered,
        ];
        all.into_iter()
            .find(|answer| answer.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "gate answer",
                    format!(
                        "expected one of not-held, expired, restricted, outside-region, \
                         not-registered; got {value:?}"
                    ),
                )
            })
    }
}

/// Whether, and under what conditions, a gate can be met after application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateFlexibility {
    /// The requirement is strict and the disposition is final.
    NotWaivable,
    /// The candidate may be reconsidered once a condition holds.
    Waivable {
        /// What would waive the requirement, in pre-approved wording.
        condition: String,
    },
    /// The requirement is met only if it holds for a stated window.
    TimeLimited {
        /// How many days the requirement must hold for.
        window_days: u32,
    },
}

impl GateFlexibility {
    /// The sentence merged into the email.
    pub fn sentence(&self) -> String {
        match self {
            Self::NotWaivable => "This requirement is not waivable for this role.".to_owned(),
            Self::Waivable { condition } => {
                format!("This requirement can be waived: {condition}.")
            }
            Self::TimeLimited { window_days } => format!(
                "This requirement is time-limited: it must be met for at least {window_days} days \
                 from the start date."
            ),
        }
    }
}

/// One pre-approved copy block for one gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateCopy {
    /// Which gate this copy covers.
    pub gate: GateKind,
    /// What the role requires, stated as a requirement, not as an allegation.
    pub requirement: String,
    /// What would satisfy the requirement.
    pub what_satisfies: String,
    /// Whether the requirement is waivable or time-limited.
    pub flexibility: GateFlexibility,
}

impl GateCopy {
    /// Builds a copy block, rejecting blank or templated text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when either sentence is blank, too long, or
    /// contains a merge slot (copy must be identical for every candidate).
    pub fn try_new(
        gate: GateKind,
        requirement: impl Into<String>,
        what_satisfies: impl Into<String>,
        flexibility: GateFlexibility,
    ) -> Result<Self> {
        let requirement = requirement.into().trim().to_owned();
        let what_satisfies = what_satisfies.into().trim().to_owned();
        for (kind, text) in [
            ("gate requirement", &requirement),
            ("gate satisfaction", &what_satisfies),
        ] {
            if text.is_empty() {
                return Err(Error::invalid(kind, "must not be empty"));
            }
            if text.chars().count() > 400 {
                return Err(Error::invalid(kind, "must be at most 400 characters"));
            }
            if text.contains("{{") {
                return Err(Error::invalid(
                    kind,
                    "must be static copy and must not contain merge slots",
                ));
            }
        }
        Ok(Self {
            gate,
            requirement,
            what_satisfies,
            flexibility,
        })
    }
}

/// The approved copy library, plus who approved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateLibrary {
    /// Sign-off for the library as a whole.
    pub approval: Approval,
    /// One block per gate.
    pub copies: Vec<GateCopy>,
}

impl GateLibrary {
    /// The seeded Australian gate copy, pending sign-off.
    pub fn seed() -> Self {
        Self {
            approval: Approval::default(),
            copies: vec![
                GateCopy {
                    gate: GateKind::RightToWork,
                    requirement: "This role requires a lawful right to work in Australia that is \
                                  not subject to restrictions."
                        .to_owned(),
                    what_satisfies: "an Australian or New Zealand passport, permanent residency, \
                                     or a current visa with unrestricted work rights"
                        .to_owned(),
                    flexibility: GateFlexibility::NotWaivable,
                },
                GateCopy {
                    gate: GateKind::VisaRestrictions,
                    requirement: "This role requires work rights that are not limited by a visa \
                                  condition, such as a fortnightly hour cap or a single-employer \
                                  sponsorship."
                        .to_owned(),
                    what_satisfies: "a visa granting unrestricted work rights, or a bridging \
                                     arrangement confirmed in writing by the Department of Home \
                                     Affairs"
                        .to_owned(),
                    flexibility: GateFlexibility::TimeLimited { window_days: 365 },
                },
                GateCopy {
                    gate: GateKind::MandatoryRegistration,
                    requirement: "This role requires current registration or accreditation with \
                                  the Australian body named in the position description."
                        .to_owned(),
                    what_satisfies: "current registration or accreditation at the time of \
                                     appointment, with no conditions that limit the duties of the \
                                     role"
                        .to_owned(),
                    flexibility: GateFlexibility::NotWaivable,
                },
                GateCopy {
                    gate: GateKind::Licence,
                    requirement:
                        "This role requires a current Australian driver's licence, or the \
                                  trade licence named in the position description."
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
                    requirement: "This role is based at the location stated in the advertisement \
                                  and is not a remote position."
                        .to_owned(),
                    what_satisfies: "residence within commutable distance of that location, or a \
                                     confirmed relocation date"
                        .to_owned(),
                    flexibility: GateFlexibility::Waivable {
                        condition: "relocation within 90 days of the start date".to_owned(),
                    },
                },
            ],
        }
    }

    /// Returns the copy block for `gate`, if the library covers it.
    pub fn copy_for(&self, gate: GateKind) -> Option<&GateCopy> {
        self.copies.iter().find(|copy| copy.gate == gate)
    }

    /// Returns the copy block for `gate`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingGateCopy`] when the library has no approved text.
    pub fn require(&self, gate: GateKind) -> Result<&GateCopy> {
        self.copy_for(gate).ok_or(Error::MissingGateCopy(gate))
    }

    /// Returns the gates the library does not yet cover.
    pub fn missing(&self) -> Vec<GateKind> {
        GateKind::ALL
            .into_iter()
            .filter(|gate| self.copy_for(*gate).is_none())
            .collect()
    }

    /// Returns whether every gate has a copy block.
    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }

    /// Returns whether the library is complete and signed off by HM and Legal.
    pub fn is_approved(&self) -> bool {
        self.is_complete() && self.approval.is_approved()
    }

    /// Records a single-pass signature on the copy library.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadySignedOff`] on a repeat and [`Error::Invalid`] for
    /// a blank actor.
    pub fn approve(
        &mut self,
        actor: impl Into<String>,
        role: crate::approval::SignOffRole,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        self.approval.record(actor, role, at)?;
        Ok(())
    }

    /// Saves the library to the workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &crate::store::Store) -> crate::error::Result<()> {
        store.save(crate::store::GATE_LIBRARY, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_covers_every_gate_with_static_copy() {
        let library = GateLibrary::seed();
        assert!(library.is_complete());
        assert!(!library.is_approved(), "seed copy starts unsigned");
        for gate in GateKind::ALL {
            let copy = library.require(gate).expect("seeded copy exists");
            assert!(!copy.requirement.contains("{{"));
            assert!(!copy.what_satisfies.contains("{{"));
        }
    }

    #[test]
    fn every_gate_offers_at_least_one_answer_and_they_are_valid() {
        for gate in GateKind::ALL {
            let answers = gate.allowed_answers();
            assert!(!answers.is_empty(), "gate {gate} needs an answer");
            for answer in answers {
                assert_eq!(
                    GateAnswer::from_str(answer.code()).expect("answer parses"),
                    *answer
                );
            }
        }
    }

    #[test]
    fn gate_codes_round_trip() {
        for gate in GateKind::ALL {
            assert_eq!(GateKind::from_str(gate.code()).expect("gate parses"), gate);
        }
    }

    #[test]
    fn copy_rejects_merge_slots() {
        let copy = GateCopy::try_new(
            GateKind::Licence,
            "This role requires {{candidate_name}} to hold a licence.",
            "a current licence",
            GateFlexibility::NotWaivable,
        );
        assert!(copy.is_err());
    }

    #[test]
    fn flexibility_sentences_read_as_sentences() {
        assert_eq!(
            GateFlexibility::NotWaivable.sentence(),
            "This requirement is not waivable for this role."
        );
        assert!(
            GateFlexibility::TimeLimited { window_days: 30 }
                .sentence()
                .contains("30 days")
        );
    }
}
