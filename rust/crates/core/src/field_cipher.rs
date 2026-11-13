//! Encryption of secrets at rest: AES-256-GCM, value format `enc:iv:tag:ciphertext` in hex.
//! Same key derivation and format as the old program, so existing rows stay readable.

use aes_gcm::aead::{AeadInOut, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce, Tag};

use crate::{Error, Result};

const SALT: &[u8] = b"ts6-webui-enc-v1";
const IV_LEN: usize = 12;
const TAG_LEN: usize = 16;

#[derive(Clone)]
pub struct FieldCipher {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for FieldCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FieldCipher")
    }
}

/// Node's `scryptSync` defaults: N=16384, r=8, p=1. Deliberately slow; derive once at startup.
fn derive_key(source: &str) -> Result<[u8; 32]> {
    let params = scrypt::Params::new(14, 8, 1).map_err(|e| Error(format!("Invalid scrypt settings: {e}")))?;
    let mut key = [0u8; 32];
    scrypt::scrypt(source.as_bytes(), SALT, &params, &mut key).map_err(|e| Error(format!("Key derivation failed: {e}")))?;
    Ok(key)
}

impl FieldCipher {
    pub fn new(source: &str) -> Result<Self> {
        let key = derive_key(source)?;
        Ok(Self { cipher: Aes256Gcm::new(&key.into()) })
    }

    pub fn encrypt(&self, plain: &str) -> Result<String> {
        let mut iv = [0u8; IV_LEN];
        getrandom::fill(&mut iv).map_err(|e| Error(format!("No random data: {e}")))?;
        let mut data = plain.as_bytes().to_vec();
        let tag = self
            .cipher
            .encrypt_inout_detached(&Nonce::from(iv), b"", data.as_mut_slice().into())
            .map_err(|_| Error("Encryption failed".into()))?;
        Ok(format!("enc:{}:{}:{}", hex::encode(iv), hex::encode(tag), hex::encode(data)))
    }

    /// A value without the `enc:` prefix is returned as it is: older rows may predate encryption.
    pub fn decrypt(&self, value: &str) -> Result<String> {
        if !value.starts_with("enc:") {
            return Ok(value.to_owned());
        }
        let invalid = || Error("Invalid encrypted format".into());
        let mut parts = value.split(':').skip(1);
        let (Some(iv), Some(tag), Some(data), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return Err(invalid());
        };
        let iv: [u8; IV_LEN] = hex::decode(iv).ok().and_then(|v| v.try_into().ok()).ok_or_else(invalid)?;
        let tag: [u8; TAG_LEN] = hex::decode(tag).ok().and_then(|v| v.try_into().ok()).ok_or_else(invalid)?;
        let mut data = hex::decode(data).map_err(|_| invalid())?;
        self.cipher
            .decrypt_inout_detached(&Nonce::from(iv), b"", data.as_mut_slice().into(), &Tag::from(tag))
            .map_err(|_| Error("Could not decrypt value: wrong ENCRYPTION_KEY or changed data".into()))?;
        String::from_utf8(data).map_err(|_| Error("Decrypted value is not text".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_round_trips_with_a_fresh_iv_each_time() {
        let cipher = FieldCipher::new("key").unwrap();
        let (a, b) = (cipher.encrypt("gizli").unwrap(), cipher.encrypt("gizli").unwrap());
        assert_ne!(a, b);
        assert_eq!(cipher.decrypt(&a).unwrap(), "gizli");
        assert_eq!(cipher.decrypt(&b).unwrap(), "gizli");
    }

    #[test]
    fn plain_values_pass_through() {
        assert_eq!(FieldCipher::new("key").unwrap().decrypt("not-encrypted").unwrap(), "not-encrypted");
    }

    #[test]
    fn changed_or_broken_values_are_errors() {
        let cipher = FieldCipher::new("key").unwrap();
        let good = cipher.encrypt("gizli").unwrap();
        assert!(FieldCipher::new("other").unwrap().decrypt(&good).is_err());
        let mut changed = good.clone();
        changed.pop();
        changed.push(if good.ends_with('0') { '1' } else { '0' });
        assert!(cipher.decrypt(&changed).is_err());
        for broken in ["enc:", "enc:a:b", "enc:zz:zz:zz", "enc:00:00:00:00"] {
            assert!(cipher.decrypt(broken).is_err(), "{broken}");
        }
    }
}
