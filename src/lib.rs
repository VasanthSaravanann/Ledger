//! Operational tooling for the v1 manual candidate feedback pilot.
//!
//! The crate implements `plan.md` as a small file-backed system rather than a
//! process document:
//!
//! - [`gate`] holds the five Tier 0 knockout gates and their pre-approved copy.
//! - [`requisition`] holds the static comparative paragraph and its sign-off.
//! - [`disposition`] is the locked, timestamped, no-free-text disposition form.
//! - [`template`] holds the three master merge templates.
//! - [`playbook`] holds the five canned reply scripts and their escalation.
//! - [`ats`] maps the ATS export onto dispositions by application id, with the
//!   hard initial-screen filter.
//! - [`disposition`] also carries corrections: a new record `SUPERSEDES` the old.
//! - [`qa`] records the second-person check and draws the deterministic 10% sample.
//! - [`log`] is the append-only, hash-chained send log and the blocked log.
//! - [`pipeline`] is the gate-blocking protocol that refuses to send without a
//!   completed disposition, a matching answer, and a recorded QA check.
//! - [`metrics`] and [`review`] are the five pre-registered metrics and the v2
//!   routing decision.
//! - [`policy`] records retention and the escalation owner; [`redact`] keeps PII
//!   out of console output.
//! - [`checklist`] tracks the operational tasks that a person, not a program,
//!   has to sign off.
//! - [`demo`] builds the deterministic synthetic fixture, and [`site`] writes the
//!   artefact pages from the loaded workspace at build time.

pub mod anchor;
pub mod approval;
pub mod ats;
pub mod checklist;
pub mod demo;
pub mod digest;
pub mod disposition;
pub mod error;
pub mod gate;
pub mod id;
pub mod identity;
pub mod jsonl;
pub mod log;
pub mod metrics;
pub mod migrate;
pub mod operator;
pub mod pipeline;
pub mod playbook;
pub mod policy;
pub mod qa;
pub mod redact;
pub mod requisition;
pub mod retention;
pub mod review;
pub mod site;
pub mod store;
pub mod template;
pub mod text;
