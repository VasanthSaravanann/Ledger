//! Criteria feasibility and the v2 go/no-go routing (Tasks 4.2 and 4.3).
//!
//! Routing is a lookup over the pre-registered metric ratings, applied in the
//! order V2 lists the consequences. It is not a score: a red candidate-value
//! signal stops v2 even if every other metric is green.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::id::RequisitionId;
use crate::metrics::{Metrics, Rating};
use crate::store::{self, Store};
use crate::text;

/// Whether HM and Legal could agree structured criteria and the
/// inherent-requirement justifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CriteriaFeasibility {
    /// Could not agree: red.
    NotAgreed,
    /// Agreed in one session: green.
    AgreedOneSession,
    /// Agreed only with external input: amber.
    AgreedWithExternalInput,
}

impl CriteriaFeasibility {
    /// Every outcome.
    pub const ALL: [Self; 3] = [
        Self::AgreedOneSession,
        Self::AgreedWithExternalInput,
        Self::NotAgreed,
    ];

    /// The stored code.
    pub fn code(self) -> &'static str {
        match self {
            Self::NotAgreed => "not-agreed",
            Self::AgreedOneSession => "agreed-one-session",
            Self::AgreedWithExternalInput => "agreed-with-external-input",
        }
    }

    /// The pre-registered rating.
    pub fn rating(self) -> Rating {
        match self {
            Self::AgreedOneSession => Rating::Green,
            Self::AgreedWithExternalInput => Rating::Amber,
            Self::NotAgreed => Rating::Red,
        }
    }
}

impl fmt::Display for CriteriaFeasibility {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for CriteriaFeasibility {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|feasibility| feasibility.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "criteria feasibility",
                    format!(
                        "expected agreed-one-session, agreed-with-external-input or not-agreed; \
                         got {value:?}"
                    ),
                )
            })
    }
}

/// One recorded feasibility decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriteriaRecord {
    /// The requisition the decision covers.
    pub requisition: RequisitionId,
    /// The decision.
    pub feasibility: CriteriaFeasibility,
    /// Who made it.
    pub actor: String,
    /// When.
    pub at: DateTime<Utc>,
    /// Optional context for the review pack.
    pub note: Option<String>,
}

/// The feasibility decisions, latest per requisition.
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
    pub fn feasibility_for(&self, requisition: &RequisitionId) -> CriteriaFeasibility {
        self.records
            .iter()
            .rev()
            .find(|record| &record.requisition == requisition)
            .map_or(CriteriaFeasibility::NotAgreed, |record| record.feasibility)
    }

    /// Every recorded decision.
    pub fn records(&self) -> &[CriteriaRecord] {
        &self.records
    }
}

/// What v2 should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Candidate-value signal red: do not build v2.
    Stop,
    /// Gate-mismatch rate red: live discrimination exposure.
    FlagDiscriminationExposure,
    /// Form-block rate red: build capture-discipline middleware.
    BuildCaptureDisciplineMiddleware,
    /// Block rate green and criteria green: thin middleware, reassess viability.
    ReassessVentureViability,
    /// Reply volume red: scope automated triage into v2.
    ScopeReplyTriage,
    /// Criteria not agreed: pivot to criteria-validation tooling.
    PivotToCriteriaValidation,
}

impl Action {
    /// The stored code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::FlagDiscriminationExposure => "flag-discrimination-exposure",
            Self::BuildCaptureDisciplineMiddleware => "build-capture-discipline-middleware",
            Self::ReassessVentureViability => "reassess-venture-viability",
            Self::ScopeReplyTriage => "scope-reply-triage",
            Self::PivotToCriteriaValidation => "pivot-to-criteria-validation",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One routing consequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// What to do.
    pub action: Action,
    /// The V2 rule that fired.
    pub rule: String,
    /// The reading that triggered it.
    pub detail: String,
}

/// The compiled routing decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routing {
    /// The requisition reviewed.
    pub requisition: RequisitionId,
    /// The metrics the decision read.
    pub metrics: Metrics,
    /// Metric 4.
    pub criteria: CriteriaFeasibility,
    /// Every consequence, most urgent first. Empty means no rule fired.
    pub decisions: Vec<Decision>,
}

