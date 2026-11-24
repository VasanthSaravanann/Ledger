//! SHA-256 helpers shared by the template renderer and the send log.

use sha2::{Digest, Sha256};

/// Returns the lowercase hex SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
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
}
