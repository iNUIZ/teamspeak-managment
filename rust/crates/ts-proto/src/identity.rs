//! TeamSpeak identity: a P-256 key, its libtomcrypt export and the security-level offset.
//! Ported from `tslib/identity.ts`. Identities saved by the old program load unchanged.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use num_bigint::BigUint;
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};

use crate::crypto::sha1;
use crate::{err, Result};

#[derive(Clone)]
pub struct Identity {
    secret: SecretKey,
    scalar: [u8; 32],
    pub key_offset: u64,
    /// Base64 of the libtomcrypt public key; the protocol's `omega`.
    pub public_key_string: String,
    /// base64(sha1(public_key_string))
    pub uid: String,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity").field("uid", &self.uid).field("key_offset", &self.key_offset).finish_non_exhaustive()
    }
}

impl Identity {
    fn from_scalar(scalar: [u8; 32], key_offset: u64) -> Result<Self> {
        let secret = SecretKey::from_slice(&scalar).or_else(|_| err("Invalid identity key"))?;
        let public_key_string = export_public_key(&secret.public_key());
        let uid = B64.encode(sha1(public_key_string.as_bytes()));
        Ok(Self { secret, scalar, key_offset, public_key_string, uid })
    }

    /// Load the values the old program stored: decimal private scalar and offset.
    pub fn from_stored(private_scalar: &str, key_offset: &str) -> Result<Self> {
        let value: BigUint = private_scalar.parse().or_else(|_| err("Invalid stored identity key"))?;
        let bytes = value.to_bytes_be();
        if bytes.len() > 32 {
            return err("Stored identity key is too long");
        }
        let mut scalar = [0u8; 32];
        scalar[32 - bytes.len()..].copy_from_slice(&bytes);
        let offset = key_offset.parse().or_else(|_| err("Invalid stored identity offset"))?;
        Self::from_scalar(scalar, offset)
    }

    /// A new random identity improved to `level`. Long-running: call it off the async runtime.
    pub fn generate(level: u32) -> Result<Self> {
        let mut scalar = [0u8; 32];
        getrandom::fill(&mut scalar).or_else(|e| err(format!("No random data: {e}")))?;
        let mut identity = Self::from_scalar(scalar, 0)?;
        identity.key_offset = find_offset(identity.public_key_string.as_bytes(), level);
        Ok(identity)
    }

    /// The private scalar in the old program's decimal storage format.
    pub fn private_scalar(&self) -> String {
        BigUint::from_bytes_be(&self.scalar).to_string()
    }

    pub fn security_level(&self) -> u32 {
        security_level(self.public_key_string.as_bytes(), self.key_offset)
    }

    /// DER-encoded ECDSA signature (P-256, SHA-256).
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        let signature: Signature = SigningKey::from(&self.secret).sign(data);
        signature.to_der().as_bytes().to_vec()
    }

    /// Old protocol (`initivexpand`): sha1 of the ECDH x coordinate.
    pub fn shared_secret(&self, server_public_key: &[u8]) -> Result<[u8; 20]> {
        let fields = parse_der_sequence(server_public_key)?;
        let (Some(DerValue::Int(x)), Some(DerValue::Int(y))) = (fields.get(2), fields.get(3)) else {
            return err("Invalid server public key");
        };
        let mut point = vec![0x04];
        point.extend(to_32_bytes(x));
        point.extend(to_32_bytes(y));
        let public = PublicKey::from_sec1_bytes(&point).or_else(|_| err("Invalid server public key point"))?;
        let shared = p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), public.as_affine());
        Ok(sha1(shared.raw_secret_bytes().as_slice()))
    }
}

fn to_32_bytes(value: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let trimmed = &value[value.len().saturating_sub(32)..];
    out[32 - trimmed.len()..].copy_from_slice(trimmed);
    out
}

/// Leading zero bits of sha1(public key + decimal offset), each byte counted from its low bit.
fn security_level(public_key: &[u8], offset: u64) -> u32 {
    let mut input = public_key.to_vec();
    input.extend(offset.to_string().as_bytes());
    let mut count = 0;
    for byte in sha1(&input) {
        if byte != 0 {
            return count + byte.trailing_zeros();
        }
        count += 8;
    }
    count
}

