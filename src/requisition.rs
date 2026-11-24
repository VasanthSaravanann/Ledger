//! Requisitions, their static comparative paragraph, and readiness.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::approval::{Approval, SignOffRole};
use crate::error::{Error, Result};
use crate::gate::{GateKind, GateLibrary};
use crate::id::{Mailbox, RequisitionId};
use crate::text;

/// Longest comparative paragraph the pilot accepts.
const MAX_COMPARATIVE_CHARS: usize = 1_500;

/// The single static paragraph describing what a competitive application looked
/// like (Task 2.3).
///
/// It is deliberately text without merge slots: the same paragraph goes to every
/// candidate in the "met the bar, stronger field" bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparativeParagraph {
    /// The approved wording.
    pub text: String,
    /// Sign-off for this wording.
    pub approval: Approval,
}

impl ComparativeParagraph {
    /// Builds an unsigned paragraph.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the text is blank, too long, or contains a
    /// merge slot (which would let a recruiter tailor it per candidate).
    pub fn try_new(text: impl Into<String>) -> Result<Self> {
        let text = text::required("comparative paragraph", text.into(), MAX_COMPARATIVE_CHARS)?;
        if text.contains("{{") {
            return Err(Error::invalid(
                "comparative paragraph",
                "must be static copy and must not contain merge slots",
            ));
        }
        Ok(Self {
            text,
            approval: Approval::default(),
        })
    }
}

/// One open requisition and the copy approved for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requisition {
    /// The requisition's ATS id.
    pub id: RequisitionId,
    /// The pilot employer, as it should appear in an email.
    pub employer: String,
    /// The advertised role title.
    pub title: String,
    /// The monitored reply mailbox candidates reach a human on.
    pub reply_to: Mailbox,
    /// The knockout gates this requisition screens on.
    pub gates: Vec<GateKind>,
    /// The static comparative paragraph, once drafted.
    pub comparative: Option<ComparativeParagraph>,
    /// Hiring-manager and legal sign-off for the requisition's static copy.
    pub approval: Approval,
    /// When the requisition was created.
    pub created_at: DateTime<Utc>,
}

impl Requisition {
    /// Builds a requisition that screens on at least one gate.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when a field is blank, too long, or when `gates`
    /// is empty.
    pub fn new(
        id: RequisitionId,
        employer: impl Into<String>,
        title: impl Into<String>,
        reply_to: Mailbox,
        gates: impl IntoIterator<Item = GateKind>,
        created_at: DateTime<Utc>,
    ) -> Result<Self> {
        let employer = text::required("employer", employer.into(), 120)?;
        let title = text::required("role title", title.into(), 120)?;
        let gates: BTreeSet<GateKind> = gates.into_iter().collect();
        if gates.is_empty() {
            return Err(Error::invalid(
                "requisition gates",
                "a requisition must screen on at least one gate",
            ));
        }
        Ok(Self {
            id,
            employer,
            title,
            reply_to,
            gates: gates.into_iter().collect(),
            comparative: None,
            approval: Approval::default(),
            created_at,
        })
    }

    /// Replaces the comparative paragraph and clears every approval that covered
    /// the old copy.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the paragraph is not acceptable.
    pub fn set_comparative(&mut self, text: impl Into<String>) -> Result<&ComparativeParagraph> {
        self.comparative = Some(ComparativeParagraph::try_new(text)?);
        // Changed copy must be re-approved; an old signature does not carry over.
        self.approval = Approval::default();
        self.comparative
            .as_ref()
            .ok_or_else(|| Error::invalid("comparative paragraph", "was not stored"))
    }

    /// Records a single-pass signature on the requisition's static copy.
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

    /// Records a single-pass signature on the comparative paragraph.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when no paragraph has been drafted, and
    /// propagates [`Approval::record`] errors.
    pub fn approve_comparative(
        &mut self,
        actor: impl Into<String>,
        role: SignOffRole,
        at: DateTime<Utc>,
    ) -> Result<()> {
        let paragraph = self.comparative.as_mut().ok_or_else(|| {
            Error::invalid(
                "comparative paragraph",
                "draft the paragraph before collecting sign-off",
            )
        })?;
        paragraph.approval.record(actor, role, at)?;
        Ok(())
    }

    /// Everything that still blocks dispatch, in the order it should be fixed.
    pub fn blocking_issues(&self, library: &GateLibrary) -> Vec<String> {
        let mut issues = Vec::new();
        for gate in &self.gates {
            if library.copy_for(*gate).is_none() {
                issues.push(format!("no approved gate copy for {gate}"));
            }
        }
        if !library.is_approved() {
            let missing: Vec<String> = library
                .approval
                .missing()
                .iter()
                .map(ToString::to_string)
                .collect();
            if library.is_complete() {
                issues.push(format!(
                    "gate copy library still needs sign-off from {}",
                    missing.join(", ")
                ));
            } else {
                issues.push("gate copy library is incomplete".to_owned());
            }
        }
        let missing = self.approval.missing();
        if !missing.is_empty() {
            let roles: Vec<String> = missing.iter().map(ToString::to_string).collect();
            issues.push(format!(
                "requisition copy still needs sign-off from {}",
                roles.join(", ")
            ));
        }
        match &self.comparative {
            None => issues.push("comparative paragraph is not drafted".to_owned()),
            Some(paragraph) if !paragraph.approval.is_approved() => {
                let roles: Vec<String> = paragraph
                    .approval
                    .missing()
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                issues.push(format!(
                    "comparative paragraph still needs sign-off from {}",
                    roles.join(", ")
                ));
            }
            Some(_) => {}
        }
        issues
    }

