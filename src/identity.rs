//! Verifiable operator identities and role-based access (V5 Phase 3).
//!
//! A `$USER` string is spoofable and proves nothing. Every operator is instead an
//! Ed25519 keypair: the public half lives in `operators.json`, the secret half in
//! `operators/<name>.key` at `0600`. Dispositions and sign-offs are signed with it,
//! so a record names the key that authorised it and a later reader can check it
//! without trusting the file's contents.
//!
//! Signatures live **outside** the send-log digest (see `FORMAT_SPEC.md`): the chain
//! proves the log is unedited, signatures prove who decided what. `verify_log.py`
//! therefore reports "signatures not checked" and `ledger verify-signatures` does
//! that job in Rust.

use std::collections::BTreeMap;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::error::{Error, Result};
use crate::store::{self, Store};
use crate::text;

/// What an operator may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// Recruiter or coordinator: imports, dispositions, QA, dispatch.
    Coordinator,
    /// Hiring Manager or Legal: signs off copy, policy, retention.
    Approver,
    /// Read-only: verify, metrics, reports.
    Auditor,
}

impl Role {
    /// Every role.
    pub const ALL: [Self; 3] = [Self::Coordinator, Self::Approver, Self::Auditor];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::Coordinator => "coordinator",
            Self::Approver => "approver",
            Self::Auditor => "auditor",
        }
    }

    /// Whether this role may run `command` when it mutates the workspace.
    ///
    /// Read-only commands are allowed for every role and are filtered before this
    /// is consulted.
    pub fn permits(self, command: &str) -> bool {
        match self {
            Self::Auditor => false,
            Self::Coordinator => matches!(
                command,
                "ats import"
                    | "disposition"
                    | "qa"
                    | "dispatch"
                    | "delivery"
                    | "review"
                    | "reply"
                    | "signal"
                    | "anchor"
                    | "log"
                    | "init"
            ),
            Self::Approver => matches!(
                command,
                "gate"
                    | "templates"
                    | "playbook"
                    | "requisition"
                    | "governance"
                    | "purge"
                    | "migrate"
                    | "checklist"
                    | "init"
                    | "demo seed"
                    | "operator"
            ),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::str::FromStr for Role {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|role| role.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "role",
                    format!("expected coordinator, approver or auditor; got {value:?}"),
                )
            })
    }
}

/// A registered operator: name, role, and public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operator {
    /// The operator's name, as used on the command line.
    pub name: String,
    /// What they may do.
    pub role: Role,
    /// The Ed25519 public key, lowercase hex.
    pub public_key: String,
    /// When the identity was registered.
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

/// The operator registry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Registry {
    operators: Vec<Operator>,
}

impl Registry {
    /// Loads the registry, or an empty one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn load(store: &Store) -> Result<Self> {
        Ok(store.load(store::OPERATORS)?.unwrap_or_default())
    }

    /// Saves the registry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be written.
    pub fn save(&self, store: &Store) -> Result<()> {
        store.save(store::OPERATORS, self)
    }

    /// Every operator.
    pub fn operators(&self) -> &[Operator] {
        &self.operators
    }

    /// Whether any operator is registered.
    pub fn is_empty(&self) -> bool {
        self.operators.is_empty()
    }

    /// Returns the operator with `name`.
    pub fn get(&self, name: &str) -> Option<&Operator> {
        self.operators
            .iter()
            .find(|operator| operator.name.eq_ignore_ascii_case(name))
    }

    /// Returns the operator with `name`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when no such operator is registered.
    pub fn require(&self, name: &str) -> Result<&Operator> {
        self.get(name).ok_or_else(|| {
            Error::invalid(
                "operator",
                format!("{name:?} is not registered; run `ledger operator register`"),
            )
        })
    }

    /// Registers an operator with a fresh key.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] on a blank or duplicate name, and propagates IO
    /// errors from the keyfile.
    pub fn register(&mut self, store: &Store, name: &str, role: Role) -> Result<Operator> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|error| {
            Error::invalid("operator key", format!("no randomness available: {error}"))
        })?;
        self.register_with(store, name, role, &secret)
    }

    /// Registers an operator from a known secret. Used by the deterministic fixture.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] on a blank or duplicate name.
    pub fn register_with(
        &mut self,
        store: &Store,
        name: &str,
        role: Role,
        secret: &[u8; 32],
    ) -> Result<Operator> {
        self.register_with_at(store, name, role, secret, chrono::Utc::now())
    }

    /// Registers an operator at a fixed instant. Used by the deterministic fixture.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] on a blank or duplicate name.
    pub fn register_with_at(
        &mut self,
        store: &Store,
        name: &str,
        role: Role,
        secret: &[u8; 32],
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Operator> {
        let name = text::required("operator name", name, 120)?;
        if self.get(&name).is_some() {
            return Err(Error::invalid(
                "operator",
                format!("{name:?} is already registered"),
            ));
        }

        let signing_key = SigningKey::from_bytes(secret);
        let public_key = to_hex(signing_key.verifying_key().as_bytes());

        let directory = store.path("operators");
        store::ensure_dir(&directory)?;
        let key_path = directory.join(format!("{}.key", sanitize(&name)));
        store::write_secret(&key_path, to_hex(secret).as_bytes())?;

        let operator = Operator {
            name,
            role,
            public_key,
            registered_at: at,
        };
        self.operators.push(operator.clone());
        self.save(store)?;
        Ok(operator)
    }

    /// Loads the signing key for `name`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the operator is unknown or the keyfile is
    /// missing, and [`Error::Io`] when it cannot be read.
    pub fn signing_key(&self, store: &Store, name: &str) -> Result<SigningKey> {
        let operator = self.require(name)?;
        let path = store
            .path("operators")
            .join(format!("{}.key", sanitize(&operator.name)));
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let bytes = from_hex(text.trim())
            .ok_or_else(|| Error::invalid("operator key", format!("{path:?} is not hex")))?;
        let secret: [u8; 32] = bytes
            .try_into()
            .map_err(|_| Error::invalid("operator key", "key must be 32 bytes"))?;
        let signing_key = SigningKey::from_bytes(&secret);
        if to_hex(signing_key.verifying_key().as_bytes()) != operator.public_key {
            return Err(Error::invalid(
                "operator key",
                format!(
                    "keyfile for {:?} does not match the registry",
                    operator.name
                ),
            ));
        }
        Ok(signing_key)
    }
}

