//! Criteria-feasibility tracking and the post-pilot go/no-go review (Tasks 4.2
//! and 4.3).
//!
//! Metric 3 is deliberately not numeric: it records whether the Hiring Manager
//! and Legal could actually agree a structured criteria list for the role, which
//! decides whether v2 goes to engineering or to criteria-validation consulting.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::id::RequisitionId;
use crate::metrics::Metrics;
use crate::store::{self, Store};
use crate::text;

/// Whether a structured criteria list could be agreed for the role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CriteriaAssessment {
    /// The review has not happened yet.
    NotAssessed,
    /// HM and Legal agreed a structured criteria list.
    Agreed,
    /// HM and Legal could not agree one.
    Disputed,
}

impl CriteriaAssessment {
    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::NotAssessed => "not-assessed",
            Self::Agreed => "agreed",
            Self::Disputed => "disputed",
        }
    }
}

impl fmt::Display for CriteriaAssessment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for CriteriaAssessment {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "not-assessed" | "unassessed" => Ok(Self::NotAssessed),
            "agreed" | "feasible" => Ok(Self::Agreed),
            "disputed" | "not-feasible" => Ok(Self::Disputed),
            other => Err(Error::invalid(
                "criteria assessment",
                format!("expected agreed, disputed or not-assessed; got {other:?}"),
            )),
        }
    }
}

/// One recorded feasibility decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriteriaRecord {
    /// The requisition the decision covers.
    pub requisition: RequisitionId,
    /// The decision.
    pub assessment: CriteriaAssessment,
    /// Who made it.
    pub actor: String,
    /// When.
    pub at: DateTime<Utc>,
    /// Optional context for the go/no-go pack.
    pub note: Option<String>,
}

/// The criteria-feasibility decisions, latest per requisition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Reviews {
    records: Vec<CriteriaRecord>,
}

impl Reviews {
    /// Loads the review file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn load(store: &Store) -> Result<Self> {
        Ok(store.load(store::REVIEW)?.unwrap_or_default())
    }

    /// Saves the review file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(store::REVIEW, self)
    }

    /// Records a decision, replacing any earlier one for the same requisition.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the actor is blank.
    pub fn assess(&mut self, record: CriteriaRecord) -> Result<()> {
        let actor = text::required("review actor", record.actor, 120)?;
        let record = CriteriaRecord {
            actor,
            note: record
                .note
                .map(|note| note.trim().to_owned())
                .filter(|note| !note.is_empty()),
            ..record
        };
        self.records
            .retain(|item| item.requisition != record.requisition);
        self.records.push(record);
        Ok(())
    }

    /// The latest decision for `requisition`.
    pub fn assessment_for(&self, requisition: &RequisitionId) -> CriteriaAssessment {
        self.records
            .iter()
            .rev()
            .find(|record| &record.requisition == requisition)
            .map_or(CriteriaAssessment::NotAssessed, |record| record.assessment)
    }

    /// Every recorded decision, in recording order.
    pub fn records(&self) -> &[CriteriaRecord] {
        &self.records
    }
}

/// The thresholds the pilot is judged against.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    /// Metric 1 ceiling: replies per 1,000 sends.
    pub max_reply_rate_per_1000: f64,
    /// Metric 2 ceiling: share of rejection attempts blocked by the form.
    pub max_form_block_rate: f64,
}

impl Default for Policy {
    /// The pilot's starting thresholds.
    ///
    /// A reply rate above 20 per 1,000 or a form-block rate above 25% means the
    /// manual pipeline is not affordable as designed, so v2 does not start.
    fn default() -> Self {
        Self {
            max_reply_rate_per_1000: 20.0,
            max_form_block_rate: 0.25,
        }
    }
}

impl Policy {
    /// Loads the thresholds, falling back to [`Policy::default`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file exists but is bad.
    pub fn load(store: &Store) -> Result<Self> {
        Ok(store.load(store::POLICY)?.unwrap_or_default())
    }

    /// Saves the thresholds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(store::POLICY, self)
    }
}

/// What the pilot says about v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Recommendation {
    /// Metrics within policy and criteria agreed: start v2 engineering.
    ProceedToV2Engineering,
    /// HM and Legal could not agree structured criteria: resolve that first.
    RunCriteriaValidationConsulting,
    /// Metrics exceeded policy: fix the manual pipeline before automating it.
    HoldAndIterate,
}

impl Recommendation {
    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::ProceedToV2Engineering => "proceed-to-v2-engineering",
            Self::RunCriteriaValidationConsulting => "run-criteria-validation-consulting",
            Self::HoldAndIterate => "hold-and-iterate",
        }
    }
}

impl fmt::Display for Recommendation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// The compiled go/no-go decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoNoGo {
    /// The requisition reviewed.
    pub requisition: RequisitionId,
    /// The pilot metrics.
    pub metrics: Metrics,
    /// Metric 3.
    pub criteria: CriteriaAssessment,
    /// The thresholds applied.
    pub policy: Policy,
    /// Every threshold the pilot exceeded, in plain words.
    pub breaches: Vec<String>,
    /// The decision.
    pub recommendation: Recommendation,
    /// One paragraph explaining the decision.
    pub rationale: String,
}

