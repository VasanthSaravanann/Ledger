//! Single-pass sign-off.
//!
//! Task 1.2 of the plan requires a single-pass approval of the static pilot copy
//! by the Hiring Manager and Legal before a requisition opens. [`Approval`] records
//! one [`SignOff`] per role and refuses a second signature from the same role, so
//! "approved once, before posting" is enforced by the type rather than by process.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Who is allowed to sign off, and on what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignOffRole {
    /// The hiring manager named on the requisition.
    HiringManager,
    /// The pilot employer's legal counsel.
    Legal,
    /// The recruiter who owns the reply mailbox.
    Recruiter,
    /// The person running the pilot.
    PilotLead,
}

impl SignOffRole {
    /// Every role, in the order a sign-off sheet lists them.
    pub const ALL: [Self; 4] = [
        Self::HiringManager,
        Self::Legal,
        Self::Recruiter,
        Self::PilotLead,
    ];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::HiringManager => "hiring-manager",
            Self::Legal => "legal",
            Self::Recruiter => "recruiter",
            Self::PilotLead => "pilot-lead",
        }
    }
}

impl fmt::Display for SignOffRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::str::FromStr for SignOffRole {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "hiring-manager" | "hm" => Ok(Self::HiringManager),
            "legal" | "counsel" => Ok(Self::Legal),
            "recruiter" | "coordinator" => Ok(Self::Recruiter),
            "pilot-lead" | "lead" => Ok(Self::PilotLead),
            other => Err(Error::invalid(
                "sign-off role",
                format!("expected hiring-manager, legal, recruiter or pilot-lead, got {other:?}"),
            )),
        }
    }
}

/// One named person's decision, at one point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignOff {
    /// The person's name or staff id.
    pub actor: String,
    /// The capacity they signed in.
    pub role: SignOffRole,
    /// When they signed.
    pub at: DateTime<Utc>,
}

impl SignOff {
    /// Builds a sign-off, rejecting a blank actor.
    fn new(actor: impl Into<String>, role: SignOffRole, at: DateTime<Utc>) -> Result<Self> {
        let actor = actor.into().trim().to_owned();
        if actor.is_empty() {
            return Err(Error::invalid("sign-off actor", "must name a person"));
        }
        Ok(Self { actor, role, at })
    }
}

/// The signatures collected for one piece of static copy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Approval {
    sign_offs: Vec<SignOff>,
}

impl Approval {
    /// Records a signature. A role signs once; a repeat is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadySignedOff`] when `role` already signed, and
    /// [`Error::Invalid`] when `actor` is blank.
    pub fn record(
        &mut self,
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
    ) -> Result<&SignOff> {
        if self.has(role) {
            return Err(Error::AlreadySignedOff { role });
        }
        self.sign_offs.push(SignOff::new(actor, role, at)?);
        self.sign_offs
            .last()
            .ok_or_else(|| Error::invalid("sign-off", "recorded sign-off disappeared"))
    }

    /// Returns whether `role` has signed.
    pub fn has(&self, role: SignOffRole) -> bool {
        self.sign_offs.iter().any(|sign_off| sign_off.role == role)
    }

    /// Returns every recorded signature.
    pub fn sign_offs(&self) -> &[SignOff] {
        &self.sign_offs
    }

    /// Returns the roles the pilot still needs before copy may be used.
    pub fn missing(&self) -> Vec<SignOffRole> {
        [SignOffRole::HiringManager, SignOffRole::Legal]
            .into_iter()
            .filter(|role| !self.has(*role))
            .collect()
    }

    /// Returns whether both the Hiring Manager and Legal signed.
    pub fn is_approved(&self) -> bool {
        self.missing().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 9, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    #[test]
    fn needs_both_hiring_manager_and_legal() {
        let mut approval = Approval::default();
        assert!(!approval.is_approved());
        approval
            .record("J. Chen", SignOffRole::HiringManager, at())
            .expect("first signature");
        assert_eq!(approval.missing(), vec![SignOffRole::Legal]);
        approval
            .record("M. Okafor", SignOffRole::Legal, at())
            .expect("second signature");
        assert!(approval.is_approved());
    }

    #[test]
    fn a_role_signs_once() {
        let mut approval = Approval::default();
        approval
            .record("J. Chen", SignOffRole::Legal, at())
            .expect("first signature");
        assert!(matches!(
            approval.record("Someone else", SignOffRole::Legal, at()),
            Err(Error::AlreadySignedOff {
                role: SignOffRole::Legal
            })
        ));
    }

    #[test]
    fn a_reviewer_must_be_named() {
        let mut approval = Approval::default();
        assert!(approval.record("  ", SignOffRole::Legal, at()).is_err());
    }
}
