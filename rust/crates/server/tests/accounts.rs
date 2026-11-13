//! The account API from the outside, through the whole router and a real database.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use teamhub_core::{Config, FieldCipher};
use teamhub_db::Db;
use teamhub_server::client_ip::TrustProxy;
use teamhub_server::pool::ConnectionPool;
use teamhub_server::state::{AppState, Limiters};
use tower::ServiceExt;

const PASSWORD: &str = "Parola123";
const SECRET: &str = "test-jwt-secret";

struct Api {
    app: Router,
    db: Db,
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
}

async fn api() -> Api {
    let db = Db::open_in_memory().await.unwrap();
    let config = Config::from_lookup(|name| match name {
        "JWT_SECRET" => Some(SECRET.to_owned()),
        "ENCRYPTION_KEY" => Some("test-encryption-key".to_owned()),
        "FRONTEND_URL" => Some("https://panel.example".to_owned()),
        _ => None,
    })
    .unwrap();
    let cipher = FieldCipher::new(&config.encryption_source).unwrap();
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        cipher: Arc::new(cipher),
        trust_proxy: Arc::new(TrustProxy::None),
        limiters: Arc::new(Limiters::default()),
        pool: Arc::new(ConnectionPool::new(false)),
        bots: None,
        activity: Arc::new(teamhub_voice::activity::ActivityLog::new(db.clone())),
    };
    Api { app: teamhub_server::router(state), db }
}

impl Api {
    async fn request(&self, method: Method, path: &str, token: Option<&str>, body: Option<Value>) -> Answer {
        self.request_from(method, path, token, body, "9.9.9.9:1").await
    }

    async fn request_from(&self, method: Method, path: &str, token: Option<&str>, body: Option<Value>, peer: &str) -> Answer {
        let mut request = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let mut request = request.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string()))).unwrap();
        let peer: SocketAddr = peer.parse().unwrap();
        request.extensions_mut().insert(ConnectInfo(peer));
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        Answer { status, headers, body: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
    }

    async fn post(&self, path: &str, body: Value) -> Answer {
        self.request(Method::POST, path, None, Some(body)).await
    }

    /// Set up the first admin and sign in.
    async fn admin(&self) -> (String, String) {
        assert_eq!(self.post("/api/setup/init", json!({"username": "admin", "password": PASSWORD})).await.status, StatusCode::CREATED);
        let login = self.post("/api/auth/login", json!({"username": "admin", "password": PASSWORD})).await;
        assert_eq!(login.status, StatusCode::OK);
        (login.body["accessToken"].as_str().unwrap().to_owned(), login.body["refreshToken"].as_str().unwrap().to_owned())
    }

    async fn viewer_token(&self, admin: &str, name: &str) -> String {
        let created = self
            .request(Method::POST, "/api/users", Some(admin), Some(json!({"username": name, "password": PASSWORD, "displayName": name})))
            .await;
        assert_eq!(created.status, StatusCode::CREATED);
        self.post("/api/auth/login", json!({"username": name, "password": PASSWORD})).await.body["accessToken"].as_str().unwrap().to_owned()
    }
}

#[tokio::test]
async fn health_keeps_its_shape_and_protective_headers() {
    let api = api().await;
    let answer = api.request(Method::GET, "/api/health", None, None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body["status"], "ok");
    assert_eq!(answer.body["version"], env!("CARGO_PKG_VERSION"));
    assert!(answer.body["timestamp"].as_str().unwrap().ends_with('Z'));
    assert_eq!(answer.headers["x-content-type-options"], "nosniff");
    assert_eq!(answer.headers["x-frame-options"], "SAMEORIGIN");
}

#[tokio::test]
async fn setup_works_once() {
    let api = api().await;
    assert_eq!(api.request(Method::GET, "/api/setup/status", None, None).await.body, json!({"needsSetup": true}));
    assert_eq!(api.post("/api/setup/init", json!({"username": "a"})).await.status, StatusCode::BAD_REQUEST);
    let weak = api.post("/api/setup/init", json!({"username": "a", "password": "weak"})).await;
    assert_eq!(weak.body["error"], "Password must be at least 8 characters long");
    let created = api.post("/api/setup/init", json!({"username": "a", "password": PASSWORD})).await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["success"], true);
    assert_eq!(api.request(Method::GET, "/api/setup/status", None, None).await.body, json!({"needsSetup": false}));
    let again = api.post("/api/setup/init", json!({"username": "b", "password": PASSWORD})).await;
    assert_eq!((again.status, again.body["error"].as_str()), (StatusCode::FORBIDDEN, Some("Setup already completed")));
    let user = api.db.user_by_username("a").await.unwrap().unwrap();
    assert_eq!((user.role.as_str(), user.display_name.as_str()), ("admin", "a"));
}

