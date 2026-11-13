//! What people asked the bots to play, and playing it again. The list shows no addresses:
//! only titles and where they came from.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_voice::chat::service::{history_limit, is_unavailable_error, ReplaySource, MEDIA_UNAVAILABLE, REPLAY_STOPPED};

use super::music_bots::{audited, manager, map_play_error, played_json, ready, save_history};
use super::ts::FlexJson;
use crate::auth::{AdminUser, ServerUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

async fn list(State(state): State<AppState>, access: ServerUser, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    if access.config_id <= 0 {
        return Err(ApiError::new(400, "Invalid config id"));
    }
    let bot = match q.get("botId") {
        None => None,
        Some(raw) => Some(raw.parse::<i64>().ok().filter(|id| *id > 0).ok_or_else(|| ApiError::new(400, "Invalid bot id"))?),
    };
    let limit = history_limit();
    state.db.tidy_music_requests(access.config_id, limit).await?;
    let rows = match bot {
        Some(bot) => state.db.music_requests(access.config_id, bot, limit).await?,
        None => state.db.server_music_requests(access.config_id, limit).await?,
    };
    let rows = rows.iter().map(|row| {
        json!({"id": row.id, "title": row.title, "artist": row.artist, "duration": row.duration, "source": row.source, "status": row.status, "requestedAt": row.requested_at.to_iso()})
    });
    Ok(Json(rows.collect()))
}

async fn gone(state: &AppState, id: i64) -> ApiError {
    if let Err(error) = state.db.mark_music_request_unavailable(id).await {
        tracing::warn!(%error, "Could not mark a request unavailable");
    }
    ApiError::new(410, MEDIA_UNAVAILABLE)
}

async fn replay(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(path): Path<HashMap<String, String>>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let invalid = || ApiError::new(400, "Invalid replay request");
    let id_in_path = |name: &str| path.get(name).and_then(|id| id.parse::<i64>().ok()).filter(|id| *id > 0);
    let (config, request_id) = (id_in_path("configId").ok_or_else(invalid)?, id_in_path("id").ok_or_else(invalid)?);
    let bot_id = body["botId"].as_i64().filter(|id| *id > 0).ok_or_else(invalid)?;
    let enqueue = match body.get("enqueue") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(invalid)?,
    };
    let manager = manager(&state)?;
    let row = state.db.server_music_request(config, request_id).await?.ok_or_else(|| ApiError::new(404, "Request not found"))?;
    let same_server = state.db.music_bot(bot_id).await?.is_some_and(|bot| bot.server_config_id == config);
    if !same_server || !manager.contains(bot_id) {
        return Err(ApiError::new(404, "Music bot not found"));
    }
    let source = manager.chat().replay_source(bot_id, &row).await.map_err(|message| {
        if message == REPLAY_STOPPED {
            ApiError::new(409, message)
        } else {
            ApiError::new(503, message).retryable(true)
        }
    })?;
    match source {
        ReplaySource::Gone => Err(gone(&state, row.id).await),
        ReplaySource::Link(url) => {
            ready(manager, bot_id)?;
            match manager.chat().play_link(bot_id, &url, enqueue).await {
                Ok(played) => Ok(Json(played_json(&played))),
                Err(message) if is_unavailable_error(&message) => Err(gone(&state, row.id).await),
                Err(message) => Err(map_play_error(message)),
            }
        }
        ReplaySource::Item(item) => {
            let bot = ready(manager, bot_id)?;
            let queue = manager.queue(bot_id)?;
            if enqueue {
                queue.lock().expect("queue lock").add(item.clone())?;
            } else {
                let dispatch = async {
                    {
                        let mut queue = queue.lock().expect("queue lock");
                        queue.add(item.clone())?;
                        let last = queue.len() - 1;
                        queue.play_at(last);
                    }
                    bot.play_item(item.clone());
                    bot.settle().await;
                    Ok(())
                };
                audited(&state, &admin, bot_id, "media.music.start", dispatch).await?;
            }
            save_history(&state, bot_id, config, vec![item]);
            Ok(Json(json!({"success": true, "queued": 1})))
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/{id}/replay", post(replay))
}
