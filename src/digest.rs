//! Canonical serialisation and SHA-256, shared by the template renderer and the
//! send log.
//!
//! The canonical form is normative: see `FORMAT_SPEC.md`. It is deliberately
//! boring — sorted keys, no whitespace, integers only, NFC-normalised strings —
//! because an independent verifier (`verify_log.py`) has to reproduce it with the
//! Python standard library alone.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{DateTime, Timelike, Utc};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// The digest algorithm version stamped into every hashed payload.
pub const DIGEST_VERSION: i64 = 2;

/// Returns the lowercase hex SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// One value in the canonical encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalValue {
    /// JSON `null`.
    Null,
    /// JSON `true` / `false`.
    Bool(bool),
    /// A base-10 integer. Floating point is not representable on purpose.
    Int(i64),
    /// A JSON string, stored NFC-normalised.
    Str(String),
    /// A canonical object: keys are emitted in byte-sorted order.
    Object(BTreeMap<String, CanonicalValue>),
}

impl CanonicalValue {
    /// Builds an object from a field list; keys are sorted when encoded.
    pub fn object(fields: impl IntoIterator<Item = (&'static str, Self)>) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    /// A string value, NFC-normalised.
    pub fn text(value: impl AsRef<str>) -> Self {
        Self::Str(nfc(value.as_ref()))
    }

    /// A string value, or JSON `null` when absent.
    pub fn maybe_text(value: Option<&str>) -> Self {
        value.map_or(Self::Null, Self::text)
    }

    /// Writes the canonical encoding into `out`.
    fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Self::Str(value) => write_string(value, out),
            Self::Object(fields) => {
                out.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write_json(out);
                }
                out.push('}');
            }
        }
    }
}

/// Encodes `value` as canonical bytes.
pub fn canonical_bytes(value: &CanonicalValue) -> Vec<u8> {
    let mut out = String::new();
    value.write_json(&mut out);
    out.into_bytes()
}

/// Writes a minimally-escaped JSON string.
fn write_string(value: &str, out: &mut String) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            character if (character as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", character as u32);
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

/// Returns the NFC-normalised form of `value`.
pub fn nfc(value: &str) -> String {
    value.nfc().collect()
}

/// The stored instant, truncated to whole seconds as `FORMAT_SPEC.md` requires.
pub fn truncate_to_seconds(at: DateTime<Utc>) -> DateTime<Utc> {
    at.with_nanosecond(0).unwrap_or(at)
}

/// The digest representation of an instant: epoch seconds, UTC.
pub fn epoch_seconds(at: DateTime<Utc>) -> i64 {
    truncate_to_seconds(at).timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_known_digest_of_an_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(sha256_hex(b"ledger"), sha256_hex(b"ledger"));
        assert_ne!(sha256_hex(b"ledger"), sha256_hex(b"Ledger"));
    }

    #[test]
    fn keys_are_sorted_and_whitespace_is_absent() {
        let value = CanonicalValue::object([
            ("zulu", CanonicalValue::Int(1)),
            ("alpha", CanonicalValue::text("x")),
            ("mike", CanonicalValue::Bool(true)),
        ]);
        assert_eq!(
            String::from_utf8(canonical_bytes(&value)).expect("utf8"),
            r#"{"alpha":"x","mike":true,"zulu":1}"#
        );
    }

    #[test]
    fn nullable_and_control_characters_are_escaped_minimally() {
        let value = CanonicalValue::object([
            ("a", CanonicalValue::Null),
            ("b", CanonicalValue::text("line\nquote\"slash\\")),
        ]);
        assert_eq!(
            String::from_utf8(canonical_bytes(&value)).expect("utf8"),
            r#"{"a":null,"b":"line\u000aquote\"slash\\"}"#
        );
    }

    #[test]
    fn nfc_normalisation_collapses_equivalent_forms() {
        let decomposed = "e\u{301}";
        let composed = "\u{e9}";
        assert_ne!(decomposed, composed);
        assert_eq!(nfc(decomposed), nfc(composed));
        let value = CanonicalValue::text(decomposed);
        assert_eq!(
            String::from_utf8(canonical_bytes(&value)).expect("utf8"),
            format!("\"{composed}\"")
        );
    }

    #[test]
    fn timestamps_hash_at_second_precision() {
        let at = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 24, 9, 0, 0)
            .single()
            .expect("timestamp");
        let with_nanos = at + chrono::Duration::milliseconds(999);
        assert_eq!(epoch_seconds(at), epoch_seconds(with_nanos));
        assert_eq!(epoch_seconds(at), 1795510800);
    }
}
