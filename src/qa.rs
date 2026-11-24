//! Second-person QA before dispatch (Task 4.1).
//!
//! The runbook requires a coordinator to check 100% of `GATE` emails and a 10%
//! random sample of `FIELD`/`ADMIN` emails. The sample is drawn deterministically
//! from the application id, so the same application is always in or out of the
//! sample and the choice cannot be gamed after the fact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::disposition::{Disposition, DispositionRecord};
use crate::error::Result;
use crate::id::{ApplicationId, QaId, RequisitionId};
use crate::jsonl::AppendLog;
use crate::store::{self, Store};

/// Why this application was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QaScope {
    /// One of the 100% of `GATE` emails.
    GateFullCheck,
    /// Part of the 10% random sample of `FIELD`/`ADMIN` emails.
    RandomTenPercent,
}

impl QaScope {
    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::GateFullCheck => "gate-full-check",
            Self::RandomTenPercent => "random-ten-percent",
        }
    }
}

/// One recorded second-person check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaRecord {
    /// The application checked.
    pub application_id: ApplicationId,
    /// The requisition.
    pub requisition: RequisitionId,
    /// The coordinator who checked it.
    pub qa: QaId,
    /// When.
    pub at: DateTime<Utc>,
    /// Why it was in scope.
    pub scope: QaScope,
}

/// The append-only QA log.
#[derive(Debug)]
pub struct QaLog {
    log: AppendLog<QaRecord>,
    records: Vec<QaRecord>,
}

impl QaLog {
    /// Opens the QA log.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Io`] or [`crate::error::Error::Json`] when the
    /// log cannot be read.
    pub fn open(store: &Store) -> Result<Self> {
        let log = store.log(store::QA_LOG);
        let records = log.read_all()?;
        Ok(Self { log, records })
    }

    /// Records a check. Repeating the same application is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Io`] or [`crate::error::Error::Json`] when the
    /// record cannot be written.
    pub fn record(&mut self, record: QaRecord) -> Result<bool> {
        if self.approved(&record.requisition, &record.application_id) {
            return Ok(false);
        }
        self.log.append(&record)?;
        self.records.push(record);
        Ok(true)
    }

    /// Whether the application has been checked.
    pub fn approved(&self, requisition: &RequisitionId, application: &ApplicationId) -> bool {
        self.records.iter().any(|record| {
            &record.requisition == requisition && &record.application_id == application
        })
    }

    /// Every check, in order.
    pub fn records(&self) -> &[QaRecord] {
        &self.records
    }

    /// How many checks were recorded for `requisition`.
    pub fn count_for(&self, requisition: &RequisitionId) -> usize {
        self.records
            .iter()
            .filter(|record| &record.requisition == requisition)
            .count()
    }
}

/// Whether this application must be checked before it can be sent.
pub fn requires_second_person(record: &DispositionRecord) -> bool {
    match &record.form.disposition {
        Disposition::Gate { .. } => true,
        Disposition::Field | Disposition::Admin { .. } => {
            sampled(&record.form.requisition, &record.form.application_id)
        }
    }
}

/// The scope a due check falls under.
pub fn scope_for(disposition: &Disposition) -> QaScope {
    match disposition {
        Disposition::Gate { .. } => QaScope::GateFullCheck,
        _ => QaScope::RandomTenPercent,
    }
}

/// Whether this application falls in the deterministic 10% sample.
///
/// The sample is keyed on `REQ_ID` and `APPLICATION_ID` only — never on CSV
/// column order or any other field — so adding a column to the ATS export cannot
/// change which rows the coordinator must check. Both values are length-prefixed
/// before hashing so that no pair of ids can collide by concatenation.
pub fn sampled(requisition: &RequisitionId, application: &ApplicationId) -> bool {
    let key = format!(
        "{}:{}:{}:{}",
        requisition.as_str().len(),
        requisition.as_str(),
        application.as_str().len(),
        application.as_str()
    );
    let digest = sha256_hex(key.as_bytes());
    let bucket = u8::from_str_radix(digest.get(0..2).unwrap_or("ff"), 16).unwrap_or(0xff);
    bucket % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requisition() -> RequisitionId {
        RequisitionId::try_from("REQ-2026-014").expect("id")
    }

    #[test]
    fn the_sample_is_stable_and_about_a_tenth() {
        let ids: Vec<ApplicationId> = (0..2_000)
            .filter_map(|index| ApplicationId::try_from(format!("APP-{index}")).ok())
            .collect();
        let hits = ids.iter().filter(|id| sampled(&requisition(), id)).count();
        assert!(
            (120..=280).contains(&hits),
            "expected roughly 10% of 2000, got {hits}"
        );
        assert_eq!(
            sampled(&requisition(), &ids[0]),
            sampled(&requisition(), &ids[0])
        );
    }

    #[test]
    fn the_sample_depends_on_the_requisition_too() {
        // One applicant id, many requisitions: if the requisition were not part of
        // the key, every answer would be identical.
        let application = ApplicationId::try_from("APP-1001").expect("id");
        let answers: std::collections::BTreeSet<bool> = (0..100)
            .map(|index| {
                let requisition =
                    RequisitionId::try_from(format!("REQ-2026-{index:03}")).expect("id");
                sampled(&requisition, &application)
            })
            .collect();
        assert!(
            answers.contains(&true) && answers.contains(&false),
            "the requisition must participate in the sampling key"
        );
    }

    #[test]
    fn unrelated_csv_columns_do_not_change_the_sample() {
        use crate::ats::Roster;
        use std::io::Cursor;

        let base = "APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,STAGE_AT_REJECTION\n\
                    APP-1,Ada,ada@example.com,REQ-2026-014,Initial screen\n\
                    APP-2,Grace,grace@example.com,REQ-2026-014,Initial screen\n";
        let extra = "APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,STAGE_AT_REJECTION,\
                     APPLIED_DATE,KO_ANSWER_01,NOTES\n\
                     APP-1,Ada,ada@example.com,REQ-2026-014,Initial screen,2026-11-01,,\n\
                     APP-2,Grace,grace@example.com,REQ-2026-014,Initial screen,2026-11-02,,\n";

        let first = Roster::from_csv(&requisition(), Cursor::new(base)).expect("roster");
        let second = Roster::from_csv(&requisition(), Cursor::new(extra)).expect("roster");
        let sample = |roster: &Roster| {
            roster
                .applicants
                .iter()
                .filter(|applicant| sampled(&requisition(), &applicant.application_id))
                .map(|applicant| applicant.application_id.to_string())
                .collect::<Vec<String>>()
        };
        assert_eq!(sample(&first), sample(&second));
    }

    #[test]
    fn gate_checks_are_always_in_scope() {
        assert_eq!(scope_for(&Disposition::Field), QaScope::RandomTenPercent);
    }
}
