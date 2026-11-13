//! Custom chat replies and who may use the media commands. Admins only.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_db::library::CommandFields;
use teamhub_db::models::ChatCommand;
use teamhub_voice::chat::commands::{is_builtin, normalize_command_name};
use teamhub_voice::chat::permissions::{parse, permissions_key, Permissions};

use super::radio::config_id;
use super::ts::FlexJson;
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

type Ids = Path<HashMap<String, String>>;

const MAX_RESPONSE: usize = 900;
const MAX_DESCRIPTION: usize = 120;

const ABOUT: &str = "## About\n\nWelcome to our TeamSpeak community. Music bots, radio, and chat commands keep the lobby moving.\n\nType **!help** for music commands or **!commands** for custom replies.\n\n_Edit this blurb in Music Bots → Commands._";

/// Name, description, reply.
const PRESETS: [(&str, &str, &str); 5] = [
    (
        "rules",
        "Server rules",
        "## Server rules\n\n1. Be respectful — no harassment or hate speech.\n2. No spam or excessive caps in chat/voice.\n3. Keep music requests reasonable; staff may skip tracks.\n4. Follow staff instructions.\n\n_Edit this text in Music Bots → Commands._",
    ),
    (
        "links",
        "Useful community links",
        "## Links\n\n- **Website:** https://example.com\n- **Discord:** https://discord.gg/your-invite\n- **Donate:** https://example.com/donate\n\n_Replace these URLs in Music Bots → Commands._",
    ),
    ("discord", "Discord invite", "## Discord\n\nJoin us: https://discord.gg/your-invite\n\n_Or fold this into !links and disable !discord._"),
    ("info", "Short server / community blurb", ABOUT),
    ("about", "Alias-style community blurb (same idea as !info)", ABOUT),
];

fn command_json(command: &ChatCommand) -> Value {
    json!({
        "id": command.id,
        "serverConfigId": command.server_config_id,
        "name": command.name,
        "response": command.response,
        "description": command.description,
        "enabled": command.enabled,
        "createdAt": command.created_at.to_iso(),
        "updatedAt": command.updated_at.to_iso(),
    })
}

struct Payload {
    name: String,
    response: String,
    description: Option<String>,
    enabled: bool,
}