impl Routing {
    /// Applies the V2 routing table, in the order V2 lists it.
    pub fn route(
        requisition: RequisitionId,
        metrics: Metrics,
        criteria: CriteriaFeasibility,
    ) -> Self {
        let mut decisions = Vec::new();

        if metrics.ratings.candidate_value == Some(Rating::Red) {
            decisions.push(Decision {
                action: Action::Stop,
                rule: "candidate-value signal red (< 30% yes)".to_owned(),
                detail: format!(
                    "yes share {:.1}%",
                    metrics.candidate_value_yes_share.unwrap_or(0.0) * 100.0
                ),
            });
            // Stop means stop: no other consequence is worth acting on.
            return Self {
                requisition,
                metrics,
                criteria,
                decisions,
            };
        }

        if metrics.ratings.gate_mismatch == Rating::Red {
            decisions.push(Decision {
                action: Action::FlagDiscriminationExposure,
                rule: "gate-mismatch rate red (> 10%)".to_owned(),
                detail: format!(
                    "{} of {} GATE records failed the knockout consistency check",
                    metrics.gate_mismatches,
                    metrics.gate_sends + metrics.gate_mismatches
                ),
            });
        }

        if metrics.ratings.form_block == Rating::Red {
            decisions.push(Decision {
                action: Action::BuildCaptureDisciplineMiddleware,
                rule: "form-block rate red (> 25%)".to_owned(),
                detail: format!(
                    "{} of {} rejection attempts were blocked",
                    metrics.blocked,
                    metrics.blocked + metrics.sends
                ),
            });
        }

        if metrics.ratings.form_block == Rating::Green
            && criteria == CriteriaFeasibility::AgreedOneSession
        {
            decisions.push(Decision {
                action: Action::ReassessVentureViability,
                rule: "form-block rate green + criteria feasibility yes".to_owned(),
                detail: "the manual pipeline is already cheap and the criteria are settled"
                    .to_owned(),
            });
        }

        if metrics.ratings.reply_volume == Rating::Red {
            decisions.push(Decision {
                action: Action::ScopeReplyTriage,
                rule: "reply volume red (> 80 per 1,000)".to_owned(),
                detail: format!(
                    "{:.1} replies per 1,000 sends",
                    metrics.reply_volume_per_1000
                ),
            });
        }

        if criteria == CriteriaFeasibility::NotAgreed {
            decisions.push(Decision {
                action: Action::PivotToCriteriaValidation,
                rule: "criteria feasibility no".to_owned(),
                detail: "HM and Legal could not agree structured criteria".to_owned(),
            });
        }

        Self {
            requisition,
            metrics,
            criteria,
            decisions,
        }
    }

    /// The most urgent consequence, if any.
    pub fn primary(&self) -> Option<&Decision> {
        self.decisions.first()
    }

    /// Whether any rule fired.
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty()
    }
}