impl GoNoGo {
    /// Builds the decision. This is the whole of Task 4.3's rule set.
    ///
    /// 1. Unresolved criteria feasibility sends v2 to criteria-validation
    ///    consulting, whatever the metrics say: engineering cannot build a
    ///    criteria list that the Hiring Manager and Legal have not agreed.
    /// 2. With criteria agreed and no threshold exceeded, v2 engineering starts.
    /// 3. Otherwise the manual pipeline is too expensive as designed and the
    ///    pilot iterates before any engineering.
    pub fn decide(
        requisition: RequisitionId,
        metrics: Metrics,
        criteria: CriteriaAssessment,
        policy: Policy,
    ) -> Self {
        let mut breaches = Vec::new();
        if metrics.reply_rate_per_1000 > policy.max_reply_rate_per_1000 {
            breaches.push(format!(
                "reply volume rate {:.1} per 1,000 sends exceeds the {:.1} ceiling",
                metrics.reply_rate_per_1000, policy.max_reply_rate_per_1000
            ));
        }
        if metrics.form_block_rate > policy.max_form_block_rate {
            breaches.push(format!(
                "form-block rate {:.1}% exceeds the {:.1}% ceiling",
                metrics.form_block_rate * 100.0,
                policy.max_form_block_rate * 100.0
            ));
        }

        let (recommendation, rationale) = match criteria {
            CriteriaAssessment::Agreed if breaches.is_empty() => (
                Recommendation::ProceedToV2Engineering,
                format!(
                    "Criteria were agreed and both decision metrics sit inside policy \
                     ({} sends, {:.1} replies per 1,000, {:.1}% of attempts blocked). Start v2 \
                     engineering.",
                    metrics.sends,
                    metrics.reply_rate_per_1000,
                    metrics.form_block_rate * 100.0
                ),
            ),
            CriteriaAssessment::Agreed => (
                Recommendation::HoldAndIterate,
                format!(
                    "Criteria were agreed, but {} threshold(s) were exceeded: {}. Fix the manual \
                     pipeline before automating it.",
                    breaches.len(),
                    breaches.join("; ")
                ),
            ),
            other => (
                Recommendation::RunCriteriaValidationConsulting,
                format!(
                    "Criteria feasibility is {other}, so the v2 Tier 1 fork is unresolved. Run \
                     criteria-validation consulting before estimating v2 engineering, whatever the \
                     metrics show."
                ),
            ),
        };

        Self {
            requisition,
            metrics,
            criteria,
            policy,
            breaches,
            recommendation,
            rationale,
        }
    }
}

impl fmt::Display for GoNoGo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "Go/no-go review for {}", self.requisition)?;
        writeln!(formatter, "{}", self.metrics)?;
        writeln!(formatter, "criteria feasibility:  {}", self.criteria)?;
        writeln!(
            formatter,
            "policy:                <= {:.1} replies per 1,000, <= {:.0}% blocked",
            self.policy.max_reply_rate_per_1000,
            self.policy.max_form_block_rate * 100.0
        )?;
        if self.breaches.is_empty() {
            writeln!(formatter, "breaches:              none")?;
        } else {
            for breach in &self.breaches {
                writeln!(formatter, "breach:                {breach}")?;
            }
        }
        writeln!(formatter, "recommendation:        {}", self.recommendation)?;
        write!(formatter, "\n{}", self.rationale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn metrics(sends: u64, replies: u64, blocked: u64) -> Metrics {
        Metrics {
            sends,
            replies,
            blocked,
            reply_rate_per_1000: if sends == 0 {
                0.0
            } else {
                replies as f64 / sends as f64 * 1_000.0
            },
            form_block_rate: if sends + blocked == 0 {
                0.0
            } else {
                blocked as f64 / (sends + blocked) as f64
            },
            reply_themes: BTreeMap::new(),
        }
    }

    fn requisition() -> RequisitionId {
        RequisitionId::try_from("REQ-1").expect("id")
    }

    #[test]
    fn agreed_criteria_and_cheap_metrics_proceed() {
        let review = GoNoGo::decide(
            requisition(),
            metrics(150, 2, 10),
            CriteriaAssessment::Agreed,
            Policy::default(),
        );
        assert_eq!(
            review.recommendation,
            Recommendation::ProceedToV2Engineering
        );
        assert!(review.breaches.is_empty());
    }

    #[test]
    fn unresolved_criteria_always_sends_work_to_consulting() {
        for criteria in [
            CriteriaAssessment::Disputed,
            CriteriaAssessment::NotAssessed,
        ] {
            let review = GoNoGo::decide(
                requisition(),
                metrics(150, 1, 0),
                criteria,
                Policy::default(),
            );
            assert_eq!(
                review.recommendation,
                Recommendation::RunCriteriaValidationConsulting
            );
        }
    }

    #[test]
    fn expensive_metrics_hold_even_with_agreed_criteria() {
        let review = GoNoGo::decide(
            requisition(),
            metrics(100, 40, 60),
            CriteriaAssessment::Agreed,
            Policy::default(),
        );
        assert_eq!(review.recommendation, Recommendation::HoldAndIterate);
        assert_eq!(review.breaches.len(), 2);
    }

    #[test]
    fn assessment_replaces_the_previous_one() {
        let mut reviews = Reviews::default();
        reviews
            .assess(CriteriaRecord {
                requisition: requisition(),
                assessment: CriteriaAssessment::Disputed,
                actor: "M. Okafor".to_owned(),
                at: Utc::now(),
                note: None,
            })
            .expect("first assessment");
        reviews
            .assess(CriteriaRecord {
                requisition: requisition(),
                assessment: CriteriaAssessment::Agreed,
                actor: "M. Okafor".to_owned(),
                at: Utc::now(),
                note: Some("  agreed after a second workshop  ".to_owned()),
            })
            .expect("second assessment");
        assert_eq!(reviews.records().len(), 1);
        assert_eq!(
            reviews.assessment_for(&requisition()),
            CriteriaAssessment::Agreed
        );
    }
}