    /// Whether the requisition may be used to dispatch feedback.
    pub fn is_ready(&self, library: &GateLibrary) -> bool {
        self.blocking_issues(library).is_empty()
    }
}

/// Every requisition the pilot knows about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequisitionBook {
    items: Vec<Requisition>,
}

impl RequisitionBook {
    /// Adds a requisition, rejecting a duplicate id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the id is already in the book.
    pub fn insert(&mut self, requisition: Requisition) -> Result<()> {
        if self.items.iter().any(|item| item.id == requisition.id) {
            return Err(Error::invalid(
                "requisition id",
                format!("{} is already in the book", requisition.id),
            ));
        }
        self.items.push(requisition);
        Ok(())
    }

    /// Returns the requisition with `id`.
    pub fn get(&self, id: &RequisitionId) -> Option<&Requisition> {
        self.items.iter().find(|item| &item.id == id)
    }

    /// Returns the requisition with `id` for mutation.
    pub fn get_mut(&mut self, id: &RequisitionId) -> Option<&mut Requisition> {
        self.items.iter_mut().find(|item| &item.id == id)
    }

    /// Returns the requisition with `id`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownRequisition`] when the id is not in the book.
    pub fn require(&self, id: &RequisitionId) -> Result<&Requisition> {
        self.get(id)
            .ok_or_else(|| Error::UnknownRequisition(id.clone()))
    }

    /// Every requisition, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Requisition> {
        self.items.iter()
    }

    /// Whether the book has no requisitions.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
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

    fn requisition() -> Requisition {
        Requisition::new(
            RequisitionId::try_from("REQ-1").expect("id"),
            "Example Health",
            "Registered Nurse",
            Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
            [GateKind::RightToWork, GateKind::MandatoryRegistration],
            at(),
        )
        .expect("valid requisition")
    }

    #[test]
    fn requires_at_least_one_gate() {
        assert!(
            Requisition::new(
                RequisitionId::try_from("REQ-1").expect("id"),
                "Example Health",
                "Registered Nurse",
                Mailbox::try_from("careers@example.com.au").expect("mailbox"),
                [],
                at(),
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_gates_are_collapsed() {
        let req = Requisition::new(
            RequisitionId::try_from("REQ-1").expect("id"),
            "Example Health",
            "Registered Nurse",
            Mailbox::try_from("careers@example.com.au").expect("mailbox"),
            [GateKind::Location, GateKind::Location],
            at(),
        )
        .expect("valid requisition");
        assert_eq!(req.gates, vec![GateKind::Location]);
    }

    fn approved_library() -> GateLibrary {
        let mut library = GateLibrary::seed();
        for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
            library
                .approve("Approver", role, at())
                .expect("library sign-off");
        }
        library
    }

    #[test]
    fn readiness_needs_an_approved_library_and_both_signatures() {
        let library = approved_library();
        let mut req = requisition();
        assert!(!req.blocking_issues(&library).is_empty());

        req.set_comparative("Shortlisted applicants had three years of acute care.")
            .expect("paragraph");
        req.approve("J. Chen", SignOffRole::HiringManager, at())
            .expect("hm signature");
        req.approve("M. Okafor", SignOffRole::Legal, at())
            .expect("legal signature");
        req.approve_comparative("J. Chen", SignOffRole::HiringManager, at())
            .expect("hm signature");
        req.approve_comparative("M. Okafor", SignOffRole::Legal, at())
            .expect("legal signature");

        assert!(
            req.is_ready(&library),
            "{:?}",
            req.blocking_issues(&library)
        );
    }

    #[test]
    fn changing_the_comparative_paragraph_voids_approval() {
        let library = GateLibrary::seed();
        let mut req = requisition();
        req.set_comparative("First wording.").expect("paragraph");
        req.approve("J. Chen", SignOffRole::HiringManager, at())
            .expect("hm signature");
        req.approve("M. Okafor", SignOffRole::Legal, at())
            .expect("legal signature");
        assert!(req.approval.is_approved());

        req.set_comparative("Second wording.")
            .expect("second paragraph");
        assert!(!req.approval.is_approved());
        assert!(!req.is_ready(&library));
    }

    #[test]
    fn comparative_paragraph_rejects_merge_slots() {
        assert!(ComparativeParagraph::try_new("Dear {{candidate_name}},").is_err());
    }
}
