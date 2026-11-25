//! Reply capture, the candidate-value signal, and the five decision metrics
//! (Task 4.2).
//!
//! Thresholds are pre-registered in this file and are not configurable: the point
//! of the pilot is to read the metrics against thresholds committed to before the
//! data arrived.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::delivery::{DeliveryConfirmation, effective_status};
use crate::error::{Error, Result};
use crate::id::RequisitionId;
use crate::jsonl::AppendLog;
use crate::log::{BlockReason, BlockedAttempt, DeliveryStatus, SendEntry};
use crate::playbook::Scenario;
use crate::store::{self, Store};

/// One reply received in the monitored mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// The requisition the reply concerns.
    pub requisition: RequisitionId,
    /// When the reply arrived.
    pub received_at: DateTime<Utc>,
    /// The playbook scenario it was handled under, when one applies.
    pub scenario: Option<Scenario>,
    /// The application the reply concerns, when the sender identifies it.
    #[serde(default)]
    pub application_id: Option<crate::id::ApplicationId>,
}

/// The append-only reply log.
#[derive(Debug)]
pub struct Replies {
    log: AppendLog<Reply>,
}

impl Replies {
    /// Opens the reply log.
    pub fn open(store: &Store) -> Self {
        Self {
            log: store.log(store::REPLIES),
        }
    }

    /// Records one reply (or `count` identical replies in one call).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the record cannot be written.
    pub fn record(&mut self, reply: Reply, count: u32) -> Result<()> {
        for _ in 0..count {
            self.log.append(&reply)?;
        }
        Ok(())
    }

    /// Every reply, in arrival order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the log cannot be read.
    pub fn all(&self) -> Result<Vec<Reply>> {
        self.log.read_all()
    }

    /// Replies for one requisition.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the log cannot be read.
    pub fn for_requisition(&self, requisition: &RequisitionId) -> Result<Vec<Reply>> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|reply| &reply.requisition == requisition)
            .collect())
    }
}

/// The answer to "Was this useful for your next application?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignalValue {
    /// Yes.
    Yes,
    /// Somewhat.
    Somewhat,
    /// No.
    No,
}

impl SignalValue {
    /// Every value.
    pub const ALL: [Self; 3] = [Self::Yes, Self::Somewhat, Self::No];

    /// The stored code, matching the mailto subject suffix.
    pub fn code(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::Somewhat => "somewhat",
            Self::No => "no",
        }
    }
}

impl fmt::Display for SignalValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::str::FromStr for SignalValue {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|signal| signal.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "candidate-value signal",
                    format!("expected yes, somewhat or no; got {value:?}"),
                )
            })
    }
}

/// One recorded value signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    /// The requisition.
    pub requisition: RequisitionId,
    /// The answer given.
    pub value: SignalValue,
    /// When it was recorded.
    pub at: DateTime<Utc>,
}

/// The append-only signal log.
#[derive(Debug)]
pub struct Signals {
    log: AppendLog<Signal>,
}

impl Signals {
    /// Opens the signal log.
    pub fn open(store: &Store) -> Self {
        Self {
            log: store.log(store::SIGNALS),
        }
    }

    /// Records `count` signals of one value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the record cannot be written.
    pub fn record(&mut self, signal: Signal, count: u32) -> Result<()> {
        for _ in 0..count {
            self.log.append(&signal)?;
        }
        Ok(())
    }

    /// Every signal for `requisition`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the log cannot be read.
    pub fn for_requisition(&self, requisition: &RequisitionId) -> Result<Vec<Signal>> {
        Ok(self
            .log
            .read_all()?
            .into_iter()
            .filter(|signal| &signal.requisition == requisition)
            .collect())
    }
}

/// A pre-registered threshold band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rating {
    /// Inside the green band.
    Green,
    /// Inside the amber band.
    Amber,
    /// Past the red threshold.
    Red,
}

impl Rating {
    /// The stored code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Amber => "amber",
            Self::Red => "red",
        }
    }
}