/// A detached signature over one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    /// The operator who signed.
    pub operator: String,
    /// Their public key, so the record verifies without the registry.
    pub public_key: String,
    /// The SHA-256 of the canonical record this signature covers.
    pub digest: String,
    /// The Ed25519 signature over `digest`, lowercase hex.
    pub signature: String,
}

impl Signed {
    /// Signs `digest` as `operator`.
    ///
    /// # Errors
    ///
    /// Propagates [`Registry::signing_key`] errors.
    pub fn create(
        registry: &Registry,
        store: &Store,
        operator: &str,
        digest: &str,
    ) -> Result<Self> {
        let key = registry.signing_key(store, operator)?;
        let entry = registry.require(operator)?;
        Ok(Self {
            operator: entry.name.clone(),
            public_key: entry.public_key.clone(),
            digest: digest.to_owned(),
            signature: to_hex(&key.sign(digest.as_bytes()).to_bytes()),
        })
    }

    /// Checks the signature against `digest`.
    pub fn verify(&self, digest: &str) -> bool {
        if self.digest != digest {
            return false;
        }
        let Some(key_bytes) = from_hex(&self.public_key) else {
            return false;
        };
        let Ok(key_bytes) = <[u8; 32]>::try_from(key_bytes) else {
            return false;
        };
        let Ok(key) = VerifyingKey::from_bytes(&key_bytes) else {
            return false;
        };
        let Some(signature_bytes) = from_hex(&self.signature) else {
            return false;
        };
        let Ok(signature_bytes) = <[u8; 64]>::try_from(signature_bytes) else {
            return false;
        };
        key.verify(digest.as_bytes(), &Signature::from_bytes(&signature_bytes))
            .is_ok()
    }
}

/// Resolves the active operator: `--operator`, then `LEDGER_OPERATOR`, then the
/// single registered operator.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when nothing is registered, when several operators
/// are registered and none was named, or when the named operator is unknown.
pub fn active(registry: &Registry, explicit: Option<&str>) -> Result<Operator> {
    if let Some(name) = explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("LEDGER_OPERATOR").ok())
    {
        return registry.require(&name).cloned();
    }
    match registry.operators() {
        [only] => Ok(only.clone()),
        [] => Err(Error::invalid(
            "operator",
            "no operator is registered; run `ledger operator register --name <name> --role <role>`",
        )),
        many => Err(Error::invalid(
            "operator",
            format!(
                "{} operators are registered ({}); pass --operator <name>",
                many.len(),
                many.iter()
                    .map(|operator| operator.name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ")
            ),
        )),
    }
}

/// Signs one record's canonical digest.
///
/// # Errors
///
/// Propagates signature-creation errors.
pub fn sign(registry: &Registry, store: &Store, operator: &str, digest: &str) -> Result<Signed> {
    Signed::create(registry, store, operator, digest)
}