impl Payload {
    fn fields(&self) -> CommandFields<'_> {
        CommandFields { name: &self.name, response: &self.response, description: self.description.as_deref(), enabled: self.enabled }
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

fn validate(name: &Value, response: &Value, description: &Value, enabled: bool) -> ApiResult<Payload> {
    let name = normalize_command_name(name.as_str().unwrap_or_default());
    if name.is_empty() {
        return Err(ApiError::new(400, "name is required (letters, numbers, hyphens, underscores; no \"!\")"));
    }
    if is_builtin(&name) {
        return Err(ApiError::new(400, format!("\"!{name}\" is a built-in command and cannot be overridden")));
    }
    let response = response.as_str().map(str::trim).unwrap_or_default();
    if response.is_empty() {
        return Err(ApiError::new(400, "response is required"));
    }
    if response.encode_utf16().count() > MAX_RESPONSE {
        return Err(ApiError::new(400, format!("response must be at most {MAX_RESPONSE} characters")));
    }
    let description = description.as_str().map(str::trim).filter(|text| !text.is_empty()).map(|text| text.chars().take(MAX_DESCRIPTION).collect());
    Ok(Payload { name, response: response.to_owned(), description, enabled })
}

fn permissions_scope(path: &HashMap<String, String>) -> ApiResult<(i64, i64)> {
    let invalid = || ApiError::new(400, "A valid server connection and virtual server are required");
    let config = config_id(path).map_err(|_| invalid())?;
    let sid = path.get("sid").and_then(|sid| sid.parse().ok()).filter(|sid| *sid > 0).ok_or_else(invalid)?;
    Ok((config, sid))
}

async fn read_permissions(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    let (config, sid) = permissions_scope(&path)?;
    let stored = state.db.setting(&permissions_key(config, sid)).await?;
    Ok(Json(Permissions::from_stored(stored.as_deref()).to_json()))
}

async fn save_permissions(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let (config, sid) = permissions_scope(&path)?;
    let policy = parse(&body).ok_or_else(|| ApiError::new(400, "Supply playback, queue and video access: everyone or server_groups with positive serverGroupIds"))?;
    let answer = policy.to_json();
    state.db.set_setting(&permissions_key(config, sid), &answer.to_string()).await?;
    Ok(Json(answer))
}

async fn presets(_admin: AdminUser) -> Json<Value> {
    Json(PRESETS.iter().map(|(name, description, response)| json!({"name": name, "description": description, "response": response})).collect())
}

/// Add the missing templates switched off, until an admin has edited them.
async fn seed_presets(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    let config = config_id(&path)?;
    if state.db.server(config).await?.is_none() {
        return Err(ApiError::new(404, "Server not found"));
    }
    let existing = state.db.chat_commands(config).await?;
    let mut created = Vec::new();
    for (name, description, response) in PRESETS {
        if existing.iter().any(|command| command.name == name) || is_builtin(name) {
            continue;
        }
        state.db.create_chat_command(config, &CommandFields { name, response, description: Some(description), enabled: false }).await?;
        created.push(name);
    }
    let commands = state.db.chat_commands(config).await?;
    Ok(Json(json!({"created": created.len(), "createdNames": created, "commands": commands.iter().map(command_json).collect::<Vec<_>>()})))
}

async fn list(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    let commands = state.db.chat_commands(config_id(&path)?).await?;
    Ok(Json(commands.iter().map(command_json).collect()))
}

/// Two requests can pick the same name at once; the database decides, with the same answer.
fn name_taken(name: &str, error: teamhub_db::Error) -> ApiError {
    if error.is_unique_violation() {
        ApiError::new(409, format!("Command \"!{name}\" already exists"))
    } else {
        error.into()
    }
}

async fn create(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids, FlexJson(body): FlexJson) -> ApiResult<(StatusCode, Json<Value>)> {
    let config = config_id(&path)?;
    let enabled = body.get("enabled").is_none_or(truthy);
    let payload = validate(&body["name"], &body["response"], &body["description"], enabled)?;
    if state.db.server(config).await?.is_none() {
        return Err(ApiError::new(404, "Server not found"));
    }
    let command = state.db.create_chat_command(config, &payload.fields()).await.map_err(|error| name_taken(&payload.name, error))?;
    Ok((StatusCode::CREATED, Json(command_json(&command))))
}

async fn update(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let config = config_id(&path)?;
    let not_found = || ApiError::new(404, "Chat command not found");
    let id: i64 = path.get("id").and_then(|id| id.parse().ok()).ok_or_else(not_found)?;
    let existing = state.db.chat_command_by_id(config, id).await?.ok_or_else(not_found)?;
    let or_existing = |key: &str, current: &str| match body.get(key) {
        None | Some(Value::Null) => json!(current),
        Some(value) => value.clone(),
    };
    let description = body.get("description").cloned().unwrap_or_else(|| json!(existing.description));
    let enabled = body.get("enabled").map_or(existing.enabled, truthy);
    let payload = validate(&or_existing("name", &existing.name), &or_existing("response", &existing.response), &description, enabled)?;
    let command = state.db.update_chat_command(config, id, &payload.fields()).await.map_err(|error| name_taken(&payload.name, error))?;
    Ok(Json(command_json(&command)))
}

async fn remove(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    let not_found = || ApiError::new(404, "Chat command not found");
    let id: i64 = path.get("id").and_then(|id| id.parse().ok()).ok_or_else(not_found)?;
    if !state.db.delete_chat_command(config_id(&path)?, id).await? {
        return Err(not_found());
    }
    Ok(Json(json!({"success": true})))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/presets", get(presets))
        .route("/seed-presets", post(seed_presets))
        .route("/permissions/{sid}", get(read_permissions).put(save_permissions))
        .route("/{id}", put(update).delete(remove))
}