impl fmt::Display for Rating {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Reply volume: replies per 1,000 sends. Green < 30, amber 30–80, red > 80.
pub fn rate_reply_volume(per_1000: f64) -> Rating {
    if per_1000 < 30.0 {
        Rating::Green
    } else if per_1000 <= 80.0 {
        Rating::Amber
    } else {
        Rating::Red
    }
}

/// Form-block rate. Green < 10%, amber 10–25%, red > 25%.
pub fn rate_form_block(rate: f64) -> Rating {
    if rate < 0.10 {
        Rating::Green
    } else if rate <= 0.25 {
        Rating::Amber
    } else {
        Rating::Red
    }
}

/// Gate-mismatch rate. Green < 2%, amber 2–10%, red > 10%.
pub fn rate_gate_mismatch(rate: f64) -> Rating {
    if rate < 0.02 {
        Rating::Green
    } else if rate <= 0.10 {
        Rating::Amber
    } else {
        Rating::Red
    }
}

/// Candidate-value signal, the `"Yes"` share. Green > 50%, amber 30–50%, red < 30%.
pub fn rate_candidate_value(yes_share: f64) -> Rating {
    if yes_share > 0.50 {
        Rating::Green
    } else if yes_share >= 0.30 {
        Rating::Amber
    } else {
        Rating::Red
    }
}

/// The five metrics, their inputs, and their ratings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Emails dispatched (any delivery status).
    pub sends: u64,
    /// Sends a reader may believe were delivered: recorded `sent` at dispatch, or
    /// confirmed by a chained `DELIVERY_CONFIRMED` entry.
    pub confirmed: u64,
    /// Sends still waiting for a confirmation, so never called `sent`.
    pub queued: u64,
    /// Replies received.
    pub replies: u64,
    /// Sends refused by a circuit-breaker.
    pub blocked: u64,
    /// `GATE` sends.
    pub gate_sends: u64,
    /// `GATE` records that failed the knockout consistency check.
    pub gate_mismatches: u64,
    /// Metric 1: replies per 1,000 sends.
    pub reply_volume_per_1000: f64,
    /// Metric 2: blocked ÷ (blocked + sends).
    pub form_block_rate: f64,
    /// Metric 3: mismatches ÷ (gate sends + mismatches).
    pub gate_mismatch_rate: f64,
    /// Metric 5: the `"Yes"` share, absent when no signal has been recorded.
    pub candidate_value_yes_share: Option<f64>,
    /// The value-signal tally.
    pub signals: BTreeMap<SignalValue, u64>,
    /// Ratings for metrics 1–3 and 5.
    pub ratings: Ratings,
}

/// Ratings for the four measurable metrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratings {
    /// Metric 1.
    pub reply_volume: Rating,
    /// Metric 2.
    pub form_block: Rating,
    /// Metric 3.
    pub gate_mismatch: Rating,
    /// Metric 5, absent when no signal has been recorded.
    pub candidate_value: Option<Rating>,
}

