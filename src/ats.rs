//! The ATS export and its mapping onto disposition records (Task 3.1).
//!
//! The ATS is the source of contact details and of the candidate's own knockout
//! answers; the form record is joined to it on the application id. The roster is
//! snapshotted so a later ATS export cannot silently change who an email went to.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::gate::{GateAnswer, GateKind};
use crate::id::{ApplicationId, CandidateEmail, RequisitionId};
use crate::store::Store;
use crate::text;

/// Header names accepted for the application id column.
const APPLICATION_HEADERS: [&str; 2] = ["application_id", "app_id"];
/// Header names accepted for the candidate name column.
const NAME_HEADERS: [&str; 2] = ["candidate_name", "name"];
/// Header names accepted for the email column.
const EMAIL_HEADERS: [&str; 2] = ["email", "candidate_email"];

/// One row of the ATS export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applicant {
    /// The ATS application id, the join key for the form.
    pub application_id: ApplicationId,
    /// The candidate's name, used only as a merge value.
    pub name: String,
    /// The candidate's email.
    pub email: CandidateEmail,
    /// Knockout answers the candidate already gave, where the export carries them.
    pub knockouts: BTreeMap<GateKind, GateAnswer>,
}

impl Applicant {
    /// The candidate's own answer to `gate`, if the export carried it.
    pub fn knockout(&self, gate: GateKind) -> Option<GateAnswer> {
        self.knockouts.get(&gate).copied()
    }
}

/// Every applicant on one requisition, as exported from the ATS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roster {
    /// The requisition the export belongs to.
    pub requisition: RequisitionId,
    /// The applicants, in export order.
    pub applicants: Vec<Applicant>,
}

impl Roster {
    /// Reads the ATS export.
    ///
    /// Required columns are an application id, a candidate name, and an email.
    /// Optional columns named `knockout_<gate>` carry the candidate's knockout
    /// answers.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for missing columns, blank or malformed cells,
    /// and [`Error::DuplicateApplication`] when an application id repeats; and
    /// [`Error::Csv`] when the file is not readable as CSV.
    pub fn from_csv(requisition: &RequisitionId, reader: impl Read) -> Result<Self> {
        let mut csv = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(reader);
        let headers = csv.headers()?.clone();
        let lowered: Vec<String> = headers
            .iter()
            .map(|header| header.trim().to_lowercase())
            .collect();

        let column = |candidates: &[&str], kind: &'static str| -> Result<usize> {
            candidates
                .iter()
                .find_map(|name| lowered.iter().position(|header| header == name))
                .ok_or_else(|| {
                    Error::invalid(
                        "ATS export",
                        format!("missing {kind} column (expected one of {candidates:?})"),
                    )
                })
        };
        let application_column = column(&APPLICATION_HEADERS, "application id")?;
        let name_column = column(&NAME_HEADERS, "candidate name")?;
        let email_column = column(&EMAIL_HEADERS, "email")?;

        let knockout_columns: Vec<(GateKind, usize)> = GateKind::ALL
            .into_iter()
            .filter_map(|gate| {
                let header = format!("knockout_{}", gate.code());
                lowered
                    .iter()
                    .position(|candidate| candidate == &header)
                    .map(|index| (gate, index))
            })
            .collect();

        let mut applicants = Vec::new();
        let mut seen = BTreeSet::new();
        for (offset, record) in csv.records().enumerate() {
            let record = record?;
            let row = offset + 2;
            if record.iter().all(|cell| cell.trim().is_empty()) {
                continue;
            }
            let cell = |index: usize| record.get(index).unwrap_or_default();
            let application_id = ApplicationId::try_from(cell(application_column))
                .map_err(|error| annotate(error, row, "application id"))?;
            if !seen.insert(application_id.clone()) {
                return Err(Error::DuplicateApplication(application_id));
            }
            let email = CandidateEmail::try_from(cell(email_column))
                .map_err(|error| annotate(error, row, "email"))?;
            let name = text::required("candidate name", cell(name_column), 120)
                .map_err(|error| annotate(error, row, "candidate name"))?;

            let mut knockouts = BTreeMap::new();
            for (gate, index) in &knockout_columns {
                let value = cell(*index);
                if value.is_empty() {
                    continue;
                }
                let answer = value
                    .parse::<GateAnswer>()
                    .map_err(|error| annotate(error, row, "knockout answer"))?;
                if !gate.allowed_answers().contains(&answer) {
                    return Err(Error::invalid(
                        "ATS export",
                        format!(
                            "row {row}: {answer} is not an allowed answer for knockout_{}",
                            gate.code()
                        ),
                    ));
                }
                knockouts.insert(*gate, answer);
            }

            applicants.push(Applicant {
                application_id,
                name,
                email,
                knockouts,
            });
        }

        Ok(Self {
            requisition: requisition.clone(),
            applicants,
        })
    }

    /// Returns the applicant with `application_id`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownApplication`] when the id is not in the roster.
    pub fn require(&self, application_id: &ApplicationId) -> Result<&Applicant> {
        self.applicants
            .iter()
            .find(|applicant| &applicant.application_id == application_id)
            .ok_or_else(|| Error::UnknownApplication(application_id.clone()))
    }

    /// Loads the saved roster for `requisition`, if one was imported.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn load(store: &Store, requisition: &RequisitionId) -> Result<Option<Self>> {
        store.load(&Store::roster_file(requisition))
    }

    /// Saves the roster.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(&Store::roster_file(&self.requisition), self)
    }
}

/// Adds row and column context to a validation error.
fn annotate(error: Error, row: usize, column: &str) -> Error {
    match error {
        Error::Invalid { kind, reason } => {
            Error::invalid(kind, format!("row {row} ({column}): {reason}"))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn requisition() -> RequisitionId {
        RequisitionId::try_from("REQ-1").expect("id")
    }

    #[test]
    fn maps_rows_and_knockout_answers() {
        let csv = "\
application_id,candidate_name,email,knockout_visa-restrictions
APP-1,Ada Lovelace,ada@example.com,restricted
APP-2,Grace Hopper,grace@example.com,
";
        let roster = Roster::from_csv(&requisition(), Cursor::new(csv)).expect("roster");
        assert_eq!(roster.applicants.len(), 2);
        assert_eq!(
            roster.applicants[0].knockout(GateKind::VisaRestrictions),
            Some(GateAnswer::Restricted)
        );
        assert_eq!(
            roster.applicants[1].knockout(GateKind::VisaRestrictions),
            None
        );
        assert!(
            roster
                .require(&ApplicationId::try_from("APP-2").expect("id"))
                .is_ok()
        );
    }

    #[test]
    fn rejects_missing_columns() {
        let csv = "application_id,candidate_name\nAPP-1,Ada\n";
        assert!(Roster::from_csv(&requisition(), Cursor::new(csv)).is_err());
    }

    #[test]
    fn rejects_duplicate_application_ids() {
        let csv = "\
application_id,candidate_name,email
APP-1,Ada Lovelace,ada@example.com
APP-1,Grace Hopper,grace@example.com
";
        assert!(matches!(
            Roster::from_csv(&requisition(), Cursor::new(csv)),
            Err(Error::DuplicateApplication(_))
        ));
    }

    #[test]
    fn rejects_a_knockout_answer_the_gate_does_not_offer() {
        let csv = "\
application_id,candidate_name,email,knockout_location
APP-1,Ada Lovelace,ada@example.com,expired
";
        assert!(Roster::from_csv(&requisition(), Cursor::new(csv)).is_err());
    }
}
