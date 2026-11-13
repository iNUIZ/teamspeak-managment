//! Values produced by the old TypeScript code must keep working. `vectors.json` holds test data only.

use serde_json::Value;
use teamhub_core::{password, FieldCipher};

fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}

#[test]
fn fields_encrypted_by_the_old_program_decrypt() {
    let v = vectors();
    let cipher = FieldCipher::new(v["source"].as_str().unwrap()).unwrap();
    for field in v["fields"].as_array().unwrap() {
        let plain = field["plain"].as_str().unwrap();
        assert_eq!(cipher.decrypt(field["encrypted"].as_str().unwrap()).unwrap(), plain);
        assert_eq!(cipher.decrypt(&cipher.encrypt(plain).unwrap()).unwrap(), plain);
    }
}

#[test]
fn hashes_made_by_the_old_program_verify() {
    let v = vectors();
    let hash = v["hash"].as_str().unwrap();
    assert!(password::verify(v["password"].as_str().unwrap(), hash));
    assert!(!password::verify("wrong", hash));
}