fn find_offset(public_key: &[u8], level: u32) -> u64 {
    (0u64..).find(|&offset| security_level(public_key, offset) >= level).expect("an offset exists")
}

/// libtomcrypt: SEQUENCE { BIT STRING 0x00, INTEGER 32, INTEGER x, INTEGER y }.
fn export_public_key(public: &PublicKey) -> String {
    let point = public.to_sec1_point(false);
    let bytes = point.as_bytes();
    let mut content = vec![0x03, 0x02, 0x07, 0x00];
    content.extend(der_integer(&[32]));
    content.extend(der_integer(&bytes[1..33]));
    content.extend(der_integer(&bytes[33..65]));
    let mut der = vec![0x30];
    der.extend(der_length(content.len()));
    der.extend(content);
    B64.encode(der)
}

fn der_integer(value: &[u8]) -> Vec<u8> {
    let start = value.iter().position(|&b| b != 0).unwrap_or(value.len().saturating_sub(1));
    let value = &value[start..];
    let mut out = vec![0x02];
    let pad = value.first().is_some_and(|b| b & 0x80 != 0);
    out.extend(der_length(value.len() + usize::from(pad)));
    if pad {
        out.push(0);
    }
    out.extend(value);
    out
}

fn der_length(len: usize) -> Vec<u8> {
    match len {
        0..=0x7f => vec![len as u8],
        0x80..=0xff => vec![0x81, len as u8],
        _ => vec![0x82, (len >> 8) as u8, len as u8],
    }
}

enum DerValue {
    Bits,
    Int(Vec<u8>),
}

fn parse_der_sequence(data: &[u8]) -> Result<Vec<DerValue>> {
    fn read_len(data: &[u8], pos: &mut usize) -> Result<usize> {
        let first = *data.get(*pos).ok_or(crate::Error("DER data too short".into()))?;
        *pos += 1;
        if first < 0x80 {
            return Ok(first.into());
        }
        let mut len = 0usize;
        for _ in 0..(first & 0x7f) {
            let byte = *data.get(*pos).ok_or(crate::Error("DER data too short".into()))?;
            len = (len << 8) | usize::from(byte);
            *pos += 1;
        }
        Ok(len)
    }

    if data.first() != Some(&0x30) {
        return err("Expected SEQUENCE tag");
    }
    let mut pos = 1;
    let len = read_len(data, &mut pos)?;
    let end = (pos + len).min(data.len());
    let mut out = Vec::new();
    while pos < end {
        let tag = data[pos];
        pos += 1;
        let len = read_len(data, &mut pos)?;
        let Some(value) = data.get(pos..pos + len) else {
            return err("DER data too short");
        };
        match tag {
            0x03 => out.push(DerValue::Bits),
            0x02 => out.push(DerValue::Int(value.to_vec())),
            _ => {}
        }
        pos += len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_identity_reaches_its_level_and_reloads() {
        let identity = Identity::generate(8).unwrap();
        assert!(identity.security_level() >= 8);
        let again = Identity::from_stored(&identity.private_scalar(), &identity.key_offset.to_string()).unwrap();
        assert_eq!(again.uid, identity.uid);
    }

    #[test]
    fn signatures_verify() {
        use p256::ecdsa::{signature::Verifier, VerifyingKey};
        let identity = Identity::generate(0).unwrap();
        let der = identity.sign(b"message");
        let signature = Signature::from_der(&der).unwrap();
        let verifying = VerifyingKey::from(&SigningKey::from(&identity.secret));
        assert!(verifying.verify(b"message", &signature).is_ok());
    }

    #[test]
    fn a_short_server_key_is_an_error_not_a_panic() {
        let identity = Identity::generate(0).unwrap();
        assert!(identity.shared_secret(&[0x30, 0x7f, 0x02]).is_err());
        assert!(identity.shared_secret(&[]).is_err());
    }
}