#[tokio::test]
async fn login_gives_the_old_shape_and_refuses_alike() {
    let api = api().await;
    api.admin().await;
    let login = api.post("/api/auth/login", json!({"username": "admin", "password": PASSWORD})).await;
    assert_eq!(login.body["user"]["username"], "admin");
    assert_eq!(login.body["user"]["role"], "admin");
    assert_eq!(login.body["refreshToken"].as_str().unwrap().len(), 128);
    assert!(api.db.user_by_username("admin").await.unwrap().unwrap().last_login_at.is_some());

    let wrong = api.post("/api/auth/login", json!({"username": "admin", "password": "Yanlis123"})).await;
    let nobody = api.post("/api/auth/login", json!({"username": "kimse", "password": PASSWORD})).await;
    assert_eq!((wrong.status, wrong.body.clone()), (StatusCode::UNAUTHORIZED, json!({"error": "Invalid credentials"})));
    assert_eq!((nobody.status, nobody.body), (StatusCode::UNAUTHORIZED, wrong.body), "a guess cannot tell which was wrong");
    assert_eq!(api.post("/api/auth/login", json!({"username": "admin"})).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_hash_from_the_old_program_signs_in() {
    let api = api().await;
    // bcryptjs output for this password; old accounts keep working.
    let hash = teamhub_core::password::hash(PASSWORD).unwrap().replacen("$2b$", "$2a$", 1);
    api.db.create_user("eski", &hash, "Eski", "viewer").await.unwrap();
    assert_eq!(api.post("/api/auth/login", json!({"username": "eski", "password": PASSWORD})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_disabled_user_cannot_use_anything() {
    let api = api().await;
    let (admin, _) = api.admin().await;
    let viewer = api.viewer_token(&admin, "izleyici").await;
    let id = api.db.user_by_username("izleyici").await.unwrap().unwrap().id;
    assert_eq!(api.request(Method::GET, "/api/auth/me", Some(&viewer), None).await.status, StatusCode::OK);
    let disabled = api.request(Method::PUT, &format!("/api/users/{id}"), Some(&admin), Some(json!({"enabled": false}))).await;
    assert_eq!(disabled.status, StatusCode::NO_CONTENT);
    let refused = api.request(Method::GET, "/api/auth/me", Some(&viewer), None).await;
    assert_eq!((refused.status, refused.body), (StatusCode::UNAUTHORIZED, json!({"error": "User account disabled or deleted"})));
    assert_eq!(api.post("/api/auth/login", json!({"username": "izleyici", "password": PASSWORD})).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tokens_are_checked() {
    let api = api().await;
    let (admin, _) = api.admin().await;
    assert_eq!(api.request(Method::GET, "/api/auth/me", None, None).await.body, json!({"error": "No token provided"}));
    assert_eq!(api.request(Method::GET, "/api/auth/me", Some("nonsense"), None).await.body, json!({"error": "Invalid or expired token"}));
    let me = api.request(Method::GET, "/api/auth/me", Some(&admin), None).await;
    assert_eq!(me.body["user"]["displayName"], "admin");
}

#[tokio::test]
async fn the_role_comes_from_the_database_not_the_token() {
    let api = api().await;
    let (admin, _) = api.admin().await;
    let promoted = api.viewer_token(&admin, "ali").await;
    let id = api.db.user_by_username("ali").await.unwrap().unwrap().id;
    assert_eq!(api.request(Method::GET, "/api/users", Some(&promoted), None).await.status, StatusCode::FORBIDDEN);
    api.request(Method::PUT, &format!("/api/users/{id}"), Some(&admin), Some(json!({"role": "admin"}))).await;
    assert_eq!(api.request(Method::GET, "/api/users", Some(&promoted), None).await.status, StatusCode::OK, "same token");
}

#[tokio::test]
async fn refresh_tokens_rotate_and_work_once() {
    let api = api().await;
    let (_, first) = api.admin().await;
    let renewed = api.post("/api/auth/refresh", json!({"refreshToken": first})).await;
    assert_eq!(renewed.status, StatusCode::OK);
    let second = renewed.body["refreshToken"].as_str().unwrap().to_owned();
    assert_ne!(second, first);
    assert_eq!(api.request(Method::GET, "/api/auth/me", renewed.body["accessToken"].as_str(), None).await.status, StatusCode::OK);

    assert_eq!(api.post("/api/auth/refresh", json!({"refreshToken": first})).await.status, StatusCode::UNAUTHORIZED, "used once");
    assert_eq!(api.post("/api/auth/refresh", json!({"refreshToken": "unknown"})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.post("/api/auth/refresh", json!({})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(api.post("/api/auth/refresh", json!({"refreshToken": second})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn an_expired_refresh_token_is_refused_and_removed() {
    let api = api().await;
    let (_, token) = api.admin().await;
    sqlx::query(r#"UPDATE "RefreshToken" SET "expiresAt" = 1"#).execute(api.db.pool()).await.unwrap();
    assert_eq!(api.post("/api/auth/refresh", json!({"refreshToken": token})).await.status, StatusCode::UNAUTHORIZED);
    assert!(api.db.refresh_token(&token).await.unwrap().is_none());
}

#[tokio::test]
async fn logout_ends_the_session() {
    let api = api().await;
    let (_, token) = api.admin().await;
    assert_eq!(api.post("/api/auth/logout", json!({"refreshToken": token})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api.post("/api/auth/refresh", json!({"refreshToken": token})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.post("/api/auth/logout", json!({})).await.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn changing_a_password_ends_every_session() {
    let api = api().await;
    let (access, refresh) = api.admin().await;
    let change = |current: &'static str, new: &'static str| json!({"currentPassword": current, "newPassword": new});
    let wrong = api.request(Method::PUT, "/api/auth/password", Some(&access), Some(change("Yanlis123", "YeniParola1"))).await;
    assert_eq!((wrong.status, wrong.body["error"].as_str()), (StatusCode::UNAUTHORIZED, Some("Current password is incorrect")));
    let weak = api.request(Method::PUT, "/api/auth/password", Some(&access), Some(change(PASSWORD, "weak"))).await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    let changed = api.request(Method::PUT, "/api/auth/password", Some(&access), Some(change(PASSWORD, "YeniParola1"))).await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    assert_eq!(api.post("/api/auth/refresh", json!({"refreshToken": refresh})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.post("/api/auth/login", json!({"username": "admin", "password": PASSWORD})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.post("/api/auth/login", json!({"username": "admin", "password": "YeniParola1"})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn sign_in_is_rate_limited_per_address() {
    let api = api().await;
    let attempt = json!({"username": "kimse", "password": "x"});
    let mut last = None;
    for _ in 0..15 {
        last = Some(api.request_from(Method::POST, "/api/auth/login", None, Some(attempt.clone()), "1.1.1.1:5").await);
    }
    let last = last.unwrap();
    assert_eq!(last.status, StatusCode::UNAUTHORIZED);
    assert_eq!(last.headers["ratelimit-remaining"], "0");
    assert_eq!(last.headers["ratelimit-limit"], "15");
    let refused = api.request_from(Method::POST, "/api/auth/login", None, Some(attempt.clone()), "1.1.1.1:6").await;
    assert_eq!((refused.status, refused.body), (StatusCode::TOO_MANY_REQUESTS, json!({"error": "Too many attempts, please try again later"})));
    assert!(refused.headers.contains_key("retry-after"));
    let other = api.request_from(Method::POST, "/api/auth/login", None, Some(attempt), "2.2.2.2:5").await;
    assert_eq!(other.status, StatusCode::UNAUTHORIZED, "another address has its own budget");
}

#[tokio::test]
async fn users_are_for_admins_and_are_audited() {
    let api = api().await;
    let (admin, _) = api.admin().await;
    assert_eq!(api.request(Method::GET, "/api/users", None, None).await.status, StatusCode::UNAUTHORIZED);
    let viewer = api.viewer_token(&admin, "izleyici").await;
    assert_eq!(api.request(Method::GET, "/api/users", Some(&viewer), None).await.body, json!({"error": "Insufficient permissions"}));

    let list = api.request(Method::GET, "/api/users", Some(&admin), None).await;
    let first = &list.body[0];
    assert_eq!(first["username"], "admin");
    assert!(first["createdAt"].as_str().unwrap().ends_with('Z'));
    assert!(first.get("passwordHash").is_none(), "{first}");

    let create = |body: Value| api.request(Method::POST, "/api/users", Some(&admin), Some(body));
    assert_eq!(create(json!({"username": "x"})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(create(json!({"username": "x", "password": PASSWORD, "displayName": "X", "role": "root"})).await.body["error"], "Invalid role. Must be one of: admin, viewer");
    let made = create(json!({"username": "x", "password": PASSWORD, "displayName": "X"})).await;
    assert_eq!(made.status, StatusCode::CREATED);
    assert_eq!(made.body["username"], "x");
    let id = made.body["id"].as_i64().unwrap();
    assert_eq!(api.db.user_by_id(id).await.unwrap().unwrap().role, "viewer");
    assert_eq!(create(json!({"username": "x", "password": PASSWORD, "displayName": "X"})).await.status, StatusCode::CONFLICT);

    assert_eq!(api.request(Method::PUT, &format!("/api/users/{id}"), Some(&admin), Some(json!({"displayName": "Yeni", "role": "admin"}))).await.status, StatusCode::NO_CONTENT);
    let changed = api.db.user_by_id(id).await.unwrap().unwrap();
    assert_eq!((changed.display_name.as_str(), changed.role.as_str(), changed.enabled), ("Yeni", "admin", true));
    assert_eq!(api.request(Method::PUT, &format!("/api/users/{id}"), Some(&admin), Some(json!({"role": "root"}))).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(api.request(Method::PUT, "/api/users/99999", Some(&admin), Some(json!({}))).await.status, StatusCode::NOT_FOUND);
    assert_eq!(api.request(Method::PUT, "/api/users/abc", Some(&admin), Some(json!({}))).await.status, StatusCode::BAD_REQUEST);

    let own = api.db.user_by_username("admin").await.unwrap().unwrap().id;
    assert_eq!(api.request(Method::DELETE, &format!("/api/users/{own}"), Some(&admin), None).await.body["error"], "Cannot delete your own account");
    assert_eq!(api.request(Method::DELETE, &format!("/api/users/{id}"), Some(&admin), None).await.status, StatusCode::NO_CONTENT);
    assert!(api.db.user_by_id(id).await.unwrap().is_none());
    assert_eq!(api.request(Method::DELETE, &format!("/api/users/{id}"), Some(&admin), None).await.status, StatusCode::NOT_FOUND);

    let events = api.db.audit_events(100).await.unwrap();
    let actions: Vec<&str> = events.iter().map(|event| event.action.as_str()).collect();
    assert_eq!(actions.iter().filter(|action| **action == "user.create").count(), 2);
    assert_eq!(actions.iter().filter(|action| **action == "user.update").count(), 1);
    assert_eq!(actions.iter().filter(|action| **action == "user.delete").count(), 1);
    assert!(events.iter().all(|event| event.outcome == "success" && event.actor_username.as_deref() == Some("admin")));
}

#[tokio::test]
async fn deleting_a_user_removes_their_sessions() {
    let api = api().await;
    let (admin, _) = api.admin().await;
    api.viewer_token(&admin, "ali").await;
    let id = api.db.user_by_username("ali").await.unwrap().unwrap().id;
    api.request(Method::DELETE, &format!("/api/users/{id}"), Some(&admin), None).await;
    let left: i64 = sqlx::query_scalar(r#"SELECT count(*) FROM "RefreshToken" WHERE "userId" = ?"#).bind(id).fetch_one(api.db.pool()).await.unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn unknown_paths_say_nothing_to_strangers() {
    let api = api().await;
    let (admin, _) = api.admin().await;
    assert_eq!(api.request(Method::GET, "/api/nothing/here", None, None).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.request(Method::GET, "/api/nothing/here", Some(&admin), None).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_the_panel_address_may_call_from_a_browser() {
    let api = api().await;
    let request = |origin: &'static str| {
        Request::builder().method(Method::OPTIONS).uri("/api/health").header(header::ORIGIN, origin).header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET").body(Body::empty()).unwrap()
    };
    let allowed = api.app.clone().oneshot(request("https://panel.example")).await.unwrap();
    assert_eq!(allowed.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "https://panel.example");
    // The answer names the panel whoever asks, so a browser on another site refuses it.
    let other = api.app.clone().oneshot(request("https://evil.example")).await.unwrap();
    assert_eq!(other.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "https://panel.example");
}
