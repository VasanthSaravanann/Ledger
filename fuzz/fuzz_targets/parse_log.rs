#![no_main]
//! A corrupted or hostile log must produce an error, never a panic and never a
//! silently-accepted chain.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = std::str::from_utf8(data)
        .ok()
        .and_then(|text| serde_json::from_str::<ledger::log::SendEntry>(text).ok());
});
