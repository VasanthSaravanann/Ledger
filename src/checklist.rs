//! The `plan.md` task sheet.
//!
//! The pilot has thirteen tasks, most of them operational rather than
//! programmable (legal review, a monitored mailbox, a legal sign-off). This module
//! keeps the sheet honest: a task is done only when a named person says so, and
//! the CLI prints the sheet next to the evidence the tool can actually count.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::store::{self, Store};
use crate::text;

/// One task from `plan.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanTask {
    /// The task id used by the CLI, for example `"2.1"`.
    pub id: &'static str,
    /// The plan phase the task belongs to.
    pub phase: u8,
    /// The task title.
    pub title: &'static str,
}

/// Every task in `plan.md`, in order.
pub const TASKS: [PlanTask; 13] = [
    PlanTask {
        id: "1.1",
        phase: 1,
        title: "APP 1.7-1.9 privacy compliance check",
    },
    PlanTask {
        id: "1.2",
        phase: 1,
        title: "Establish approval workflow",
    },
    PlanTask {
        id: "1.3",
        phase: 1,
        title: "Provision monitored reply mailbox",
    },
    PlanTask {
        id: "2.1",
        phase: 2,
        title: "Build the single-screen disposition form",
    },
    PlanTask {
        id: "2.2",
        phase: 2,
        title: "Draft the gate-text library (Tier 0)",
    },
    PlanTask {
        id: "2.3",
        phase: 2,
        title: "Draft the role-level comparative paragraph (Tier 2)",
    },
    PlanTask {
        id: "2.4",
        phase: 2,
        title: "Create the three master email templates",
    },
    PlanTask {
        id: "3.1",
        phase: 3,
        title: "Configure ATS CSV export and merge mapping",
    },
    PlanTask {
        id: "3.2",
        phase: 3,
        title: "Build the immutable send log",
    },
    PlanTask {
        id: "3.3",
        phase: 3,
        title: "Establish gate-blocking protocol",
    },
    PlanTask {
        id: "4.1",
        phase: 4,
        title: "Execute pilot on target requisition",
    },
    PlanTask {
        id: "4.2",
        phase: 4,
        title: "Instrument and track the three v2 decision metrics",
    },
    PlanTask {
        id: "4.3",
        phase: 4,
        title: "Post-pilot go/no-go review",
    },
];

/// Returns the task with `id`.
///
/// # Errors
///
/// Returns [`Error::UnknownPlanTask`] when the id is not in `plan.md`.
pub fn task(id: &str) -> Result<PlanTask> {
    TASKS
        .into_iter()
        .find(|task| task.id == id.trim())
        .ok_or_else(|| Error::UnknownPlanTask(id.trim().to_owned()))
}

/// One task marked done by a named person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    /// The task id.
    pub task: String,
    /// Who confirmed it.
    pub actor: String,
    /// When.
    pub at: DateTime<Utc>,
    /// Optional evidence reference, such as a ticket or document id.
    pub reference: Option<String>,
}

/// The completed tasks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Checklist {
    completions: Vec<Completion>,
}

impl Checklist {
    /// Loads the checklist.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn load(store: &Store) -> Result<Self> {
        Ok(store.load(store::CHECKLIST)?.unwrap_or_default())
    }

    /// Saves the checklist.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(store::CHECKLIST, self)
    }

    /// Marks a task done, at most once.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownPlanTask`] for an unknown id, [`Error::Invalid`]
    /// when the actor is blank, and [`Error::Invalid`] when the task is already
    /// marked done.
    pub fn mark(
        &mut self,
        task_id: &str,
        actor: impl Into<String>,
        at: DateTime<Utc>,
        reference: Option<String>,
    ) -> Result<&Completion> {
        let task = task(task_id)?;
        if self
            .completions
            .iter()
            .any(|completion| completion.task == task.id)
        {
            return Err(Error::invalid(
                "plan task",
                format!("task {} is already marked done", task.id),
            ));
        }
        let actor = text::required("checklist actor", actor.into(), 120)?;
        self.completions.push(Completion {
            task: task.id.to_owned(),
            actor,
            at,
            reference: reference
                .map(|reference| reference.trim().to_owned())
                .filter(|reference| !reference.is_empty()),
        });
        self.completions
            .last()
            .ok_or_else(|| Error::invalid("plan task", "completion was not stored"))
    }

    /// Whether `task_id` is marked done.
    pub fn is_done(&self, task_id: &str) -> bool {
        self.completions
            .iter()
            .any(|completion| completion.task == task_id)
    }

    /// The completion record for `task_id`.
    pub fn completion(&self, task_id: &str) -> Option<&Completion> {
        self.completions
            .iter()
            .find(|completion| completion.task == task_id)
    }

    /// How many tasks are done, out of the total in `plan.md`.
    pub fn progress(&self) -> (usize, usize) {
        (self.completions.len(), TASKS.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_has_every_task_in_plan_order() {
        assert_eq!(TASKS.len(), 13);
        let ids: Vec<&str> = TASKS.iter().map(|task| task.id).collect();
        assert_eq!(
            ids,
            vec![
                "1.1", "1.2", "1.3", "2.1", "2.2", "2.3", "2.4", "3.1", "3.2", "3.3", "4.1", "4.2",
                "4.3"
            ]
        );
        assert_eq!(
            task("9.9").expect_err("unknown task").to_string(),
            "unknown plan task 9.9"
        );
    }

    #[test]
    fn a_task_is_marked_once() {
        let mut checklist = Checklist::default();
        let at = Utc::now();
        checklist
            .mark("2.1", "R. Patel", at, Some("form-42".to_owned()))
            .expect("first mark");
        assert!(checklist.is_done("2.1"));
        assert_eq!(checklist.progress(), (1, TASKS.len()));
        assert!(checklist.mark("2.1", "R. Patel", at, None).is_err());
        assert!(checklist.mark("1.1", "  ", at, None).is_err());
    }
}
