//! The five-scenario canned reply playbook (Task 2.5).
//!
//! The scripts are signed off in the same single pass as the gate copy and the
//! templates, so a coordinator never improvises a reply to a complaint. Scenario 4
//! carries the mandatory escalation: acknowledge within one business day and hand
//! the matter to the named ER/Legal owner without arguing the merits.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::approval::{Approval, SignOffRole};
use crate::error::{Error, Result};
use crate::store::{self, Store};
use crate::text;

/// The five canned reply situations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scenario {
    /// "Please give me individual feedback".
    IndividualFeedbackRequest,
    /// "You recorded my answer wrongly".
    RecordCorrection,
    /// "What should I do differently next time?".
    NextTimeGuidance,
    /// "I think this was discriminatory".
    DiscriminationConcern,
    /// "Can you keep me in mind for other roles?".
    TalentPool,
}

impl Scenario {
    /// Every scenario, in playbook order.
    pub const ALL: [Self; 5] = [
        Self::IndividualFeedbackRequest,
        Self::RecordCorrection,
        Self::NextTimeGuidance,
        Self::DiscriminationConcern,
        Self::TalentPool,
    ];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::IndividualFeedbackRequest => "individual-feedback",
            Self::RecordCorrection => "record-correction",
            Self::NextTimeGuidance => "next-time-guidance",
            Self::DiscriminationConcern => "discrimination-concern",
            Self::TalentPool => "talent-pool",
        }
    }

    /// Whether the scenario must be escalated to the named ER/Legal owner.
    pub fn requires_escalation(self) -> bool {
        matches!(self, Self::DiscriminationConcern)
    }
}

impl fmt::Display for Scenario {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for Scenario {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|scenario| scenario.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "reply scenario",
                    format!(
                        "expected individual-feedback, record-correction, next-time-guidance, \
                         discrimination-concern or talent-pool; got {value:?}"
                    ),
                )
            })
    }
}

/// One approved reply script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    /// The situation the script answers.
    pub scenario: Scenario,
    /// The verbatim text the coordinator sends.
    pub text: String,
    /// The escalation action, when the scenario requires one.
    pub escalation: Option<String>,
}

/// The five scripts plus their single-pass sign-off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playbook {
    /// Sign-off shared by all five scripts.
    pub approval: Approval,
    /// The five scripts.
    pub scripts: Vec<Script>,
}

impl Playbook {
    /// The seeded scripts, pending sign-off.
    pub fn seed() -> Self {
        Self {
            approval: Approval::default(),
            scripts: vec![
                Script {
                    scenario: Scenario::IndividualFeedbackRequest,
                    text: "Thank you for asking. We do not provide individual feedback for this \
                           role, because the decision identified no individual shortfall. The \
                           role-level paragraph in our email describes what the shortlisted \
                           applicants demonstrated."
                        .to_owned(),
                    escalation: None,
                },
                Script {
                    scenario: Scenario::RecordCorrection,
                    text: "Thank you for telling us. We are checking your application record \
                           against our ATS. If our record is wrong, we will correct it, tell you \
                           what changed, and reconsider your application if the requisition is \
                           still open."
                        .to_owned(),
                    escalation: Some(
                        "Coordinator audits the ATS record and, if erroneous, captures a \
                         correction record that supersedes the original, then reinstates the \
                         candidate if the requisition is open."
                            .to_owned(),
                    ),
                },
                Script {
                    scenario: Scenario::NextTimeGuidance,
                    text: "The role-level paragraph in our email describes what the shortlisted \
                           applicants demonstrated and how they evidenced it. We cannot comment \
                           on your application individually, but that paragraph is written to be \
                           useful for your next application."
                        .to_owned(),
                    escalation: None,
                },
                Script {
                    scenario: Scenario::DiscriminationConcern,
                    text: "Thank you for raising this. We take the concern seriously. Your email \
                           has been passed to our employee relations and legal team, who will \
                           respond within one business day. We will not debate the merits of \
                           your application in this thread."
                        .to_owned(),
                    escalation: Some(
                        "Acknowledge within one business day and escalate immediately to the \
                         named ER/Legal owner; do not argue the merits."
                            .to_owned(),
                    ),
                },
                Script {
                    scenario: Scenario::TalentPool,
                    text: "Thank you for your interest. If you would like to be considered for \
                           future roles, reply to this email and we will keep your details on \
                           file with your consent."
                        .to_owned(),
                    escalation: None,
                },
            ],
        }
    }

    /// Returns the script for `scenario`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the playbook lacks it.
    pub fn require(&self, scenario: Scenario) -> Result<&Script> {
        self.scripts
            .iter()
            .find(|script| script.scenario == scenario)
            .ok_or_else(|| Error::invalid("playbook", format!("no script for scenario {scenario}")))
    }

    /// Whether all five scenarios are present.
    pub fn is_complete(&self) -> bool {
        Scenario::ALL
            .into_iter()
            .all(|scenario| self.require(scenario).is_ok())
    }

    /// Whether the playbook is complete and signed off.
    pub fn is_approved(&self) -> bool {
        self.is_complete() && self.approval.is_approved()
    }

    /// Records a single-pass signature on the playbook.
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
        self.approval.record(actor, role, at)?;
        Ok(())
    }

    /// Loads the playbook.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn load(store: &Store) -> Result<Option<Self>> {
        store.load(store::PLAYBOOK)
    }

    /// Saves the playbook.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(store::PLAYBOOK, self)
    }

    /// Validates a hand-edited script after loading.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when a script is blank or a scenario repeats.
    pub fn validate(&self) -> Result<()> {
        for scenario in Scenario::ALL {
            let script = self.require(scenario)?;
            text::required("playbook script", script.text.clone(), 2_000)?;
        }
        if self.scripts.len() != Scenario::ALL.len() {
            return Err(Error::invalid(
                "playbook",
                "expected exactly five scripts, one per scenario",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_playbook_covers_all_five_scenarios() {
        let playbook = Playbook::seed();
        assert!(playbook.is_complete());
        assert!(!playbook.is_approved());
        playbook.validate().expect("seeded playbook is valid");
        for scenario in Scenario::ALL {
            assert_eq!(
                playbook.require(scenario).expect("script").scenario,
                scenario
            );
        }
    }

    #[test]
    fn only_the_discrimination_concern_escalates_to_er_legal() {
        let playbook = Playbook::seed();
        for scenario in Scenario::ALL {
            let script = playbook.require(scenario).expect("script");
            assert_eq!(
                script.escalation.is_some(),
                matches!(
                    scenario,
                    Scenario::DiscriminationConcern | Scenario::RecordCorrection
                ),
                "{scenario} follow-up presence"
            );
        }
        assert_eq!(
            Scenario::ALL
                .into_iter()
                .filter(|scenario| scenario.requires_escalation())
                .collect::<Vec<Scenario>>(),
            vec![Scenario::DiscriminationConcern]
        );
        assert!(
            playbook
                .require(Scenario::RecordCorrection)
                .expect("script")
                .escalation
                .clone()
                .expect("follow-up")
                .contains("supersedes")
        );
        assert!(
            playbook
                .require(Scenario::DiscriminationConcern)
                .expect("script")
                .escalation
                .clone()
                .expect("escalation")
                .contains("one business day")
        );
    }

    #[test]
    fn scenarios_round_trip() {
        for scenario in Scenario::ALL {
            assert_eq!(
                Scenario::from_str(scenario.code()).expect("parses"),
                scenario
            );
        }
    }
}