/// The canonical digest of an arbitrary field map, used for record signatures.
pub fn digest_fields(fields: impl IntoIterator<Item = (&'static str, String)>) -> String {
    let canonical = crate::digest::CanonicalValue::object(
        fields
            .into_iter()
            .map(|(key, value)| (key, crate::digest::CanonicalValue::text(value))),
    );
    sha256_hex(&crate::digest::canonical_bytes(&canonical))
}

/// Lowercase hex.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Parses lowercase or uppercase hex.
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let mut chars = text.chars();
    while let Some(high) = chars.next() {
        let low = chars.next()?;
        bytes.push((high.to_digit(16)? as u8) << 4 | (low.to_digit(16)? as u8));
    }
    Some(bytes)
}

/// A filesystem-safe form of an operator name.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

/// Convenience for reporting: operator name → role.
pub fn summary(registry: &Registry) -> BTreeMap<String, String> {
    registry
        .operators()
        .iter()
        .map(|operator| (operator.name.clone(), operator.role.to_string()))
        .collect()
}

/// Whether a keyfile exists for `operator`.
pub fn keyfile_path(store: &Store, operator: &str) -> std::path::PathBuf {
    store.path("operators").join(format!(
        "{}.key",
        Path::new(operator)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn workspace(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-identity-{tag}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::open(&dir).expect("workspace"), dir)
    }

    #[test]
    fn a_signature_verifies_only_for_its_own_digest() {
        let (store, dir) = workspace("sign");
        let mut registry = Registry::default();
        registry
            .register(&store, "qa-patel", Role::Coordinator)
            .expect("register");
        let digest = digest_fields([("application", "APP-1".to_owned())]);
        let signed = sign(&registry, &store, "qa-patel", &digest).expect("sign");
        assert!(signed.verify(&digest));
        assert!(!signed.verify(&digest_fields([("application", "APP-2".to_owned())])));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_signature_or_key_is_rejected() {
        let (store, dir) = workspace("tamper");
        let mut registry = Registry::default();
        registry
            .register(&store, "j-chen", Role::Approver)
            .expect("register");
        let digest = digest_fields([("copy", "v1".to_owned())]);
        let mut signed = sign(&registry, &store, "j-chen", &digest).expect("sign");

        let mut signature = from_hex(&signed.signature).expect("hex");
        signature[0] ^= 0xff;
        signed.signature = to_hex(&signature);
        assert!(!signed.verify(&digest));

        signed.signature = to_hex(&[0u8; 64]);
        assert!(!signed.verify(&digest));

        let mut bad_key = from_hex(&signed.public_key).expect("hex");
        bad_key[0] ^= 0xff;
        signed.public_key = to_hex(&bad_key);
        assert!(!signed.verify(&digest));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keyfiles_are_private_and_match_the_registry() {
        let (store, dir) = workspace("keyfile");
        let mut registry = Registry::default();
        let operator = registry
            .register(&store, "qa-patel", Role::Coordinator)
            .expect("register");
        let path = store.path("operators").join("qa-patel.key");
        assert!(path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(registry.signing_key(&store, "qa-patel").is_ok());
        assert_eq!(operator.public_key.len(), 64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roles_gate_mutating_commands() {
        assert!(Role::Coordinator.permits("disposition"));
        assert!(Role::Coordinator.permits("dispatch"));
        assert!(!Role::Coordinator.permits("gate"));
        assert!(Role::Approver.permits("gate"));
        assert!(Role::Approver.permits("purge"));
        assert!(!Role::Approver.permits("dispatch"));
        for role in Role::ALL {
            assert!(!role.permits("dispatch") || role == Role::Coordinator);
        }
    }

    #[test]
    fn duplicate_and_blank_names_are_refused() {
        let (store, dir) = workspace("names");
        let mut registry = Registry::default();
        registry
            .register(&store, "qa-patel", Role::Coordinator)
            .expect("register");
        assert!(
            registry
                .register(&store, "qa-patel", Role::Auditor)
                .is_err()
        );
        assert!(registry.register(&store, "   ", Role::Auditor).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn active_operator_resolution_is_explicit() {
        let (store, dir) = workspace("active");
        let mut registry = Registry::default();
        registry
            .register(&store, "qa-patel", Role::Coordinator)
            .expect("one");
        assert_eq!(active(&registry, None).expect("single").name, "qa-patel");
        registry
            .register(&store, "j-chen", Role::Approver)
            .expect("two");
        assert!(active(&registry, None).is_err(), "ambiguous");
        assert_eq!(
            active(&registry, Some("j-chen")).expect("named").role,
            Role::Approver
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
