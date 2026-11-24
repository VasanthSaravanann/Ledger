//! Validated identifiers and addresses.
//!
//! The pilot keys every record on an ATS application id, and every email needs a
//! real reply-to mailbox. Both are newtypes so a record cannot be built from an
//! unvalidated string.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Longest identifier or address the pilot accepts.
const MAX_LEN: usize = 128;

/// Defines a validated, serde-friendly string newtype.
macro_rules! label_id {
    ($(#[$meta:meta])* $name:ident, $label:literal) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Returns the identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            fn check(value: &str) -> Result<()> {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err(Error::invalid($label, "must not be empty"));
                }
                if trimmed.chars().count() > MAX_LEN {
                    return Err(Error::invalid($label, "must be at most 128 characters"));
                }
                if trimmed.chars().any(char::is_control) {
                    return Err(Error::invalid(
                        $label,
                        "must not contain control characters",
                    ));
                }
                Ok(())
            }
        }

        impl TryFrom<String> for $name {
            type Error = Error;

            fn try_from(value: String) -> Result<Self> {
                Self::check(&value)?;
                Ok(Self(value.trim().to_owned()))
            }
        }

        impl TryFrom<&str> for $name {
            type Error = Error;

            fn try_from(value: &str) -> Result<Self> {
                Self::try_from(value.to_owned())
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(value: &str) -> Result<Self> {
                Self::try_from(value)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}({:?})", stringify!($name), self.0)
            }
        }
    };
}

label_id!(
    /// Identifies one open requisition, for example `"REQ-2026-014"`.
    RequisitionId,
    "requisition id"
);
label_id!(
    /// Identifies the recruiter who captured a disposition.
    RecruiterId,
    "recruiter id"
);
label_id!(
    /// The ATS application id that joins the export to the disposition form.
    ApplicationId,
    "application id"
);

label_id!(
    /// Identifies the named person who made the adverse-action call.
    DecisionMakerId,
    "decision maker id"
);
label_id!(
    /// Identifies the coordinator who performed the second-person QA check.
    QaId,
    "qa id"
);
label_id!(
    /// The stable id of a captured disposition record, for `SUPERSEDES` links.
    DispositionId,
    "disposition id"
);
label_id!(
    /// The stable id of a send-log entry.
    SendId,
    "send id"
);

/// An immutable `TEXT_VERSION` tag such as `v1.0` or `v1.11`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TextVersion(String);

impl TextVersion {
    /// Validates `v<major>.<minor>`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for any other shape, so a version can never be
    /// blank or a free-text note.
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        let Some(rest) = trimmed.strip_prefix('v') else {
            return Err(Error::invalid("text version", "must look like v1.0"));
        };
        let (major, minor) = rest
            .split_once('.')
            .ok_or_else(|| Error::invalid("text version", "must look like v1.0"))?;
        let numeric = |part: &str| {
            !part.is_empty()
                && part.len() <= 4
                && part.chars().all(|character| character.is_ascii_digit())
        };
        if !numeric(major) || !numeric(minor) {
            return Err(Error::invalid("text version", "must look like v1.0"));
        }
        Ok(Self(format!("v{major}.{minor}")))
    }

    /// The tag as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TextVersion {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl FromStr for TextVersion {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl From<TextVersion> for String {
    fn from(version: TextVersion) -> Self {
        version.0
    }
}

impl fmt::Display for TextVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Debug for TextVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "TextVersion({:?})", self.0)
    }
}

/// A candidate email address, used as the merge recipient.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CandidateEmail(String);

impl CandidateEmail {
    /// Returns the address as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CandidateEmail {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        let trimmed = value.trim();
        check_single_address(trimmed, "candidate email")?;
        Ok(Self(trimmed.to_lowercase()))
    }
}

impl TryFrom<&str> for CandidateEmail {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::try_from(value.to_owned())
    }
}

impl FromStr for CandidateEmail {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::try_from(value)
    }
}

impl From<CandidateEmail> for String {
    fn from(email: CandidateEmail) -> Self {
        email.0
    }
}

impl fmt::Display for CandidateEmail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Debug for CandidateEmail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "CandidateEmail({:?})", self.0)
    }
}

/// The reply-to mailbox that candidates reach a named human on.
///
/// Task 1.3 of the plan forbids an unmonitored or automated reply address, so the
/// local part may not be a `no-reply` style mailbox.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Mailbox(String);

