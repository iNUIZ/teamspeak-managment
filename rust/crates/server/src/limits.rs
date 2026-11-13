//! Rate limits and the headers every response carries.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::client_ip::{address_rate_key, client_ip};
use crate::rate::{Hit, RateLimiter};
use crate::state::AppState;

fn limited(limiter: &RateLimiter, key: &str, refusal: Value, request: Request, next: Next) -> impl std::future::Future<Output = Response> {
    let hit = limiter.hit(key);
    async move {
        let mut response = if hit.allowed {
            next.run(request).await
        } else {
            (axum::http::StatusCode::TOO_MANY_REQUESTS, Json(refusal)).into_response()
        };
        add_rate_headers(&mut response, hit);
        response
    }
}

fn add_rate_headers(response: &mut Response, hit: Hit) {
    let reset = hit.reset_after.as_secs() + u64::from(hit.reset_after.subsec_nanos() > 0);
    let headers = response.headers_mut();
    for (name, value) in [("ratelimit-limit", hit.limit), ("ratelimit-remaining", hit.remaining), ("ratelimit-reset", reset)] {
        if let Ok(value) = HeaderValue::from_str(&value.to_string()) {
            headers.insert(HeaderName::from_static(name), value);
        }
    }
    if !hit.allowed {
        if let Ok(value) = HeaderValue::from_str(&reset.to_string()) {
            headers.insert(header::RETRY_AFTER, value);
        }
    }
}

fn rate_key(state: &AppState, request: &Request) -> String {
    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
    address_rate_key(client_ip(&state.trust_proxy, peer, request.headers()))
}

pub async fn auth_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let key = rate_key(&state, &request);
    limited(&state.limiters.auth, &key, json!({"error": "Too many attempts, please try again later"}), request, next).await
}

pub async fn me_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let key = rate_key(&state, &request);
    limited(&state.limiters.me, &key, json!("Too many requests, please try again later."), request, next).await
}

/// The protective headers the old program sent through helmet.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        ("content-security-policy", "default-src 'self';base-uri 'self';font-src 'self' https: data:;form-action 'self';frame-ancestors 'self';img-src 'self' data:;object-src 'none';script-src 'self';script-src-attr 'none';style-src 'self' https: 'unsafe-inline';upgrade-insecure-requests"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("origin-agent-cluster", "?1"),
        ("referrer-policy", "no-referrer"),
        ("strict-transport-security", "max-age=31536000; includeSubDomains"),
        ("x-content-type-options", "nosniff"),
        ("x-dns-prefetch-control", "off"),
        ("x-download-options", "noopen"),
        ("x-frame-options", "SAMEORIGIN"),
        ("x-permitted-cross-domain-policies", "none"),
        ("x-xss-protection", "0"),
    ] {
        headers.insert(HeaderName::from_static(name), HeaderValue::from_static(value));
    }
    response
}
