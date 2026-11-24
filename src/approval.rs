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
    /// The SHA-256 of the copy this signature covers.
    ///
    /// Absent only in workspaces written before sign-offs were bound to copy; such
    /// a sign-off does not count as approval, so an edited block can never inherit
    /// an old signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artefact_hash: Option<String>,
    /// Ed25519 signature over `artefact_hash`, when an operator key is registered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<crate::identity::Signed>,
}

impl SignOff {
    /// Builds a sign-off, rejecting a blank actor.
    fn new(
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
        artefact_hash: impl Into<String>,
    ) -> Result<Self> {
        let actor = actor.into().trim().to_owned();
        if actor.is_empty() {
            return Err(Error::invalid("sign-off actor", "must name a person"));
        }
        let artefact_hash = artefact_hash.into();
        if artefact_hash.len() != 64 {
            return Err(Error::invalid(
                "sign-off",
                "must be bound to a 64-character content hash",
            ));
        }
        Ok(Self {
            actor,
            role,
            at,
            artefact_hash: Some(artefact_hash),
            signature: None,
        })
    }

    /// Whether this sign-off covers exactly `hash`.
    pub fn covers(&self, hash: &str) -> bool {
        self.artefact_hash.as_deref() == Some(hash)
    }
}

/// The signatures collected for one piece of static copy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Approval {
    sign_offs: Vec<SignOff>,
}

impl Approval {
    /// Records a signature bound to the copy's content hash. A role signs once.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadySignedOff`] when `role` already signed, and
    /// [`Error::Invalid`] when `actor` is blank or the hash is malformed.
    pub fn record(
        &mut self,
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
        artefact_hash: impl Into<String>,
    ) -> Result<&SignOff> {
        let hash = artefact_hash.into();
        // A role signs one *version* of the copy. Re-signing the same copy is a
        // repeat; signing a new copy is a re-approval, and the earlier sign-off is
        // kept so the drift is visible rather than erased.
        if let Some(latest) = self.latest(role) {
            if latest.covers(&hash) {
                return Err(Error::AlreadySignedOff { role });
            }
        }
        self.sign_offs.push(SignOff::new(actor, role, at, hash)?);
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

    /// The newest sign-off for `role`.
    pub fn latest(&self, role: SignOffRole) -> Option<&SignOff> {
        self.sign_offs
            .iter()
            .rev()
            .find(|sign_off| sign_off.role == role)
    }

    /// Whether both roles' newest sign-off covers exactly this copy.
    pub fn is_bound_to(&self, hash: &str) -> bool {
        [SignOffRole::HiringManager, SignOffRole::Legal]
            .into_iter()
            .all(|role| {
                self.latest(role)
                    .is_some_and(|sign_off| sign_off.covers(hash))
            })
    }

    /// Why this approval does not cover `hash`, if it does not.
    ///
    /// Distinguishes "not signed" from "signed different copy", because the second
    /// is the case an operator must not paper over.
    pub fn drift(&self, hash: &str) -> Vec<String> {
        let mut issues = Vec::new();
        for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
            match self.latest(role) {
                None => issues.push(format!("{role} has not signed")),
                Some(sign_off) => match &sign_off.artefact_hash {
                    None => issues.push(format!(
                        "{role} signed before copy binding was introduced; re-approval required"
                    )),
                    Some(recorded) if recorded != hash => issues.push(format!(
                        "copy changed after {role} signed (signed {}, current {})",
                        &recorded[0..12.min(recorded.len())],
                        &hash[0..12.min(hash.len())]
                    )),
                    Some(_) => {}
                },
            }
        }
        issues
    }

    /// Attaches an Ed25519 signature to a role's sign-off.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when no sign-off with that role exists, and
    /// propagates signature errors.
    pub fn sign(
        &mut self,
        role: SignOffRole,
        registry: &crate::identity::Registry,
        store: &crate::store::Store,
        operator: &str,
    ) -> Result<()> {
        let sign_off = self
            .sign_offs
            .iter_mut()
            .rev()
            .find(|sign_off| sign_off.role == role)
            .ok_or_else(|| {
                Error::invalid(
                    "sign-off",
                    format!("{role} has not signed yet, so there is nothing to sign"),
                )
            })?;
        let hash = sign_off
            .artefact_hash
            .clone()
            .ok_or_else(|| Error::invalid("sign-off", "sign-off is not bound to copy"))?;
        sign_off.signature = Some(crate::identity::sign(registry, store, operator, &hash)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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
            .record("J. Chen", SignOffRole::HiringManager, at(), HASH_A)
            .expect("first signature");
        assert_eq!(approval.missing(), vec![SignOffRole::Legal]);
        approval
            .record("M. Okafor", SignOffRole::Legal, at(), HASH_A)
            .expect("second signature");
        assert!(approval.is_approved());
        assert!(approval.is_bound_to(HASH_A));
    }

    #[test]
    fn a_role_signs_once() {
        let mut approval = Approval::default();
        approval
            .record("J. Chen", SignOffRole::Legal, at(), HASH_A)
            .expect("first signature");
        assert!(matches!(
            approval.record("Someone else", SignOffRole::Legal, at(), HASH_A),
            Err(Error::AlreadySignedOff {
                role: SignOffRole::Legal
            })
        ));
    }

    #[test]
    fn a_reviewer_must_be_named() {
        let mut approval = Approval::default();
        assert!(
            approval
                .record("  ", SignOffRole::Legal, at(), HASH_A)
                .is_err()
        );
    }

    #[test]
    fn a_signature_does_not_transfer_to_edited_copy() {
        let mut approval = Approval::default();
        approval
            .record("J. Chen", SignOffRole::HiringManager, at(), HASH_A)
            .expect("hm");
        approval
            .record("M. Okafor", SignOffRole::Legal, at(), HASH_A)
            .expect("legal");
        assert!(approval.is_approved());
        assert!(approval.is_bound_to(HASH_A));
        assert!(!approval.is_bound_to(HASH_B), "edited copy is not approved");

        let drift = approval.drift(HASH_B);
        assert_eq!(drift.len(), 2, "{drift:?}");
        assert!(drift.iter().all(|issue| issue.contains("copy changed")));
    }

    #[test]
    fn an_unbound_legacy_signature_does_not_count() {
        let mut approval = Approval::default();
        approval.sign_offs.push(SignOff {
            actor: "J. Chen".to_owned(),
            role: SignOffRole::HiringManager,
            at: at(),
            artefact_hash: None,
            signature: None,
        });
        approval.sign_offs.push(SignOff {
            actor: "M. Okafor".to_owned(),
            role: SignOffRole::Legal,
            at: at(),
            artefact_hash: None,
            signature: None,
        });
        assert!(approval.is_approved(), "both roles are present");
        assert!(!approval.is_bound_to(HASH_A), "but nothing is bound");
        assert!(
            approval
                .drift(HASH_A)
                .iter()
                .any(|issue| issue.contains("re-approval required"))
        );
    }

    #[test]
    fn a_malformed_hash_is_refused() {
        let mut approval = Approval::default();
        assert!(
            approval
                .record("J. Chen", SignOffRole::Legal, at(), "short")
                .is_err()
        );
    }
}
