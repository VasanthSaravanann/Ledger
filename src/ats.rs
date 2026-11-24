//! The ATS export and its hard initial-screen filter (Task 3.1).
//!
//! The export is the source of contact details, the requisition, and the
//! candidate's own knockout answers (`KO_ANSWER_01`..`KO_ANSWER_05`, which map to
//! `GATE-01`..`GATE-05` in order). Rows whose `STAGE_AT_REJECTION` is not the
//! initial screen are excluded at import, so an interviewed candidate can never
//! receive `Template B`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::gate::{GateAnswer, GateKind};
use crate::id::{ApplicationId, CandidateEmail, RequisitionId};
use crate::store::Store;
use crate::text;

/// The stage a row was rejected at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// The initial screen: the only stage this pilot writes to.
    InitialScreen,
}

impl Stage {
    /// Parses the export value, accepting the spellings an ATS actually emits.
    pub fn parse(value: &str) -> Option<Self> {
        let normalised = value
            .trim()
            .to_lowercase()
            .replace(['_', '-'], " ")
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ");
        match normalised.as_str() {
            "initial screen" | "initial screening" | "screen" | "initial" | "knockout screen" => {
                Some(Self::InitialScreen)
            }
            _ => None,
        }
    }

    /// The stored code.
    pub fn code(self) -> &'static str {
        match self {
            Self::InitialScreen => "initial-screen",
        }
    }
}

/// One row of the ATS export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applicant {
    /// `APPLICATION_ID`, the join key.
    pub application_id: ApplicationId,
    /// `FIRST_NAME`.
    pub first_name: String,
    /// `EMAIL`.
    pub email: CandidateEmail,
    /// `STAGE_AT_REJECTION`, always the initial screen.
    pub stage: Stage,
    /// `APPLIED_DATE`, when the export carried one.
    pub applied_at: Option<DateTime<Utc>>,
    /// `KO_ANSWER_0n`, keyed by the gate the column belongs to.
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
    /// How many rows were excluded because they were past the initial screen.
    #[serde(default)]
    pub excluded_not_initial_screen: usize,
}

impl Roster {
    /// Reads the ATS export.
    ///
    /// Required columns are `APPLICATION_ID`, `FIRST_NAME`, `EMAIL`, `REQ_ID`, and
    /// `STAGE_AT_REJECTION`. `ROLE_TITLE` and `APPLIED_DATE` are read when
    /// present. `KO_ANSWER_01`..`KO_ANSWER_05` carry the candidate's answers.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for missing columns, a `REQ_ID` that contradicts
    /// the requisition, or a malformed cell; and [`Error::DuplicateApplication`]
    /// when an id repeats.
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
        let application_column = column(&["application_id", "app_id"], "APPLICATION_ID")?;
        let first_name_column = column(&["first_name", "candidate_name", "name"], "FIRST_NAME")?;
        let email_column = column(&["email", "candidate_email"], "EMAIL")?;
        let requisition_column = column(&["req_id", "requisition_id"], "REQ_ID")?;
        let stage_column = column(&["stage_at_rejection", "stage"], "STAGE_AT_REJECTION")?;
        let applied_column = lowered.iter().position(|header| header == "applied_date");

        let knockout_columns: Vec<(GateKind, usize)> = GateKind::ALL
            .into_iter()
            .filter_map(|gate| {
                let header = format!("ko_answer_{:02}", gate.ordinal());
                lowered
                    .iter()
                    .position(|candidate| candidate == &header)
                    .map(|index| (gate, index))
            })
            .collect();

        let mut applicants = Vec::new();
        let mut seen = BTreeSet::new();
        let mut excluded = 0;
        for (offset, record) in csv.records().enumerate() {
            let record = record?;
            let row = offset + 2;
            if record.iter().all(|cell| cell.trim().is_empty()) {
                continue;
            }
            let cell = |index: usize| record.get(index).unwrap_or_default();

            let row_requisition = cell(requisition_column).trim();
            if !row_requisition.eq_ignore_ascii_case(requisition.as_str()) {
                return Err(Error::invalid(
                    "ATS export",
                    format!(
                        "row {row}: REQ_ID {row_requisition:?} does not match requisition {requisition}"
                    ),
                ));
            }
            let Some(stage) = Stage::parse(cell(stage_column)) else {
                // Hard filter: anything past the initial screen never enters the roster.
                excluded += 1;
                continue;
            };
            debug_assert_eq!(stage, Stage::InitialScreen);

            let application_id = ApplicationId::try_from(cell(application_column))
                .map_err(|error| annotate(error, row, "APPLICATION_ID"))?;
            if !seen.insert(application_id.clone()) {
                return Err(Error::DuplicateApplication(application_id));
            }
            let email = CandidateEmail::try_from(cell(email_column))
                .map_err(|error| annotate(error, row, "EMAIL"))?;
            let first_name = text::required("first name", cell(first_name_column), 120)
                .map_err(|error| annotate(error, row, "FIRST_NAME"))?;

            let applied_at = applied_column
                .map(&cell)
                .filter(|value| !value.trim().is_empty())
                .map(|value| parse_date(value, row))
                .transpose()?;

            let mut knockouts = BTreeMap::new();
            for (gate, index) in &knockout_columns {
                let value = cell(*index);
                if value.is_empty() {
                    continue;
                }
                let answer = value
                    .parse::<GateAnswer>()
                    .map_err(|error| annotate(error, row, "KO_ANSWER"))?;
                if !gate.allowed_answers().contains(&answer) {
                    return Err(Error::invalid(
                        "ATS export",
                        format!(
                            "row {row}: {answer} is not an allowed answer for KO_ANSWER_{:02} ({gate})",
                            gate.ordinal()
                        ),
                    ));
                }
                knockouts.insert(*gate, answer);
            }

            applicants.push(Applicant {
                application_id,
                first_name,
                email,
                stage,
                applied_at,
                knockouts,
            });
        }