impl fmt::Display for Routing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "v2 routing for {}", self.requisition)?;
        writeln!(formatter, "{}", self.metrics)?;
        writeln!(
            formatter,
            "criteria feasibility: {:.0}  [{}]",
            self.criteria,
            self.criteria.rating()
        )?;
        if self.decisions.is_empty() {
            return write!(
                formatter,
                "\nno routing rule fired: all metrics are amber or better and criteria are agreed \
                 with external input"
            );
        }
        writeln!(formatter, "\nrouting:")?;
        for (index, decision) in self.decisions.iter().enumerate() {
            writeln!(
                formatter,
                "  {}. {} ({}) — {}",
                index + 1,
                decision.action,
                decision.rule,
                decision.detail
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{Metrics, Ratings, SignalValue};
    use std::collections::BTreeMap;

    fn metrics(reply: Rating, block: Rating, mismatch: Rating, value: Option<Rating>) -> Metrics {
        Metrics {
            sends: 100,
            confirmed: 100,
            queued: 0,
            replies: 5,
            blocked: 10,
            gate_sends: 20,
            gate_mismatches: 0,
            reply_volume_per_1000: 50.0,
            form_block_rate: 0.09,
            gate_mismatch_rate: 0.0,
            candidate_value_yes_share: value.map(|_| 0.6),
            signals: BTreeMap::<SignalValue, u64>::new(),
            ratings: Ratings {
                reply_volume: reply,
                form_block: block,
                gate_mismatch: mismatch,
                candidate_value: value,
            },
        }
    }

    fn requisition() -> RequisitionId {
        RequisitionId::try_from("REQ-1").expect("id")
    }

    #[test]
    fn a_red_value_signal_stops_everything() {
        let routing = Routing::route(
            requisition(),
            metrics(Rating::Red, Rating::Red, Rating::Red, Some(Rating::Red)),
            CriteriaFeasibility::NotAgreed,
        );
        assert_eq!(routing.decisions.len(), 1);
        assert_eq!(routing.primary().expect("primary").action, Action::Stop);
    }

    #[test]
    fn red_block_rate_points_at_capture_discipline() {
        let routing = Routing::route(
            requisition(),
            metrics(
                Rating::Green,
                Rating::Red,
                Rating::Green,
                Some(Rating::Amber),
            ),
            CriteriaFeasibility::AgreedWithExternalInput,
        );
        assert_eq!(
            routing.primary().expect("primary").action,
            Action::BuildCaptureDisciplineMiddleware
        );
    }

    #[test]
    fn a_mismatch_red_flags_exposure_before_block_rate() {
        let routing = Routing::route(
            requisition(),
            metrics(Rating::Green, Rating::Red, Rating::Red, Some(Rating::Green)),
            CriteriaFeasibility::AgreedOneSession,
        );
        assert_eq!(
            routing.primary().expect("primary").action,
            Action::FlagDiscriminationExposure
        );
    }

    #[test]
    fn green_block_and_green_criteria_reassess_viability() {
        let routing = Routing::route(
            requisition(),
            metrics(
                Rating::Green,
                Rating::Green,
                Rating::Green,
                Some(Rating::Green),
            ),
            CriteriaFeasibility::AgreedOneSession,
        );
        assert_eq!(
            routing.primary().expect("primary").action,
            Action::ReassessVentureViability
        );
    }

    #[test]
    fn unagreed_criteria_pivot_to_consulting() {
        let routing = Routing::route(
            requisition(),
            metrics(
                Rating::Green,
                Rating::Green,
                Rating::Green,
                Some(Rating::Green),
            ),
            CriteriaFeasibility::NotAgreed,
        );
        assert_eq!(
            routing
                .decisions
                .iter()
                .map(|decision| decision.action)
                .collect::<Vec<Action>>(),
            vec![Action::PivotToCriteriaValidation],
            "reassessing viability needs criteria agreed, so only the pivot applies"
        );
    }

    #[test]
    fn thin_middleware_and_red_reply_volume_can_both_apply() {
        let routing = Routing::route(
            requisition(),
            metrics(
                Rating::Red,
                Rating::Green,
                Rating::Green,
                Some(Rating::Green),
            ),
            CriteriaFeasibility::AgreedOneSession,
        );
        assert_eq!(
            routing
                .decisions
                .iter()
                .map(|decision| decision.action)
                .collect::<Vec<Action>>(),
            vec![Action::ReassessVentureViability, Action::ScopeReplyTriage]
        );
    }

    #[test]
    fn feasibility_ratings_match_the_table() {
        assert_eq!(
            CriteriaFeasibility::AgreedOneSession.rating(),
            Rating::Green
        );
        assert_eq!(
            CriteriaFeasibility::AgreedWithExternalInput.rating(),
            Rating::Amber
        );
        assert_eq!(CriteriaFeasibility::NotAgreed.rating(), Rating::Red);
        for feasibility in CriteriaFeasibility::ALL {
            assert_eq!(
                CriteriaFeasibility::from_str(feasibility.code()).expect("parses"),
                feasibility
            );
        }
    }
}