impl Metrics {
    /// Computes the metrics from the evidence on disk.
    pub fn compute(
        sends: &[SendEntry],
        replies: &[Reply],
        blocked: &[BlockedAttempt],
        signals: &[Signal],
        confirmations: &[DeliveryConfirmation],
    ) -> Self {
        let send_count = sends.len() as u64;
        let confirmed_ids: BTreeSet<&crate::id::SendId> =
            confirmations.iter().map(|entry| &entry.send_id).collect();
        let mut confirmed = 0_u64;
        let mut queued = 0_u64;
        for entry in sends {
            match effective_status(entry, confirmed_ids.contains(&entry.send_id)) {
                DeliveryStatus::Sent => confirmed += 1,
                DeliveryStatus::Queued => queued += 1,
                DeliveryStatus::Failed => {}
            }
        }
        let reply_count = replies.len() as u64;
        let blocked_count = blocked.len() as u64;
        let gate_sends = sends
            .iter()
            .filter(|entry| entry.disposition == crate::disposition::DispositionKind::Gate)
            .count() as u64;
        let gate_mismatches = blocked
            .iter()
            .filter(|attempt| matches!(attempt.reason, BlockReason::AnswerMismatch { .. }))
            .count() as u64;

        let mut tally: BTreeMap<SignalValue, u64> = BTreeMap::new();
        for signal in signals {
            *tally.entry(signal.value).or_insert(0) += 1;
        }
        let yes = tally.get(&SignalValue::Yes).copied().unwrap_or(0);
        let somewhat = tally.get(&SignalValue::Somewhat).copied().unwrap_or(0);
        let no = tally.get(&SignalValue::No).copied().unwrap_or(0);
        let answered = yes + somewhat + no;
        let yes_share = if answered == 0 {
            None
        } else {
            Some(yes as f64 / answered as f64)
        };

        let reply_volume_per_1000 = if send_count == 0 {
            0.0
        } else {
            reply_count as f64 / send_count as f64 * 1_000.0
        };
        let attempts = send_count + blocked_count;
        let form_block_rate = if attempts == 0 {
            0.0
        } else {
            blocked_count as f64 / attempts as f64
        };
        let gate_attempts = gate_sends + gate_mismatches;
        let gate_mismatch_rate = if gate_attempts == 0 {
            0.0
        } else {
            gate_mismatches as f64 / gate_attempts as f64
        };

        Self {
            sends: send_count,
            confirmed,
            queued,
            replies: reply_count,
            blocked: blocked_count,
            gate_sends,
            gate_mismatches,
            reply_volume_per_1000,
            form_block_rate,
            gate_mismatch_rate,
            candidate_value_yes_share: yes_share,
            signals: tally,
            ratings: Ratings {
                reply_volume: rate_reply_volume(reply_volume_per_1000),
                form_block: rate_form_block(form_block_rate),
                gate_mismatch: rate_gate_mismatch(gate_mismatch_rate),
                candidate_value: yes_share.map(rate_candidate_value),
            },
        }
    }

    /// How many sends have a non-failed delivery status.
    pub fn delivered(&self, sends: &[SendEntry]) -> u64 {
        sends
            .iter()
            .filter(|entry| entry.delivery_status != DeliveryStatus::Failed)
            .count() as u64
    }
}

