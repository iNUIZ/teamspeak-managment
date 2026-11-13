//! Channels of a virtual server.

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use std::collections::HashMap;

use super::ts::{body_params, params, FlexJson, VsRequest};
use crate::error::ApiResult;
use crate::state::AppState;

type CidPath = Path<HashMap<String, String>>;

fn cid_of(path: &HashMap<String, String>) -> String {
    path.get("cid").cloned().unwrap_or_default()
}

fn with_cid(cid: &str, body: &Value) -> Vec<(String, String)> {
    let mut all = params(&[("cid", cid)]);
    all.extend(body_params(body).into_iter().filter(|(key, _)| key != "cid"));
    all
}

async fn list(request: VsRequest) -> ApiResult<Json<Value>> {
    let flags = params(&[("-topic", ""), ("-flags", ""), ("-voice", ""), ("-limits", ""), ("-icon", ""), ("-secondsempty", "")]);
    Ok(Json(request.read_urgent("channellist", flags).await?))
}

async fn info(request: VsRequest, Path(path): CidPath) -> ApiResult<Json<Value>> {
    Ok(Json(request.read("channelinfo", params(&[("cid", &cid_of(&path))])).await?))
}

async fn create(request: VsRequest, FlexJson(body): FlexJson) -> ApiResult<(StatusCode, Json<Value>)> {
    let result = request
        .change_resolving("channel.create", "channel", None, "channelcreate", body_params(&body), |answer| {
            answer[0]["cid"].as_str().map(str::to_owned).or_else(|| answer[0]["cid"].as_i64().map(|cid| cid.to_string()))
        })
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update(request: VsRequest, Path(path): CidPath, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cid = cid_of(&path);
    Ok(Json(request.change("channel.update", "channel", Some(cid.clone()), "channeledit", with_cid(&cid, &body)).await?))
}

async fn remove(request: VsRequest, Path(path): CidPath, Query(query): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    let cid = cid_of(&path);
    let force = query.get("force").filter(|force| !force.is_empty()).map_or("1", String::as_str);
    Ok(Json(request.change("channel.delete", "channel", Some(cid.clone()), "channeldelete", params(&[("cid", &cid), ("force", force)])).await?))
}

async fn move_channel(request: VsRequest, Path(path): CidPath, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cid = cid_of(&path);
    Ok(Json(request.change("channel.move", "channel", Some(cid.clone()), "channelmove", with_cid(&cid, &body)).await?))
}

async fn permissions(request: VsRequest, Path(path): CidPath) -> ApiResult<Json<Value>> {
    Ok(Json(request.read("channelpermlist", params(&[("cid", &cid_of(&path)), ("-permsid", "")])).await?))
}

async fn add_permission(request: VsRequest, Path(path): CidPath, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cid = cid_of(&path);
    Ok(Json(request.change("channel.permission_add", "channel", Some(cid.clone()), "channeladdperm", with_cid(&cid, &body)).await?))
}

async fn delete_permission(request: VsRequest, Path(path): CidPath, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cid = cid_of(&path);
    Ok(Json(request.change("channel.permission_delete", "channel", Some(cid.clone()), "channeldelperm", with_cid(&cid, &body)).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{cid}", get(info).put(update).delete(remove))
        .route("/{cid}/move", post(move_channel))
        .route("/{cid}/permissions", get(permissions).put(add_permission).delete(delete_permission))
}
