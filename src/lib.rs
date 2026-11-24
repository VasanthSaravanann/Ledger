//! Operational tooling for the v1 manual candidate feedback pilot.
//!
//! The crate implements `plan.md` as a small file-backed system rather than a
//! process document:
//!
//! - [`gate`] holds the five Tier 0 knockout gates and their pre-approved copy.
//! - [`requisition`] holds the static comparative paragraph and its sign-off.
//! - [`disposition`] is the locked, timestamped, no-free-text disposition form.
//! - [`template`] holds the three master merge templates.
//! - [`ats`] maps the ATS export onto dispositions by application id.
//! - [`log`] is the append-only, hash-chained send log and the blocked log.
//! - [`pipeline`] is the gate-blocking protocol that refuses to send without a
//!   completed disposition.
//! - [`metrics`] and [`review`] are the three decision metrics and the go/no-go.
//! - [`checklist`] tracks the operational tasks that a person, not a program,
//!   has to sign off.

pub mod approval;
pub mod ats;
pub mod checklist;
pub mod digest;
pub mod disposition;
pub mod error;
pub mod gate;
pub mod id;
pub mod jsonl;
pub mod log;
pub mod metrics;
pub mod pipeline;
pub mod requisition;
pub mod review;
pub mod store;
pub mod template;
pub mod text;
