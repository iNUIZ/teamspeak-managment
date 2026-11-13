//! Server license chain and the new protocol's shared secret. Ported from `tslib/license.ts`.

use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;

use crate::crypto::sha512;
use crate::{err, Result};

const LICENSE_ROOT_KEY: [u8; 32] = [
    0xcd, 0x0d, 0xe2, 0xae, 0xd4, 0x63, 0x45, 0x50, 0x9a, 0x7e, 0x3c, 0xfd, 0x8f, 0x68, 0xb3, 0xdc, 0x75, 0x55, 0xb2,
    0x9d, 0xcc, 0xec, 0x73, 0xcd, 0x18, 0x75, 0x0f, 0x99, 0x38, 0x12, 0x40, 0x8a,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseBlock {
    pub key: [u8; 32],
    pub hash: [u8; 32],
    pub kind: u8,
}

fn null_terminated_len(data: &[u8], start: usize) -> Result<usize> {
    data.get(start..)
        .and_then(|rest| rest.iter().position(|&b| b == 0))
        .ok_or(crate::Error("Non-null-terminated string in license".into()))
}

pub fn parse_license(data: &[u8]) -> Result<Vec<LicenseBlock>> {
    match data.first() {
        None => return err("License too short"),
        Some(1) => {}
        Some(_) => return err("Unsupported license version"),
    }
    let mut blocks = Vec::new();
    let mut pos = 1;
    while pos < data.len() {
        if data.len() - pos < 42 {
            return err("License block too short");
        }
        if data[pos] != 0 {
            return err(format!("Wrong key kind {}", data[pos]));
        }
        let kind = data[pos + 33];
        let extra = match kind {
            0 => null_terminated_len(data, pos + 46)? + 5,
            2 => null_terminated_len(data, pos + 47)? + 6,
            8 => {
                let count = data[pos + 43];
                let mut p = pos + 44;
                for _ in 0..count {
                    let Some(&len) = data.get(p) else {
                        return err("License block too short");
                    };
                    p += 1 + usize::from(len);
                }
                p - (pos + 42)
            }
            32 => 0,
            other => return err(format!("Invalid license block type {other}")),
        };
        let end = pos + 42 + extra;
        let Some(content) = data.get(pos + 1..end) else {
            return err("License block too short");
        };
        let hash = sha512(content);
        blocks.push(LicenseBlock {
            key: data[pos + 1..pos + 33].try_into().expect("32 bytes"),
            hash: hash[..32].try_into().expect("32 bytes"),
            kind,
        });
        pos = end;
    }
    Ok(blocks)
}

fn decompress(bytes: &[u8; 32]) -> Result<EdwardsPoint> {
    CompressedEdwardsY(*bytes).decompress().ok_or(crate::Error("Invalid license key point".into()))
}

/// The final public key of the chain.
pub fn derive_license_key(blocks: &[LicenseBlock]) -> Result<[u8; 32]> {
    let mut parent = decompress(&LICENSE_ROOT_KEY)?;
    for block in blocks {
        let mut scalar = block.hash;
        scalar[0] &= 0xf8;
        scalar[31] &= 0x3f;
        scalar[31] |= 0x40;
        parent = Scalar::from_bytes_mod_order(scalar) * decompress(&block.key)? + parent;
    }
    Ok(parent.compress().to_bytes())
}

/// Shared secret of the new protocol (`initivexpand2`).
pub fn shared_secret2(server_key: &[u8; 32], temp_private: &[u8; 32]) -> Result<[u8; 64]> {
    let mut scalar = *temp_private;
    scalar[31] &= 0x7f;
    let shared = Scalar::from_bytes_mod_order(scalar) * decompress(server_key)?;
    Ok(sha512(shared.compress().as_bytes()))
}

pub fn temporary_public_key(private: &[u8; 32]) -> [u8; 32] {
    EdwardsPoint::mul_base(&Scalar::from_bytes_mod_order(*private)).compress().to_bytes()
}

/// A temporary Ed25519 key pair: (public, private).
pub fn generate_temporary_key() -> Result<([u8; 32], [u8; 32])> {
    let mut private = [0u8; 32];
    getrandom::fill(&mut private).or_else(|e| err(format!("No random data: {e}")))?;
    private[0] &= 248;
    private[31] &= 127;
    private[31] |= 64;
    Ok((temporary_public_key(&private), private))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_licenses_are_errors_not_panics() {
        assert!(parse_license(&[]).is_err());
        assert!(parse_license(&[2]).is_err());
        assert!(parse_license(&[1, 0, 0]).is_err());
        let mut block = vec![1u8, 0];
        block.extend([0u8; 32]);
        block.push(0); // intermediate block with no room for its name
        block.extend([0u8; 8]);
        assert!(parse_license(&block).is_err());
    }
}