impl Mailbox {
    /// Returns the address as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Mailbox {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        let trimmed = value.trim();
        check_single_address(trimmed, "reply-to mailbox")?;
        let local = trimmed.split('@').next().unwrap_or_default().to_lowercase();
        let unmonitored = [
            "no-reply",
            "noreply",
            "no_reply",
            "donotreply",
            "do-not-reply",
        ];
        if unmonitored.contains(&local.as_str()) {
            return Err(Error::invalid(
                "reply-to mailbox",
                "must be a human-monitored mailbox, not an automated no-reply address",
            ));
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl TryFrom<&str> for Mailbox {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::try_from(value.to_owned())
    }
}

impl FromStr for Mailbox {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::try_from(value)
    }
}

impl From<Mailbox> for String {
    fn from(mailbox: Mailbox) -> Self {
        mailbox.0
    }
}

impl fmt::Display for Mailbox {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Debug for Mailbox {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Mailbox({:?})", self.0)
    }
}

/// One of the two disposition-email shapes, plus the administrative closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateKind {
    /// Template A: the `GATE` disposition.
    Gate,
    /// Template B: the `FIELD` disposition.
    Field,
    /// Template C: the `ADMIN` disposition.
    Admin,
}

impl TemplateKind {
    /// Every template the pilot needs.
    pub const ALL: [Self; 3] = [Self::Gate, Self::Field, Self::Admin];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::Gate => "gate",
            Self::Field => "field",
            Self::Admin => "admin",
        }
    }

    /// The plan's `Template A` / `B` / `C` letter.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Gate => "A",
            Self::Field => "B",
            Self::Admin => "C",
        }
    }
}

impl fmt::Display for TemplateKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for TemplateKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "gate" | "a" | "failed-gate" => Ok(Self::Gate),
            "field" | "b" | "comparative" => Ok(Self::Field),
            "admin" | "c" | "administrative" => Ok(Self::Admin),
            other => Err(Error::invalid(
                "template kind",
                format!("expected gate, field or admin, got {other:?}"),
            )),
        }
    }
}

/// Checks that `value` looks like exactly one address, not a list or a display name.
fn check_single_address(value: &str, kind: &'static str) -> Result<()> {
    if value.chars().count() > MAX_LEN {
        return Err(Error::invalid(kind, "must be at most 128 characters"));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(Error::invalid(kind, "must not contain whitespace"));
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err(Error::invalid(
            kind,
            "must be a single addr-spec like name@example.com",
        ));
    }
    if !domain.contains('.') {
        return Err(Error::invalid(kind, "domain must contain a dot"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_application_ids() {
        let id = ApplicationId::try_from("  APP-1001\n").expect("valid application id");
        assert_eq!(id.as_str(), "APP-1001");
    }

    #[test]
    fn rejects_blank_ids() {
        assert!(ApplicationId::try_from("   ").is_err());
    }

    #[test]
    fn rejects_control_characters() {
        assert!(RequisitionId::try_from("REQ-\u{7}1").is_err());
    }

    #[test]
    fn lowercases_candidate_email() {
        let email = CandidateEmail::try_from("Ada.Lovelace@Example.COM").expect("valid email");
        assert_eq!(email.as_str(), "ada.lovelace@example.com");
    }

    #[test]
    fn rejects_email_lists_and_display_names() {
        assert!(CandidateEmail::try_from("a@example.com, b@example.com").is_err());
        assert!(CandidateEmail::try_from("Ada <a@example.com>").is_err());
        assert!(CandidateEmail::try_from("a@localhost").is_err());
    }

    #[test]
    fn rejects_unmonitored_reply_mailbox() {
        assert!(Mailbox::try_from("no-reply@example.com.au").is_err());
        assert!(Mailbox::try_from("careers-feedback@example.com.au").is_ok());
    }

    #[test]
    fn text_versions_are_strictly_shaped() {
        assert_eq!(
            TextVersion::parse(" v1.0 ").expect("valid").as_str(),
            "v1.0"
        );
        assert_eq!(
            TextVersion::parse("v1.11").expect("valid").as_str(),
            "v1.11"
        );
        for bad in ["1.0", "v1", "v1.", "v.1", "v1.a", "v1.0-beta", ""] {
            assert!(TextVersion::parse(bad).is_err(), "{bad:?} must be rejected");
        }
        assert!(
            TextVersion::parse("v1.1").expect("valid") > TextVersion::parse("v1.0").expect("valid")
        );
    }

    #[test]
    fn template_kind_round_trips_through_its_code() {
        for kind in TemplateKind::ALL {
            assert_eq!(
                TemplateKind::from_str(kind.code()).expect("parses its own code"),
                kind
            );
            assert_eq!(
                TemplateKind::from_str(kind.letter()).expect("parses its letter"),
                kind
            );
        }
        assert_eq!(TemplateKind::Gate.letter(), "A");
        assert_eq!(TemplateKind::Admin.letter(), "C");
    }
}
