//! Small validation helpers for stored prose.

use crate::error::{Error, Result};

/// Trims `value` and rejects it when blank or longer than `max` characters.
///
/// # Errors
///
/// Returns [`Error::Invalid`] naming `kind` when the text is out of bounds.
pub fn required(kind: &'static str, value: impl Into<String>, max: usize) -> Result<String> {
    let trimmed = value.into().trim().to_owned();
    if trimmed.is_empty() {
        return Err(Error::invalid(kind, "must not be empty"));
    }
    if trimmed.chars().count() > max {
        return Err(Error::invalid(
            kind,
            format!("must be at most {max} characters"),
        ));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_text() {
        assert_eq!(
            required("title", "  Engineer \n", 40).expect("valid"),
            "Engineer"
        );
    }

    #[test]
    fn rejects_blank_and_overlong_text() {
        assert!(required("title", "   ", 40).is_err());
        assert!(required("title", "x".repeat(41), 40).is_err());
    }
}
