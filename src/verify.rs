//! Workspace-wide verification, reusable by the CLI and the web UI (V5 4.x).
//!
//! The CLI used to compose this inside its handler. It is a report type now, so the
//! `ledger ui` governance and audit screens render exactly what `verify-signatures`
//! would print, rather than reimplementing the verdict.

use serde::{Deserialize, Serialize};

use crate::approval::ArtefactStatus;
use crate::disposition::Dispositions;
use crate::error::Result;
use crate::id::TemplateKind;
use crate::store::Store;

/// One artefact's verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtefactReport {
    /// Stable label, for example `gate-library` or `template-field`.
    pub label: String,
    /// The verdict.
    pub status: ArtefactStatus,
}

/// Everything `verify-signatures` checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    /// Dispositions whose signature verifies.
    pub dispositions_signed: usize,
    /// Dispositions with no signature.
    pub dispositions_unsigned: usize,
    /// Disposition record ids whose signature does not verify.
    pub dispositions_invalid: Vec<String>,
    /// The five sign-off sites, plus one per extra requisition.
    pub artefacts: Vec<ArtefactReport>,
}

impl VerificationReport {
    /// Whether every checked thing is in order.
    pub fn is_clean(&self) -> bool {
        self.dispositions_invalid.is_empty()
            && self
                .artefacts
                .iter()
                .all(|artefact| artefact.status.is_valid())
    }

    /// Artefacts that are stale, with the reason, for a UI badge.
    pub fn stale_labels(&self) -> Vec<String> {
        self.artefacts
            .iter()
            .filter(|artefact| matches!(artefact.status, ArtefactStatus::Stale { .. }))
            .map(|artefact| artefact.label.clone())
            .collect()
    }

    /// Artefacts that are unsigned.
    pub fn unsigned_labels(&self) -> Vec<String> {
        self.artefacts
            .iter()
            .filter(|artefact| matches!(artefact.status, ArtefactStatus::Unsigned))
            .map(|artefact| artefact.label.clone())
            .collect()
    }

    /// Artefacts whose signature failed to verify.
    pub fn invalid_labels(&self) -> Vec<String> {
        self.artefacts
            .iter()
            .filter(|artefact| matches!(artefact.status, ArtefactStatus::Invalid { .. }))
            .map(|artefact| artefact.label.clone())
            .collect()
    }
}

/// Verifies dispositions, the five sign-off sites, and every requisition.
///
/// # Errors
///
/// Propagates read errors and any malformed stored artefact.
pub fn verify_workspace(store: &Store) -> Result<VerificationReport> {
    let dispositions = Dispositions::open(store)?;
    let mut signed = 0;
    let mut unsigned = 0;
    let mut invalid = Vec::new();
    for record in dispositions.records() {
        match &record.form.signature {
            None => unsigned += 1,
            Some(_) if record.form.signature_valid() => signed += 1,
            Some(_) => invalid.push(record.id.to_string()),
        }
    }

    let library: crate::gate::GateLibrary = store
        .load(crate::store::GATE_LIBRARY)?
        .ok_or_else(|| crate::error::Error::invalid("workspace", "no gate library"))?;
    let templates: crate::template::Templates = store
        .load(crate::store::TEMPLATES)?
        .ok_or_else(|| crate::error::Error::invalid("workspace", "no templates"))?;
    let playbook: crate::playbook::Playbook = store
        .load(crate::store::PLAYBOOK)?
        .ok_or_else(|| crate::error::Error::invalid("workspace", "no reply playbook"))?;
    let book = crate::requisition::RequisitionBook::load(store)?;

    let mut artefacts = Vec::new();
    artefacts.push(ArtefactReport {
        label: "gate-library".to_owned(),
        status: library.approval.artefact_status(&library.content_hash()),
    });
    for kind in TemplateKind::ALL {
        let template = templates.require(kind)?;
        artefacts.push(ArtefactReport {
            label: format!("template-{}", kind.code()),
            status: template.approval.artefact_status(&template.content_hash()),
        });
    }
    artefacts.push(ArtefactReport {
        label: "playbook".to_owned(),
        status: playbook.approval.artefact_status(&playbook.content_hash()),
    });
    for requisition in book.iter() {
        artefacts.push(ArtefactReport {
            label: format!("requisition-{}", requisition.id),
            status: requisition
                .approval
                .artefact_status(&requisition.content_hash()),
        });
        if let Some(paragraph) = &requisition.comparative {
            artefacts.push(ArtefactReport {
                label: format!("comparative-{}", requisition.id),
                status: paragraph
                    .approval
                    .artefact_status(&paragraph.content_hash()),
            });
        }
    }

    Ok(VerificationReport {
        dispositions_signed: signed,
        dispositions_unsigned: unsigned,
        dispositions_invalid: invalid,
        artefacts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-verify-{tag}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");
        crate::demo::seed(&store, false, false).expect("fixture");
        (store, dir)
    }

    #[test]
    fn a_fresh_fixture_is_clean() {
        let (store, dir) = fixture("clean");
        let report = verify_workspace(&store).expect("verify");
        assert_eq!(report.dispositions_unsigned, 0);
        assert!(report.dispositions_invalid.is_empty());
        assert_eq!(
            report.artefacts.len(),
            7,
            "library, 3 templates, playbook, req, para"
        );
        assert!(report.is_clean(), "{:?}", report.artefacts);
        assert!(report.stale_labels().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edited_block_turns_the_library_stale() {
        let (store, dir) = fixture("stale");
        let mut library: crate::gate::GateLibrary = store
            .load(crate::store::GATE_LIBRARY)
            .expect("load")
            .expect("seeded");
        library
            .bump_version(
                crate::gate::GateKind::Licence,
                crate::id::TextVersion::parse("v9.9").expect("version"),
            )
            .expect("bump");
        store
            .save(crate::store::GATE_LIBRARY, &library)
            .expect("save");

        let report = verify_workspace(&store).expect("verify");
        assert!(!report.is_clean());
        assert_eq!(report.stale_labels(), vec!["gate-library".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
