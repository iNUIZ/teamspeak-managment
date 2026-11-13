//! Server connections and TeamSpeak-backed routes, against a fake WebQuery server.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode, Uri};
use axum::routing::any;
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_core::{Config, FieldCipher};
use teamhub_db::Db;
use teamhub_server::auth::sign_access_token;
use teamhub_server::client_ip::TrustProxy;
use teamhub_server::pool::ConnectionPool;
use teamhub_server::state::{AppState, Limiters};
use tower::ServiceExt;

const SECRET: &str = "test-jwt-secret";

struct Api {
    app: Router,
    db: Db,
    admin: String,
    viewer: String,
    viewer_id: i64,
    calls: Arc<Mutex<Vec<String>>>,
    port: u16,
}

async fn fake_webquery(calls: Arc<Mutex<Vec<String>>>) -> u16 {
    let handler = move |uri: Uri| {
        let calls = calls.clone();
        async move {
            calls.lock().unwrap().push(uri.path_and_query().map(ToString::to_string).unwrap_or_default());
            if uri.path().ends_with("refused") || uri.query().is_some_and(|query| query.contains("refuse")) {
                return Json(json!({"status": {"code": 2568, "message": "insufficient client permissions"}}));
            }
            if uri.path().ends_with("channelcreate") {
                return Json(json!({"body": [{"cid": "77"}], "status": {"code": 0}}));
            }
            Json(json!({"body": [{"path": uri.path()}], "status": {"code": 0}}))
        }
    };
    let app = Router::new().fallback(any(handler));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

async fn api() -> Api {
    let db = Db::open_in_memory().await.unwrap();
    let config = Config::from_lookup(|name| match name {
        "JWT_SECRET" => Some(SECRET.to_owned()),
        "ENCRYPTION_KEY" => Some("test-encryption-key".to_owned()),
        _ => None,
    })
    .unwrap();
    let cipher = FieldCipher::new(&config.encryption_source).unwrap();
    let admin_id = db.create_user("admin", "x", "Admin", "admin").await.unwrap();
    let viewer_id = db.create_user("viewer", "x", "Viewer", "viewer").await.unwrap();
    let admin = sign_access_token(SECRET, "15m", admin_id, "admin", "admin").unwrap();
    let viewer = sign_access_token(SECRET, "15m", viewer_id, "viewer", "viewer").unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let port = fake_webquery(calls.clone()).await;
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
    Api { app: teamhub_server::router(state), db, admin, viewer, viewer_id, calls, port }
}

impl Api {
    async fn call(&self, method: Method, path: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json").header(header::AUTHORIZATION, format!("Bearer {token}"));
        request = request.header("x-test", "1");
        let mut request = request.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string()))).unwrap();
        request.extensions_mut().insert(ConnectInfo("9.9.9.9:1".parse::<SocketAddr>().unwrap()));
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn add_server(&self) -> i64 {
        let (status, body) = self
            .call(Method::POST, "/api/servers", &self.admin, Some(json!({"name": "S", "host": "127.0.0.1", "webqueryPort": self.port, "apiKey": "gizli-anahtar", "sshUsername": "ssh", "sshPassword": "gizli-ssh"})))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_i64().unwrap()
    }

    fn last_call(&self) -> String {
        self.calls.lock().unwrap().last().cloned().unwrap_or_default()
    }

    async fn audit_actions(&self) -> Vec<(String, String)> {
        self.db.audit_events(100).await.unwrap().into_iter().map(|event| (event.action, event.outcome)).collect()
    }
}

#[tokio::test]
async fn secrets_are_encrypted_and_never_come_back() {
    let api = api().await;
    let id = api.add_server().await;
    let row = api.db.server(id).await.unwrap().unwrap();
    assert!(row.api_key.starts_with("enc:"), "stored encrypted");
    assert!(row.ssh_password.as_deref().unwrap().starts_with("enc:"));

    let (_, list) = api.call(Method::GET, "/api/servers", &api.admin, None).await;
    let text = list.to_string();
    assert!(!text.contains("gizli") && !text.contains("enc:") && !text.contains("apiKey") && !text.contains("sshPassword"), "{text}");
    assert_eq!(list[0]["hasSshCredentials"], true);
    assert_eq!(list[0]["metricsPort"], 9187, "admins see metrics");

    let (_, as_viewer) = api.call(Method::GET, &format!("/api/servers/{id}"), &api.viewer, None).await;
    assert_eq!(as_viewer["name"], "S");
    assert!(as_viewer.get("metricsPort").is_none(), "{as_viewer}");

    let events = api.db.audit_events(10).await.unwrap();
    assert_eq!(events[0].action, "connection.create");
    assert_eq!(events[0].connection_id, Some(id));
}

