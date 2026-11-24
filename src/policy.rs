//! Governance settings the pilot needs on the record: retention and escalation
//! (Tasks 1.1 and 1.2).
//!
//! V2 requires an agreed retention period for the send log and disposition
//! records, and a named ER/Legal escalation owner with a one-business-day SLA for
//! discrimination complaints. The tool records both so the runbook can be checked
//! rather than remembered.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::store::{self, Store};
use crate::text;

/// Seven years in days: the starting point for employment records, until counsel
/// agrees a different period.
pub const DEFAULT_RETENTION_DAYS: u32 = 2_557;

/// The escalation SLA V2 commits to, in hours.
pub const DEFAULT_SLA_HOURS: u32 = 24;

/// Retention and escalation on the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// How long the send log and disposition records are retained.
    pub retention_days: u32,
    /// The named ER/Legal owner for escalation.
    pub escalation_owner: String,
    /// The acknowledgement SLA for a discrimination complaint, in hours.
    pub escalation_sla_hours: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            escalation_owner: String::new(),
            escalation_sla_hours: DEFAULT_SLA_HOURS,
        }
    }
}

impl Policy {
    /// Loads the policy, falling back to the default.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file exists but is bad.
    pub fn load(store: &Store) -> Result<Self> {
        Ok(store.load(store::POLICY)?.unwrap_or_default())
    }

    /// Saves the policy.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(store::POLICY, self)
    }

    /// Sets the named escalation owner.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the name is blank.
    pub fn set_escalation_owner(&mut self, owner: impl Into<String>) -> Result<()> {
        self.escalation_owner = text::required("escalation owner", owner.into(), 120)?;
        Ok(())
    }

    /// Sets the retention period.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the period is zero or absurdly long.
    pub fn set_retention_days(&mut self, days: u32) -> Result<()> {
        if !(1..=36_500).contains(&days) {
            return Err(Error::invalid(
                "retention period",
                "must be between 1 and 36500 days",
            ));
        }
        self.retention_days = days;
        Ok(())
    }

    /// Sets the escalation SLA.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the SLA is zero or longer than a week.
    pub fn set_escalation_sla_hours(&mut self, hours: u32) -> Result<()> {
        if !(1..=168).contains(&hours) {
            return Err(Error::invalid(
                "escalation SLA",
                "must be between 1 and 168 hours",
            ));
        }
        self.escalation_sla_hours = hours;
        Ok(())
    }

    /// Whether the escalation owner has been named.
    pub fn is_configured(&self) -> bool {
        !self.escalation_owner.trim().is_empty()
    }
}

impl fmt::Display for Policy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "retention:          {} days ({:.1} years)",
            self.retention_days,
            self.retention_days as f64 / 365.25
        )?;
        if self.is_configured() {
            writeln!(formatter, "escalation owner:   {}", self.escalation_owner)?;
        } else {
            writeln!(formatter, "escalation owner:   NOT NAMED")?;
        }
        write!(
            formatter,
            "escalation SLA:     {} hours",
            self.escalation_sla_hours
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_owner_is_required_before_the_policy_is_configured() {
        let mut policy = Policy::default();
        assert!(!policy.is_configured());
        assert!(policy.set_escalation_owner("   ").is_err());
        policy
            .set_escalation_owner("M. Okafor, Employee Relations")
            .expect("owner");
        assert!(policy.is_configured());
    }

    #[test]
    fn bounds_are_enforced() {
        let mut policy = Policy::default();
        assert!(policy.set_retention_days(0).is_err());
        assert!(policy.set_retention_days(2_557).is_ok());
        assert!(policy.set_escalation_sla_hours(0).is_err());
        assert!(policy.set_escalation_sla_hours(24).is_ok());
    }

    #[test]
    fn the_default_is_seven_years_and_one_business_day() {
        let policy = Policy::default();
        assert_eq!(policy.retention_days, DEFAULT_RETENTION_DAYS);
        assert_eq!(policy.escalation_sla_hours, DEFAULT_SLA_HOURS);
    }
}
