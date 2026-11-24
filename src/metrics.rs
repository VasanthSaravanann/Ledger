//! Reply capture and the three v2 decision metrics (Task 4.2).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::id::RequisitionId;
use crate::jsonl::AppendLog;
use crate::log::{BlockedAttempt, SendEntry};
use crate::store::{self, Store};

/// The reply themes Legal and HR review after the pilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplyTheme {
    /// A question about the requirement or the gate text.
    GateQuestion,
    /// A request for status or reconsideration.
    StatusRequest,
    /// A complaint about the process itself.
    ProcessConcern,
    /// Anything else the reviewer classifies.
    Other,
}

impl ReplyTheme {
    /// Every theme.
    pub const ALL: [Self; 4] = [
        Self::GateQuestion,
        Self::StatusRequest,
        Self::ProcessConcern,
        Self::Other,
    ];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::GateQuestion => "gate-question",
            Self::StatusRequest => "status-request",
            Self::ProcessConcern => "process-concern",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for ReplyTheme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for ReplyTheme {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|theme| theme.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "reply theme",
                    format!(
                        "expected one of gate-question, status-request, process-concern, other; \
                         got {value:?}"
                    ),
                )
            })
    }
}

/// One candidate reply received in the monitored mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// The requisition the reply concerns.
    pub requisition: RequisitionId,
    /// When the reply arrived.
    pub received_at: DateTime<Utc>,
    /// The reviewed theme.
    pub theme: ReplyTheme,
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

/// The three pilot metrics, plus the reply-theme tally for the go/no-go review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Emails dispatched.
    pub sends: u64,
    /// Replies received.
    pub replies: u64,
    /// Sends refused for a missing or contradictory disposition.
    pub blocked: u64,
    /// Metric 1: replies per 1,000 sends.
    pub reply_rate_per_1000: f64,
    /// Metric 2: blocked attempts as a share of all rejection attempts.
    pub form_block_rate: f64,
    /// Replies by theme, for the Legal/HR review.
    pub reply_themes: BTreeMap<ReplyTheme, u64>,
}

impl Metrics {
    /// Computes metrics from the evidence already on disk.
    pub fn compute(sends: &[SendEntry], replies: &[Reply], blocked: &[BlockedAttempt]) -> Self {
        let send_count = sends.len() as u64;
        let reply_count = replies.len() as u64;
        let blocked_count = blocked.len() as u64;

        let mut reply_themes = BTreeMap::new();
        for reply in replies {
            *reply_themes.entry(reply.theme).or_insert(0) += 1;
        }

        let reply_rate_per_1000 = if send_count == 0 {
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

        Self {
            sends: send_count,
            replies: reply_count,
            blocked: blocked_count,
            reply_rate_per_1000,
            form_block_rate,
            reply_themes,
        }
    }
}

impl fmt::Display for Metrics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "sends:                 {}", self.sends)?;
        writeln!(formatter, "replies:               {}", self.replies)?;
        writeln!(formatter, "blocked attempts:      {}", self.blocked)?;
        writeln!(
            formatter,
            "reply volume rate:     {:.1} per 1,000 sends",
            self.reply_rate_per_1000
        )?;
        write!(
            formatter,
            "form-block rate:       {:.1}%",
            self.form_block_rate * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_evidence_gives_zero_rates_not_a_division_by_zero() {
        let computed = Metrics::compute(&[], &[], &[]);
        assert_eq!(computed.reply_rate_per_1000, 0.0);
        assert_eq!(computed.form_block_rate, 0.0);
        assert_eq!(computed.sends, 0);
    }

    #[test]
    fn reply_themes_are_tallied() {
        let replies = vec![
            Reply {
                requisition: RequisitionId::try_from("REQ-1").expect("id"),
                received_at: Utc::now(),
                theme: ReplyTheme::GateQuestion,
            },
            Reply {
                requisition: RequisitionId::try_from("REQ-1").expect("id"),
                received_at: Utc::now(),
                theme: ReplyTheme::GateQuestion,
            },
        ];
        let metrics = Metrics::compute(&[], &replies, &[]);
        assert_eq!(
            metrics.reply_themes.get(&ReplyTheme::GateQuestion),
            Some(&2)
        );
        assert_eq!(metrics.replies, 2);
    }

    #[test]
    fn themes_round_trip() {
        for theme in ReplyTheme::ALL {
            assert_eq!(ReplyTheme::from_str(theme.code()).expect("parses"), theme);
        }
    }
}
