//! PII-safe rendering of identifiers in errors and operator logs.
//!
//! Application ids and addresses are still written verbatim to the workspace, where
//! an operator with 0600 access can read them. What changes here is the *console*:
//! stderr is the most likely place for a candidate's email to leak into a CI log, a
//! screenshot, or a support ticket, so it carries a stable hash prefix instead.
//!
//! `--debug-ids` (or `LEDGER_DEBUG_IDS=1`) turns redaction off for local debugging.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::digest::sha256_hex;

static DEBUG_IDS: AtomicBool = AtomicBool::new(false);

/// Enables or disables revealing raw identifiers in console output.
pub fn set_debug_ids(enabled: bool) {
    DEBUG_IDS.store(enabled, Ordering::Relaxed);
}

/// Whether raw identifiers are currently revealed.
pub fn debug_ids() -> bool {
    DEBUG_IDS.load(Ordering::Relaxed) || std::env::var("LEDGER_DEBUG_IDS").is_ok_and(|v| v == "1")
}

/// A stable, non-reversible handle for an identifier: `#a1b2c3d4`.
///
/// Stable so an operator can search the workspace for the same value, and short
/// enough to paste. `ledger find --hash <handle>` resolves it.
pub fn id(value: &str) -> String {
    if debug_ids() {
        return value.to_owned();
    }
    let digest = sha256_hex(value.trim().to_lowercase().as_bytes());
    format!("#{}", digest.get(0..8).unwrap_or("00000000"))
}

/// Redacts anything that looks like an email address in a free-form string.
pub fn text(value: &str) -> String {
    if debug_ids() {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len());
    for token in value.split_inclusive(char::is_whitespace) {
        let core = token.trim_end();
        let trailing = &token[core.len()..];
        if core.contains('@') {
            out.push_str(&id(core));
        } else {
            out.push_str(core);
        }
        out.push_str(trailing);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_become_stable_handles() {
        set_debug_ids(false);
        let first = id("APP-1001");
        assert_eq!(first, id("APP-1001"));
        assert_eq!(first, id(" app-1001 "));
        assert!(first.starts_with('#'));
        assert_eq!(first.len(), 9);
        assert_ne!(first, id("APP-1002"));
    }

    #[test]
    fn emails_inside_prose_are_redacted() {
        set_debug_ids(false);
        let redacted = text("email ada@example.com does not match");
        assert!(!redacted.contains('@'), "{redacted}");
        assert!(redacted.contains("does not match"));
    }

    #[test]
    fn debug_mode_reveals_the_raw_value() {
        set_debug_ids(true);
        assert_eq!(id("APP-1001"), "APP-1001");
        set_debug_ids(false);
    }
}
