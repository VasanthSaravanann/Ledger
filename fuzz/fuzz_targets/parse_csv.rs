#![no_main]
//! The ATS importer is the only place untrusted, externally-authored text enters
//! the pilot, so it is the first fuzz target.

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let requisition = ledger::id::RequisitionId::try_from("REQ-2026-014").expect("constant id");
    let _ = ledger::ats::Roster::from_csv(&requisition, Cursor::new(data.to_vec()));
});