impl fmt::Display for Metrics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "sends:              {}", self.sends)?;
        writeln!(formatter, "confirmed:          {}", self.confirmed)?;
        writeln!(formatter, "queued:             {}", self.queued)?;
        writeln!(formatter, "replies:            {}", self.replies)?;
        writeln!(formatter, "blocked:            {}", self.blocked)?;
        writeln!(formatter, "gate sends:         {}", self.gate_sends)?;
        writeln!(formatter, "gate mismatches:    {}", self.gate_mismatches)?;
        writeln!(
            formatter,
            "reply volume:       {:.1} per 1,000  [{}]",
            self.reply_volume_per_1000, self.ratings.reply_volume
        )?;
        writeln!(
            formatter,
            "form-block rate:    {:.1}%  [{}]",
            self.form_block_rate * 100.0,
            self.ratings.form_block
        )?;
        writeln!(
            formatter,
            "gate-mismatch rate: {:.1}%  [{}]",
            self.gate_mismatch_rate * 100.0,
            self.ratings.gate_mismatch
        )?;
        match (self.candidate_value_yes_share, self.ratings.candidate_value) {
            (Some(share), Some(rating)) => write!(
                formatter,
                "candidate value:    {:.1}% yes  [{}]",
                share * 100.0,
                rating
            ),
            _ => write!(formatter, "candidate value:    no signals recorded"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(sends: u64, replies: u64, blocked: u64, gate: u64, mismatch: u64) -> Metrics {
        let reply_volume_per_1000 = if sends == 0 {
            0.0
        } else {
            replies as f64 / sends as f64 * 1_000.0
        };
        let attempts = sends + blocked;
        let form_block_rate = if attempts == 0 {
            0.0
        } else {
            blocked as f64 / attempts as f64
        };
        let gate_attempts = gate + mismatch;
        let gate_mismatch_rate = if gate_attempts == 0 {
            0.0
        } else {
            mismatch as f64 / gate_attempts as f64
        };
        Metrics {
            sends,
            confirmed: sends,
            queued: 0,
            replies,
            blocked,
            gate_sends: gate,
            gate_mismatches: mismatch,
            reply_volume_per_1000,
            form_block_rate,
            gate_mismatch_rate,
            candidate_value_yes_share: None,
            signals: BTreeMap::new(),
            ratings: Ratings {
                reply_volume: rate_reply_volume(reply_volume_per_1000),
                form_block: rate_form_block(form_block_rate),
                gate_mismatch: rate_gate_mismatch(gate_mismatch_rate),
                candidate_value: None,
            },
        }
    }

    #[test]
    fn the_bands_match_the_pre_registered_table() {
        assert_eq!(rate_reply_volume(29.9), Rating::Green);
        assert_eq!(rate_reply_volume(30.0), Rating::Amber);
        assert_eq!(rate_reply_volume(80.0), Rating::Amber);
        assert_eq!(rate_reply_volume(80.1), Rating::Red);

        assert_eq!(rate_form_block(0.099), Rating::Green);
        assert_eq!(rate_form_block(0.10), Rating::Amber);
        assert_eq!(rate_form_block(0.25), Rating::Amber);
        assert_eq!(rate_form_block(0.251), Rating::Red);

        assert_eq!(rate_gate_mismatch(0.019), Rating::Green);
        assert_eq!(rate_gate_mismatch(0.02), Rating::Amber);
        assert_eq!(rate_gate_mismatch(0.10), Rating::Amber);
        assert_eq!(rate_gate_mismatch(0.11), Rating::Red);

        assert_eq!(rate_candidate_value(0.51), Rating::Green);
        assert_eq!(rate_candidate_value(0.50), Rating::Amber);
        assert_eq!(rate_candidate_value(0.30), Rating::Amber);
        assert_eq!(rate_candidate_value(0.29), Rating::Red);
    }

    #[test]
    fn empty_evidence_has_no_candidate_value_rating() {
        let metrics = Metrics::compute(&[], &[], &[], &[], &[]);
        assert_eq!(metrics.reply_volume_per_1000, 0.0);
        assert_eq!(metrics.form_block_rate, 0.0);
        assert_eq!(metrics.confirmed, 0);
        assert_eq!(metrics.queued, 0);
        assert!(metrics.candidate_value_yes_share.is_none());
        assert!(metrics.ratings.candidate_value.is_none());
    }

    #[test]
    fn a_twenty_percent_mismatch_is_red() {
        let metrics = metrics(100, 3, 5, 4, 1);
        assert_eq!(metrics.ratings.gate_mismatch, Rating::Red);
        assert!((metrics.gate_mismatch_rate - 0.2).abs() < 1e-9);
    }

    #[test]
    fn the_yes_share_ignores_unanswered() {
        let signals = vec![
            Signal {
                requisition: RequisitionId::try_from("REQ-1").expect("id"),
                value: SignalValue::Yes,
                at: Utc::now(),
            },
            Signal {
                requisition: RequisitionId::try_from("REQ-1").expect("id"),
                value: SignalValue::No,
                at: Utc::now(),
            },
            Signal {
                requisition: RequisitionId::try_from("REQ-1").expect("id"),
                value: SignalValue::No,
                at: Utc::now(),
            },
        ];
        let metrics = Metrics::compute(&[], &[], &[], &signals, &[]);
        assert_eq!(metrics.candidate_value_yes_share, Some(1.0 / 3.0));
        assert_eq!(metrics.ratings.candidate_value, Some(Rating::Amber));
    }

    #[test]
    fn signal_values_round_trip() {
        for value in SignalValue::ALL {
            assert_eq!(value.code().parse::<SignalValue>().expect("parses"), value);
        }
    }
}
