//! AES-128-EAX, per-packet key derivation and hash helpers. Ported from `tslib/crypto.ts`.

use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes::Aes128;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};

pub const MAC_LEN: usize = 8;
pub const INIT_MAC: &[u8; 8] = b"TS3INIT1";
/// "c:\windows\system\firewall32.cpl" split into key and nonce.
pub const DUMMY_KEY: &[u8; 16] = b"c:\\windows\\syste";
pub const DUMMY_NONCE: &[u8; 16] = b"m\\firewall32.cpl";
pub const INIT_VERSION: u32 = 1_566_914_096; // 3.5.0 [Stable]

const BLOCK: usize = 16;
type Block = [u8; BLOCK];

pub fn sha1(data: &[u8]) -> [u8; 20] {
    Sha1::digest(data).into()
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn sha512(data: &[u8]) -> [u8; 64] {
    Sha512::digest(data).into()
}

pub fn hash_password(password: &str) -> String {
    if password.is_empty() {
        return String::new();
    }
    B64.encode(sha1(password.as_bytes()))
}

fn encrypt_block(cipher: &Aes128, block: &Block) -> Block {
    let mut out = (*block).into();
    cipher.encrypt_block(&mut out);
    out.into()
}

fn xor_block(a: &Block, b: &Block) -> Block {
    std::array::from_fn(|i| a[i] ^ b[i])
}

fn double_block(block: &Block) -> Block {
    let mut out = [0u8; BLOCK];
    let mut carry = 0;
    for i in (0..BLOCK).rev() {
        out[i] = (block[i] << 1) | carry;
        carry = block[i] >> 7;
    }
    if carry != 0 {
        out[BLOCK - 1] ^= 0x87;
    }
    out
}

/// OMAC1 of `[0; 15] || tag || data`, the building block of EAX.
fn eax_omac(cipher: &Aes128, tag: u8, data: &[u8]) -> Block {
    let k1 = double_block(&encrypt_block(cipher, &[0; BLOCK]));
    let k2 = double_block(&k1);

    let mut tag_block = [0u8; BLOCK];
    tag_block[BLOCK - 1] = tag;
    let mut x = encrypt_block(cipher, &tag_block);
    if data.is_empty() {
        // The tag block was the whole message: it is complete, so it takes K1.
        return encrypt_block(cipher, &xor_block(&tag_block, &k1));
    }

    let last_start = (data.len() - 1) / BLOCK * BLOCK;
    for chunk in data[..last_start].chunks_exact(BLOCK) {
        let block: Block = chunk.try_into().expect("exact chunk");
        x = encrypt_block(cipher, &xor_block(&x, &block));
    }
    let rest = &data[last_start..];
    let mut last = [0u8; BLOCK];
    last[..rest.len()].copy_from_slice(rest);
    let last = if rest.len() == BLOCK {
        xor_block(&last, &k1)
    } else {
        last[rest.len()] = 0x80;
        xor_block(&last, &k2)
    };
    encrypt_block(cipher, &xor_block(&x, &last))
}

fn ctr(cipher: &Aes128, counter: &Block, data: &[u8]) -> Vec<u8> {
    let mut counter = *counter;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(BLOCK) {
        let stream = encrypt_block(cipher, &counter);
        out.extend(chunk.iter().zip(stream).map(|(byte, key)| byte ^ key));
        for byte in counter.iter_mut().rev() {
            *byte = byte.wrapping_add(1);
            if *byte != 0 {
                break;
            }
        }
    }
    out
}

fn eax_tag(cipher: &Aes128, n: &Block, header: &[u8], ciphertext: &[u8]) -> [u8; MAC_LEN] {
    let h = eax_omac(cipher, 1, header);
    let c = eax_omac(cipher, 2, ciphertext);
    let tag = xor_block(&xor_block(n, &h), &c);
    tag[..MAC_LEN].try_into().expect("mac length")
}

pub fn eax_encrypt(key: &[u8; 16], nonce: &[u8; 16], header: &[u8], plain: &[u8]) -> (Vec<u8>, [u8; MAC_LEN]) {
    let cipher = Aes128::new(&(*key).into());
    let n = eax_omac(&cipher, 0, nonce);
    let ciphertext = ctr(&cipher, &n, plain);
    let mac = eax_tag(&cipher, &n, header, &ciphertext);
    (ciphertext, mac)
}

pub fn eax_decrypt(key: &[u8; 16], nonce: &[u8; 16], header: &[u8], ciphertext: &[u8], mac: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes128::new(&(*key).into());
    let n = eax_omac(&cipher, 0, nonce);
    let tag = eax_tag(&cipher, &n, header, ciphertext);
    if mac.len() != MAC_LEN || tag.iter().zip(mac).fold(0, |diff, (a, b)| diff | (a ^ b)) != 0 {
        return None;
    }
    Some(ctr(&cipher, &n, ciphertext))
}

/// Key and nonce of one packet, derived from the session's IV struct.
pub fn derive_key_nonce(from_server: bool, packet_id: u16, generation: u32, packet_type: u8, iv: &[u8]) -> ([u8; 16], [u8; 16]) {
    let mut tmp = vec![0u8; if iv.len() == 20 { 26 } else { 70 }];
    tmp[0] = if from_server { 0x30 } else { 0x31 };
    tmp[1] = packet_type;
    tmp[2..6].copy_from_slice(&generation.to_be_bytes());
    let len = iv.len().min(tmp.len() - 6);
    tmp[6..6 + len].copy_from_slice(&iv[..len]);

    let hash = sha256(&tmp);
    let mut key: [u8; 16] = hash[..16].try_into().expect("16 bytes");
    let nonce: [u8; 16] = hash[16..32].try_into().expect("16 bytes");
    let id = packet_id.to_be_bytes();
    key[0] ^= id[0];
    key[1] ^= id[1];
    (key, nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eax_rejects_a_changed_message() {
        let (ct, mac) = eax_encrypt(DUMMY_KEY, DUMMY_NONCE, b"head", b"hello");
        assert_eq!(eax_decrypt(DUMMY_KEY, DUMMY_NONCE, b"head", &ct, &mac).unwrap(), b"hello");
        assert!(eax_decrypt(DUMMY_KEY, DUMMY_NONCE, b"HEAD", &ct, &mac).is_none());
    }
}
