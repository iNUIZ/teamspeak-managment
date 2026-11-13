//! Check a copy of a real database, opened read-only. Prints only counts and yes/no answers.
//!
//! TEAMHUB_ENCRYPTION_KEY=... [TEAMHUB_ADMIN_PASSWORD=...] cargo run -p teamhub-db --example verify_live -- <file>

use serde_json::Value;
use teamhub_core::{password, FieldCipher};
use teamhub_db::{models, Db};
use teamhub_ts_proto::Identity;

fn text(value: &Value, key: &str) -> Option<String> {
    match &value[key] {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: verify_live <file>")?;
    let key = std::env::var("TEAMHUB_ENCRYPTION_KEY").map_err(|_| "TEAMHUB_ENCRYPTION_KEY is not set")?;

    let db = Db::open_read_only(&format!("file:{path}")).await?;
    println!("schema: matches");

    let mut rows = 0;
    for (table, count) in models::decode_all(db.pool()).await? {
        println!("  {table}: {count}");
        rows += count;
    }
    println!("decoded {rows} rows in all tables");

    let cipher = FieldCipher::new(&key)?;
    for server in db.servers().await? {
        let api_key = cipher.decrypt(&server.api_key)?;
        println!("server {}: apiKey decrypted ({} chars)", server.id, api_key.chars().count());
        if let Some(ssh) = &server.ssh_password {
            println!("server {}: sshPassword decrypted ({} chars)", server.id, cipher.decrypt(ssh)?.chars().count());
        }
    }

    let mut failures = 0;
    for bot in db.music_bots().await? {
        let Some(stored) = &bot.identity_data else {
            println!("bot {}: no identity", bot.id);
            continue;
        };
        let json: Value = serde_json::from_str(&cipher.decrypt(stored)?)?;
        let (Some(scalar), Some(offset)) = (text(&json, "privateKeyBigInt"), text(&json, "keyOffset")) else {
            return Err(format!("bot {}: identity fields missing", bot.id).into());
        };
        let identity = Identity::from_stored(&scalar, &offset)?;
        let uid = text(&json, "uid") == Some(identity.uid.clone());
        let key = text(&json, "publicKeyString") == Some(identity.public_key_string.clone());
        failures += usize::from(!uid || !key);
        println!(
            "bot {}: uid same={uid} public key same={key} security level={} created={}",
            bot.id,
            identity.security_level(),
            bot.created_at.0.format("%Y-%m-%d"),
        );
    }

    if let Ok(admin_password) = std::env::var("TEAMHUB_ADMIN_PASSWORD") {
        let user = db.user_by_username("admin").await?.ok_or("no admin user")?;
        let matches = password::verify(&admin_password, &user.password_hash);
        println!("admin password matches: {matches}");
    }
    println!("{}", if failures == 0 { "ALL OK" } else { "IDENTITY MISMATCH" });
    db.close().await;
    if failures > 0 {
        return Err("identity mismatch".into());
    }
    Ok(())
}
