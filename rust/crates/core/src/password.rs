//! bcrypt password hashes; hashes made by the old program keep working.

use crate::{Error, Result};

const COST: u32 = 12;

pub fn hash(password: &str) -> Result<String> {
    bcrypt::hash(password, COST).map_err(|e| Error(format!("Password hash failed: {e}")))
}

/// False for a wrong password and for a malformed hash.
pub fn verify(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_hash_never_matches() {
        assert!(!verify("x", "not-a-hash"));
        assert!(!verify("x", ""));
    }
}