#[tokio::test]
async fn only_admins_change_connections() {
    let api = api().await;
    let body = json!({"name": "S", "host": "h", "apiKey": "k"});
    assert_eq!(api.call(Method::POST, "/api/servers", &api.viewer, Some(body)).await.0, StatusCode::FORBIDDEN);
    let id = api.add_server().await;
    assert_eq!(api.call(Method::PUT, &format!("/api/servers/{id}"), &api.viewer, Some(json!({"name": "x"}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(api.call(Method::DELETE, &format!("/api/servers/{id}"), &api.viewer, None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bad_addresses_are_refused() {
    let api = api().await;
    for (body, problem) in [
        (json!({"name": "S", "apiKey": "k"}), "Name, host, and API key are required"),
        (json!({"name": "S", "host": "http://x", "apiKey": "k"}), "Host must be a hostname or IP address without URL path or credentials"),
        (json!({"name": "S", "host": "169.254.169.254", "apiKey": "k"}), "Host is not allowed"),
        (json!({"name": "S", "host": "h", "apiKey": "k", "webqueryPort": 70000}), "Port must be an integer between 1 and 65535"),
    ] {
        let (status, answer) = api.call(Method::POST, "/api/servers", &api.admin, Some(body)).await;
        assert_eq!((status, answer["error"].as_str()), (StatusCode::BAD_REQUEST, Some(problem)));
    }
}

#[tokio::test]
async fn editing_keeps_secrets_left_empty_and_notes_changed_ones() {
    let api = api().await;
    let id = api.add_server().await;
    let before = api.db.server(id).await.unwrap().unwrap();
    let path = format!("/api/servers/{id}");
    let (status, answer) = api.call(Method::PUT, &path, &api.admin, Some(json!({"name": "Yeni", "apiKey": "", "sshPassword": "", "sshUsername": ""}))).await;
    assert_eq!((status, answer["name"].as_str()), (StatusCode::OK, Some("Yeni")));
    let after = api.db.server(id).await.unwrap().unwrap();
    assert_eq!(after.api_key, before.api_key);
    assert_eq!(after.ssh_password, before.ssh_password);
    assert_eq!(after.ssh_username.as_deref(), Some("ssh"));
    assert_eq!(api.db.audit_events(1).await.unwrap()[0].action, "connection.update");

    api.call(Method::PUT, &path, &api.admin, Some(json!({"apiKey": "yeni-anahtar"}))).await;
    assert_ne!(api.db.server(id).await.unwrap().unwrap().api_key, before.api_key);
    let actions = api.audit_actions().await;
    assert!(actions.iter().any(|(action, _)| action == "connection.credentials_changed"), "{actions:?}");
    assert_eq!(api.call(Method::PUT, "/api/servers/9999", &api.admin, Some(json!({}))).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_connection_ends_its_routes() {
    let api = api().await;
    let id = api.add_server().await;
    let channels = format!("/api/servers/{id}/vs/1/channels");
    assert_eq!(api.call(Method::GET, &channels, &api.admin, None).await.0, StatusCode::OK);
    assert_eq!(api.call(Method::DELETE, &format!("/api/servers/{id}"), &api.admin, None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(api.call(Method::GET, &channels, &api.admin, None).await.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(api.call(Method::DELETE, &format!("/api/servers/{id}"), &api.admin, None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn viewers_need_access_to_each_server() {
    let api = api().await;
    let id = api.add_server().await;
    let channels = format!("/api/servers/{id}/vs/1/channels");
    let (status, body) = api.call(Method::GET, &channels, &api.viewer, None).await;
    assert_eq!((status, body), (StatusCode::FORBIDDEN, json!({"error": "No access to this server"})));
    assert!(api.calls.lock().unwrap().is_empty(), "nothing reached TeamSpeak");
    sqlx::query(r#"INSERT INTO "UserServerAccess" ("userId", "serverConfigId") VALUES (?, ?)"#).bind(api.viewer_id).bind(id).execute(api.db.pool()).await.unwrap();
    assert_eq!(api.call(Method::GET, &channels, &api.viewer, None).await.0, StatusCode::OK);
    // Reading is allowed; changing is still not.
    assert_eq!(api.call(Method::POST, &channels, &api.viewer, Some(json!({"channel_name": "x"}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(api.call(Method::DELETE, &format!("{channels}/5"), &api.viewer, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(api.calls.lock().unwrap().len(), 1);
    assert!(api.audit_actions().await.iter().all(|(action, _)| action != "channel.create"));
}

#[tokio::test]
async fn channel_routes_reach_teamspeak_as_before() {
    let api = api().await;
    let id = api.add_server().await;
    let base = format!("/api/servers/{id}/vs/3/channels");

    let (status, body) = api.call(Method::GET, &base, &api.admin, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["path"], "/3/channellist");
    assert!(api.last_call().contains("-topic=") && api.last_call().contains("-secondsempty="), "{}", api.last_call());

    api.call(Method::GET, &format!("{base}/5"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/3/channelinfo?cid=5");

    let (status, body) = api.call(Method::POST, &base, &api.admin, Some(json!({"channel_name": "Oda"}))).await;
    assert_eq!((status, body[0]["cid"].as_str()), (StatusCode::CREATED, Some("77")));
    let created = api.db.audit_events(100).await.unwrap().into_iter().find(|event| event.action == "channel.create").unwrap();
    assert_eq!((created.target_id.as_deref(), created.outcome.as_str(), created.virtual_server_id), (Some("77"), "success", Some(3)));

    api.call(Method::PUT, &format!("{base}/5"), &api.admin, Some(json!({"channel_name": "Yeni", "cid": "999"}))).await;
    assert!(api.last_call().starts_with("/3/channeledit?cid=5&"), "the path decides the channel: {}", api.last_call());
    assert!(!api.last_call().contains("999"));

    api.call(Method::DELETE, &format!("{base}/5"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/3/channeldelete?cid=5&force=1");
    api.call(Method::DELETE, &format!("{base}/5?force=0"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/3/channeldelete?cid=5&force=0");

    api.call(Method::POST, &format!("{base}/5/move"), &api.admin, Some(json!({"cpid": 1}))).await;
    assert_eq!(api.last_call(), "/3/channelmove?cid=5&cpid=1");

    api.call(Method::GET, &format!("{base}/5/permissions"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/3/channelpermlist?cid=5&-permsid=");
}

#[tokio::test]
async fn a_refused_action_is_audited_without_its_text() {
    let api = api().await;
    let id = api.add_server().await;
    let (status, body) = api.call(Method::POST, &format!("/api/servers/{id}/vs/1/channels"), &api.admin, Some(json!({"channel_name": "refuse", "channel_password": "gizli-parola"}))).await;
    assert_eq!((status, body["reason"].as_str()), (StatusCode::FORBIDDEN, Some("ts_permission_denied")));
    let event = api.db.audit_events(100).await.unwrap().into_iter().find(|event| event.action == "channel.create").unwrap();
    assert_eq!((event.outcome.as_str(), event.result_code.as_deref()), ("failure", Some("ts_error")));
    assert!(event.completed_at.is_some());
    let stored = format!("{:?} {:?} {:?}", event.action, event.target_id, event.result_code);
    assert!(!stored.contains("gizli"));
}

#[tokio::test]
async fn virtual_server_routes() {
    let api = api().await;
    let id = api.add_server().await;
    let base = format!("/api/servers/{id}/virtual-servers");
    assert_eq!(api.call(Method::GET, &base, &api.admin, None).await.1[0]["path"], "/serverlist");
    assert_eq!(api.call(Method::GET, &format!("{base}/2/info"), &api.admin, None).await.1[0]["path"], "/2/serverinfo");
    api.call(Method::POST, &format!("{base}/2/start"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/serverstart?sid=2");
    api.call(Method::POST, &format!("{base}/2/stop"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/serverstop?sid=2");
    api.call(Method::GET, &format!("{base}/2/connection-info"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/2/serverrequestconnectioninfo");
    assert_eq!(api.call(Method::POST, &format!("{base}/2/stop"), &api.viewer, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(api.call(Method::GET, &format!("{base}/abc/info"), &api.admin, None).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn only_allowed_server_settings_are_sent() {
    let api = api().await;
    let id = api.add_server().await;
    let (status, _) = api
        .call(Method::PUT, &format!("/api/servers/{id}/virtual-servers/1"), &api.admin, Some(json!({"virtualserver_name": "Ad", "virtualserver_port": 1, "serveradmin_password": "x"})))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(api.last_call(), "/1/serveredit?virtualserver_name=Ad");
}

#[tokio::test]
async fn client_actions_reach_teamspeak_and_hide_their_text() {
    let api = api().await;
    let id = api.add_server().await;
    let base = format!("/api/servers/{id}/vs/1/clients");

    api.call(Method::GET, &base, &api.admin, None).await;
    assert!(api.last_call().contains("-ip="), "admins see addresses");
    let viewer_access = sqlx::query(r#"INSERT INTO "UserServerAccess" ("userId", "serverConfigId") VALUES (?, ?)"#).bind(api.viewer_id).bind(id).execute(api.db.pool()).await;
    viewer_access.unwrap();
    api.call(Method::GET, &base, &api.viewer, None).await;
    assert!(!api.last_call().contains("-ip="), "viewers do not: {}", api.last_call());

    assert_eq!(api.call(Method::POST, &format!("{base}/9/kick"), &api.viewer, Some(json!({}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(api.call(Method::GET, &format!("{base}/database"), &api.viewer, None).await.0, StatusCode::FORBIDDEN);

    api.call(Method::POST, &format!("{base}/9/kick"), &api.admin, Some(json!({"reasonmsg": "gizli-sebep"}))).await;
    assert_eq!(api.last_call(), "/1/clientkick?clid=9&reasonid=5&reasonmsg=gizli-sebep");
    // No body at all still works, as it did.
    assert_eq!(api.call(Method::POST, &format!("{base}/9/ban"), &api.admin, None).await.0, StatusCode::OK);
    assert_eq!(api.last_call(), "/1/banclient?clid=9&time=0");
    api.call(Method::POST, &format!("{base}/9/move"), &api.admin, Some(json!({"cid": 3}))).await;
    assert_eq!(api.last_call(), "/1/clientmove?clid=9&cid=3");
    api.call(Method::POST, &format!("{base}/9/poke"), &api.admin, Some(json!({"msg": "selam"}))).await;
    assert_eq!(api.last_call(), "/1/clientpoke?clid=9&msg=selam");
    api.call(Method::POST, &format!("{base}/9/message"), &api.admin, Some(json!({"msg": "selam"}))).await;
    assert_eq!(api.last_call(), "/1/sendtextmessage?targetmode=1&target=9&msg=selam");
    api.call(Method::GET, &format!("{base}/database?start=5"), &api.admin, None).await;
    assert_eq!(api.last_call(), "/1/clientdblist?start=5&duration=100");

    let events = api.db.audit_events(100).await.unwrap();
    let kick = events.iter().find(|event| event.action == "client.kick").unwrap();
    assert_eq!((kick.target_id.as_deref(), kick.outcome.as_str()), (Some("9"), "success"));
}

#[tokio::test]
async fn groups_bans_and_the_rest_reach_teamspeak() {
    let api = api().await;
    let id = api.add_server().await;
    let vs = format!("/api/servers/{id}/vs/1");
    for (method, path, body, expected) in [
        (Method::GET, "server-groups", None, "/1/servergrouplist"),
        (Method::DELETE, "server-groups/6", None, "/1/servergroupdel?sgid=6&force=1"),
        (Method::PUT, "server-groups/6", Some(json!({"name": "A"})), "/1/servergrouprename?sgid=6&name=A"),
        (Method::POST, "server-groups/6/copy", Some(json!({"tsgid": 0})), "/1/servergroupcopy?ssgid=6&tsgid=0"),
        (Method::GET, "server-groups/6/members", None, "/1/servergroupclientlist?sgid=6&-names="),
        (Method::POST, "server-groups/6/members", Some(json!({"cldbid": 4, "x": 1})), "/1/servergroupaddclient?sgid=6&cldbid=4"),
        (Method::DELETE, "server-groups/6/members/4", None, "/1/servergroupdelclient?sgid=6&cldbid=4"),
        (Method::GET, "server-groups/6/permissions", None, "/1/servergrouppermlist?sgid=6&-permsid="),
        (Method::DELETE, "channel-groups/8", None, "/1/channelgroupdel?cgid=8&force=1"),
        (Method::GET, "channel-groups/8/clients", None, "/1/channelgroupclientlist?cgid=8"),
        (Method::POST, "channel-groups/8/assign", Some(json!({"cldbid": 4, "cid": 2})), "/1/setclientchannelgroup?cgid=8&cid=2&cldbid=4"),
        (Method::GET, "permissions/find?permid=5", None, "/1/permfind?permid=5"),
        (Method::GET, "permissions/overview/4", None, "/1/permoverview?cldbid=4&cid=0&permid=0"),
        (Method::GET, "bans", None, "/1/banlist"),
        (Method::DELETE, "bans/7", None, "/1/bandel?banid=7"),
        (Method::DELETE, "bans", None, "/1/bandelall"),
        (Method::GET, "complaints", None, "/1/complainlist"),
        (Method::DELETE, "complaints/1/2", None, "/1/complaindel?tcldbid=1&fcldbid=2"),
        (Method::GET, "messages", None, "/1/messagelist"),
        (Method::GET, "messages/3", None, "/1/messageget?msgid=3"),
        (Method::DELETE, "messages/3", None, "/1/messagedel?msgid=3"),
    ] {
        let (status, answer) = api.call(method.clone(), &format!("{vs}/{path}"), &api.admin, body).await;
        assert!(status.is_success(), "{method} {path}: {status} {answer}");
        assert_eq!(api.last_call(), expected, "{method} {path}");
    }
    let instance = format!("/api/servers/{id}/instance");
    assert_eq!(api.call(Method::GET, &instance, &api.admin, None).await.1[0]["path"], "/instanceinfo");
    api.call(Method::PUT, &instance, &api.admin, Some(json!({"serverinstance_filetransfer_port": 30033, "serverinstance_other": 1}))).await;
    assert_eq!(api.last_call(), "/instanceedit?serverinstance_filetransfer_port=30033");
    assert_eq!(api.call(Method::PUT, &instance, &api.viewer, Some(json!({}))).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn privilege_keys_are_checked_and_stay_out_of_the_trail() {
    let api = api().await;
    let id = api.add_server().await;
    let base = format!("/api/servers/{id}/vs/1/tokens");
    for bad in [json!({"tokentype": 2, "tokenid1": 1}), json!({"tokentype": 0, "tokenid1": 0}), json!({"tokentype": 1, "tokenid1": 1, "tokenid2": 0})] {
        assert_eq!(api.call(Method::POST, &base, &api.admin, Some(bad)).await.0, StatusCode::BAD_REQUEST);
    }
    assert_eq!(api.call(Method::POST, &base, &api.admin, Some(json!({"tokentype": 0, "tokenid1": 6}))).await.0, StatusCode::CREATED);
    assert_eq!(api.last_call(), "/1/privilegekeyadd?tokentype=0&tokenid1=6&tokenid2=0&tokendescription=");
    api.call(Method::DELETE, &format!("{base}/gizli-anahtar-degeri"), &api.admin, None).await;
    let events = api.db.audit_events(100).await.unwrap();
    let delete = events.iter().find(|event| event.action == "privilege_key.delete").unwrap();
    assert_eq!(delete.target_id, None, "the key is a secret");
    assert_eq!(api.call(Method::GET, &base, &api.viewer, None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn messages_are_checked_before_sending() {
    let api = api().await;
    let id = api.add_server().await;
    let base = format!("/api/servers/{id}/vs/1/messages");
    let uid = "abcdefghijklmnop=";
    for (body, problem) in [
        (json!({"mode": "x", "message": "m"}), "Invalid message delivery mode"),
        (json!({"message": "  "}), "Message must contain between 1 and 8192 characters"),
        (json!({"mode": "chat", "message": "m"}), "Select an online recipient"),
        (json!({"message": "m", "expectedUid": "short"}), "Invalid recipient UID"),
        (json!({"message": "m", "cluid": "short", "subject": "s"}), "A valid recipient UID and subject are required"),
    ] {
        let (status, answer) = api.call(Method::POST, &base, &api.admin, Some(body)).await;
        assert_eq!((status, answer["error"].as_str()), (StatusCode::BAD_REQUEST, Some(problem)));
    }
    let (status, answer) = api.call(Method::POST, &base, &api.admin, Some(json!({"message": "m", "cluid": uid, "subject": "s"}))).await;
    assert_eq!((status, answer), (StatusCode::CREATED, json!({"accepted": true, "mode": "offline"})));
    assert!(api.last_call().starts_with("/1/messageadd?"));
    let (status, _) = api.call(Method::POST, &base, &api.admin, Some(json!({"mode": "chat", "message": "m", "clid": 4, "expectedUid": uid}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "the fake client has another identity");
    assert_eq!(api.last_call(), "/1/clientinfo?clid=4", "nothing was sent");
    let (status, _) = api.call(Method::POST, &base, &api.admin, Some(json!({"mode": "chat", "message": "m", "clid": 4}))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(api.last_call(), "/1/sendtextmessage?targetmode=1&target=4&msg=m");
}