        Ok(Self {
            requisition: requisition.clone(),
            applicants,
            excluded_not_initial_screen: excluded,
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

    /// Loads the saved roster for `requisition`.
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

/// Parses an `APPLIED_DATE` cell.
fn parse_date(value: &str, row: usize) -> Result<DateTime<Utc>> {
    let trimmed = value.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d", "%d/%m/%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, format) {
            return Ok(date
                .and_hms_opt(0, 0, 0)
                .map(|naive| naive.and_utc())
                .unwrap_or_default());
        }
    }
    Err(Error::invalid(
        "ATS export",
        format!("row {row}: APPLIED_DATE {trimmed:?} is not a date"),
    ))
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

    const HEADER: &str = "APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,ROLE_TITLE,STAGE_AT_REJECTION,\
                          KO_ANSWER_01,KO_ANSWER_02,APPLIED_DATE\n";

    fn requisition() -> RequisitionId {
        RequisitionId::try_from("REQ-1").expect("id")
    }

    #[test]
    fn maps_rows_and_numbered_knockout_answers() {
        let csv = format!(
            "{HEADER}APP-1,Ada,ada@example.com,REQ-1,Registered Nurse,Initial screen,not-held,,\
             2026-11-01\nAPP-2,Grace,grace@example.com,REQ-1,Registered Nurse,Initial Screen,,,\
             2026-11-02\n"
        );
        let roster = Roster::from_csv(&requisition(), Cursor::new(csv)).expect("roster");
        assert_eq!(roster.applicants.len(), 2);
        assert_eq!(
            roster.applicants[0].knockout(GateKind::RightToWork),
            Some(GateAnswer::NotHeld)
        );
        assert!(roster.applicants[0].applied_at.is_some());
        assert_eq!(roster.excluded_not_initial_screen, 0);
    }

    #[test]
    fn a_row_past_the_initial_screen_is_excluded() {
        let csv = format!(
            "{HEADER}APP-1,Ada,ada@example.com,REQ-1,Registered Nurse,Initial screen,,,\n\
             APP-2,Grace,grace@example.com,REQ-1,Registered Nurse,Interviewed,,,\n\
             APP-3,Katherine,katherine@example.com,REQ-1,Registered Nurse,Offer,,,\n"
        );
        let roster = Roster::from_csv(&requisition(), Cursor::new(csv)).expect("roster");
        assert_eq!(roster.applicants.len(), 1);
        assert_eq!(roster.excluded_not_initial_screen, 2);
        assert!(
            roster
                .require(&ApplicationId::try_from("APP-2").expect("id"))
                .is_err(),
            "an interviewed candidate must never enter the roster"
        );
    }

    #[test]
    fn a_row_for_another_requisition_is_rejected() {
        let csv =
            format!("{HEADER}APP-1,Ada,ada@example.com,REQ-9,Registered Nurse,Initial screen,,,\n");
        assert!(Roster::from_csv(&requisition(), Cursor::new(csv)).is_err());
    }

    #[test]
    fn duplicate_application_ids_are_rejected() {
        let csv = format!(
            "{HEADER}APP-1,Ada,ada@example.com,REQ-1,Registered Nurse,Initial screen,,,\n\
             APP-1,Grace,grace@example.com,REQ-1,Registered Nurse,Initial screen,,,\n"
        );
        assert!(matches!(
            Roster::from_csv(&requisition(), Cursor::new(csv)),
            Err(Error::DuplicateApplication(_))
        ));
    }

    #[test]
    fn a_knockout_answer_the_gate_does_not_offer_is_rejected() {
        let csv = format!(
            "{HEADER}APP-1,Ada,ada@example.com,REQ-1,Registered Nurse,Initial screen,expired,,\n"
        );
        assert!(Roster::from_csv(&requisition(), Cursor::new(csv)).is_err());
    }

    #[test]
    fn stage_spellings_in_the_wild_parse() {
        for value in [
            "Initial screen",
            "initial-screen",
            "INITIAL_SCREEN",
            "screen",
        ] {
            assert_eq!(Stage::parse(value), Some(Stage::InitialScreen), "{value:?}");
        }
        for value in ["Interviewed", "shortlisted", "", "offer"] {
            assert_eq!(Stage::parse(value), None, "{value:?}");
        }
    }
}
