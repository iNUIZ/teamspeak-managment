//! The WebQuery client. One task sends one request at a time: TeamSpeak's flood protection trips
//! when commands fire back to back, and a second connection would show up as a second query client.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::host::{web_query_path, Endpoint};
use crate::{Error, Result};

/// Minimum gap between commands.
pub const MIN_COMMAND_GAP: Duration = Duration::from_millis(350);
const FLOOD_BASE_PAUSE: Duration = Duration::from_secs(60);
const FLOOD_MAX_PAUSE: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const RETRY_DELAY: Duration = Duration::from_millis(150);
const UNAVAILABLE_RETRY_SECONDS: u64 = 5;

/// Interactive traffic jumps ahead of background bots and animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    High,
    Normal,
    Low,
}

pub type QueryParams = Vec<(String, String)>;

struct Job {
    post: bool,
    sid: u32,
    command: String,
    params: QueryParams,
    reply: oneshot::Sender<Result<Value>>,
}

struct Queued {
    rank: Reverse<(Priority, u64)>,
    job: Job,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.rank == other.rank
    }
}
impl Eq for Queued {}
impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Queued {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank.cmp(&other.rank)
    }
}

#[derive(Default)]
struct Flood {
    pause_until: Option<Instant>,
    strikes: u32,
}

impl Flood {
    fn remaining(&self, now: Instant) -> Duration {
        self.pause_until.map_or(Duration::ZERO, |until| until.saturating_duration_since(now))
    }

    /// Wait a minute, doubling with each strike up to five.
    fn strike(&mut self, now: Instant) -> Duration {
        self.strikes += 1;
        let pause = (FLOOD_BASE_PAUSE * 2u32.pow((self.strikes - 1).min(3))).min(FLOOD_MAX_PAUSE);
        self.pause_until = Some(now + pause);
        pause
    }

    fn clear(&mut self) {
        self.pause_until = None;
        self.strikes = 0;
    }
}

fn seconds_up(duration: Duration) -> u64 {
    duration.as_secs() + u64::from(duration.subsec_nanos() > 0)
}

/// TeamSpeak's anti-spam answer, commonly 524, 3329 or 3331.
pub fn is_flood(code: i64, message: &str) -> bool {
    matches!(code, 524 | 3329 | 3331) || message.to_lowercase().contains("flood")
}

/// `response.data.status` when present, else the whole document.
fn read_response(status: reqwest::StatusCode, bytes: &[u8]) -> Result<Value> {
    let data: Option<Value> = serde_json::from_slice(bytes).ok();
    let Some(data) = data else {
        return Err(Error::Api { code: -1, message: format!("Request failed with status code {}", status.as_u16()) });
    };
    let code = data["status"]["code"].as_i64();
    if let Some(code) = code.filter(|code| *code != 0) {
        let message = data["status"]["message"].as_str().unwrap_or_default().to_owned();
        return Err(Error::Api { code, message });
    }
    if !status.is_success() && code.is_none() {
        return Err(Error::Api { code: -1, message: format!("Request failed with status code {}", status.as_u16()) });
    }
    // An empty body is still what the old program returned: the whole document.
    let body = &data["body"];
    let truthy = match body {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(text) => !text.is_empty(),
        Value::Number(number) => number.as_f64() != Some(0.0),
        Value::Array(_) | Value::Object(_) => true,
    };
    Ok(if truthy { body.clone() } else { data })
}

fn is_transient(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

fn build_http(api_key: &str, allow_self_signed: bool) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    let mut key = reqwest::header::HeaderValue::from_str(api_key).map_err(|_| Error::Invalid("Invalid API key".to_owned()))?;
    key.set_sensitive(true);
    headers.insert("x-api-key", key);
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(REQUEST_TIMEOUT)
        // Stay on the checked TeamSpeak address: never follow a redirect off it.
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(1)
        .danger_accept_invalid_certs(allow_self_signed)
        .build()
        .map_err(|error| Error::Invalid(format!("Could not create the query client: {error}")))
}

async fn send(http: &reqwest::Client, origin: &str, job: &Job) -> std::result::Result<Result<Value>, reqwest::Error> {
    let path = match web_query_path(job.sid, &job.command) {
        Ok(path) => path,
        Err(error) => return Ok(Err(error)),
    };
    let url = format!("{origin}{path}");
    let builder = if job.post { http.post(url) } else { http.get(url) };
    let response = builder.query(&job.params).send().await?;
    let status = response.status();
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Ok(Err(Error::Api { code: -1, message: "The response is too large".to_owned() }));
        }
    }
    Ok(read_response(status, &bytes))
}

struct Pump {
    http: reqwest::Client,
    api_key: String,
    allow_self_signed: bool,
    origin: String,
    flood: Arc<Mutex<Flood>>,
    gap: Duration,
}

impl Pump {
    /// One retry on a stale connection, on a new client so a hostname is resolved again (a
    /// recreated container has a new address). A flood answer is never retried.
    async fn run_job(&mut self, job: &Job) -> Result<Value> {
        match send(&self.http, &self.origin, job).await {
            Ok(result) => result,
            Err(error) if is_transient(&error) => {
                if let Ok(http) = build_http(&self.api_key, self.allow_self_signed) {
                    self.http = http;
                }
                tokio::time::sleep(RETRY_DELAY).await;
                match send(&self.http, &self.origin, job).await {
                    Ok(result) => result,
                    Err(error) if is_transient(&error) => Err(Error::Unavailable { retry_after_seconds: UNAVAILABLE_RETRY_SECONDS }),
                    Err(error) => Err(Error::Api { code: -1, message: error.without_url().to_string() }),
                }
            }
            Err(error) => Err(Error::Api { code: -1, message: error.without_url().to_string() }),
        }
    }

    async fn run(mut self, mut incoming: mpsc::UnboundedReceiver<(Priority, Job)>) {
        let mut queue = BinaryHeap::new();
        let mut sequence = 0u64;
        let mut next_allowed = tokio::time::Instant::now();
        loop {
            // Everything waiting decides the order, not only what arrived first.
            while let Ok((priority, job)) = incoming.try_recv() {
                sequence += 1;
                queue.push(Queued { rank: Reverse((priority, sequence)), job });
            }
            // No backlog during a cooldown: callers can show a useful state now, and background
            // jobs back off instead of stampeding the server later.
            let remaining = self.flood.lock().expect("flood lock").remaining(Instant::now());
            if !remaining.is_zero() {
                let retry_after_seconds = seconds_up(remaining).max(1);
                for waiting in queue.drain() {
                    let _ = waiting.job.reply.send(Err(Error::Flood { retry_after_seconds }));
                }
            }
            let now = tokio::time::Instant::now();
            let due = next_allowed.max(now);
            if queue.is_empty() || due > now {
                // Something more urgent may arrive while waiting for the gap.
                tokio::select! {
                    () = tokio::time::sleep_until(due), if !queue.is_empty() => {}
                    received = incoming.recv() => match received {
                        Some((priority, job)) => {
                            sequence += 1;
                            queue.push(Queued { rank: Reverse((priority, sequence)), job });
                        }
                        None => return,
                    },
                }
                continue;
            }
            let Some(next) = queue.pop() else { continue };
            self.handle(next.job).await;
            next_allowed = tokio::time::Instant::now() + self.gap;
        }
    }

    async fn handle(&mut self, job: Job) {
        let result = self.run_job(&job).await;
        let flooded = matches!(&result, Err(Error::Api { code, message }) if is_flood(*code, message));
        if flooded {
            let pause = self.flood.lock().expect("flood lock").strike(Instant::now());
            tracing::warn!(pause_ms = pause.as_millis() as u64, "TeamSpeak flood protection: pausing query traffic");
            let _ = job.reply.send(Err(Error::Flood { retry_after_seconds: seconds_up(pause) }));
            return;
        }
        if result.is_ok() {
            self.flood.lock().expect("flood lock").clear();
        }
        let _ = job.reply.send(result);
    }
}

/// A TeamSpeak server's WebQuery. Dropping it ends the task behind it.
pub struct WebQueryClient {
    jobs: mpsc::UnboundedSender<(Priority, Job)>,
    flood: Arc<Mutex<Flood>>,
}

impl WebQueryClient {
    pub fn new(endpoint: &Endpoint, api_key: &str, allow_self_signed: bool) -> Result<Self> {
        Self::with_gap(endpoint, api_key, allow_self_signed, MIN_COMMAND_GAP)
    }

    fn with_gap(endpoint: &Endpoint, api_key: &str, allow_self_signed: bool, gap: Duration) -> Result<Self> {
        let flood = Arc::new(Mutex::new(Flood::default()));
        let (jobs, incoming) = mpsc::unbounded_channel();
        let pump = Pump {
            http: build_http(api_key, allow_self_signed)?,
            api_key: api_key.to_owned(),
            allow_self_signed,
            origin: endpoint.origin.clone(),
            flood: flood.clone(),
            gap,
        };
        tokio::spawn(pump.run(incoming));
        Ok(Self { jobs, flood })
    }

    /// How long query traffic is still paused.
    pub fn flood_cooldown(&self) -> Duration {
        self.flood.lock().expect("flood lock").remaining(Instant::now())
    }

    async fn enqueue(&self, post: bool, sid: u32, command: &str, params: QueryParams, priority: Priority) -> Result<Value> {
        let remaining = self.flood_cooldown();
        if !remaining.is_zero() {
            return Err(Error::Flood { retry_after_seconds: seconds_up(remaining).max(1) });
        }
        let (reply, answer) = oneshot::channel();
        let job = Job { post, sid, command: command.to_owned(), params, reply };
        self.jobs.send((priority, job)).map_err(|_| Error::Unavailable { retry_after_seconds: UNAVAILABLE_RETRY_SECONDS })?;
        answer.await.map_err(|_| Error::Unavailable { retry_after_seconds: UNAVAILABLE_RETRY_SECONDS })?
    }

    pub async fn execute(&self, sid: u32, command: &str, params: QueryParams, priority: Priority) -> Result<Value> {
        self.enqueue(false, sid, command, params, priority).await
    }

    pub async fn execute_post(&self, sid: u32, command: &str, params: QueryParams, priority: Priority) -> Result<Value> {
        self.enqueue(true, sid, command, params, priority).await
    }

    /// Read the version, to prove the address and key work. Flood and "still starting" stay
    /// errors, so they reach the user as such and not as a failed connection.
    pub async fn test_connection(&self) -> Result<std::result::Result<Value, String>> {
        match self.execute(0, "version", Vec::new(), Priority::High).await {
            Ok(version) => Ok(Ok(version)),
            Err(error @ (Error::Flood { .. } | Error::Unavailable { .. })) => Err(error),
            Err(error) => Ok(Err(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::HostPolicy;
    use axum::extract::{Query, State};
    use axum::http::HeaderMap;
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::json;
    use std::collections::HashMap;

    #[derive(Clone, Default)]
    struct Fake {
        calls: Arc<Mutex<Vec<(String, Instant)>>>,
    }

    async fn answer(path: String, fake: Fake, params: HashMap<String, String>, headers: &HeaderMap) -> Json<Value> {
        let key = headers.get("x-api-key").and_then(|value| value.to_str().ok()).unwrap_or_default();
        fake.calls.lock().unwrap().push((path.clone(), Instant::now()));
        if key != "secret" {
            return Json(json!({"status":{"code":5122,"message":"invalid apikey"}}));
        }
        if path.ends_with("flood") {
            return Json(json!({"status":{"code":524,"message":"client is flooding"}}));
        }
        if path.ends_with("refused") {
            return Json(json!({"status":{"code":2568,"message":"insufficient client permissions"}}));
        }
        if path.ends_with("slow") {
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
        Json(json!({"body":[{"path":path,"a":params.get("a")}],"status":{"code":0,"message":"ok"}}))
    }

    async fn serve(fake: Fake) -> Endpoint {
        let handler = |State(fake): State<Fake>, uri: axum::http::Uri, Query(params): Query<HashMap<String, String>>, headers: HeaderMap| async move {
            answer(uri.path().to_owned(), fake, params, &headers).await
        };
        let app = Router::new().fallback(get(handler).post(handler)).with_state(fake);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Endpoint::new("127.0.0.1", Some(i64::from(port)), false, 10080, HostPolicy::default()).unwrap()
    }

    fn client(endpoint: &Endpoint, gap_ms: u64) -> WebQueryClient {
        WebQueryClient::with_gap(endpoint, "secret", false, Duration::from_millis(gap_ms)).unwrap()
    }

    #[tokio::test]
    async fn a_command_reaches_the_right_path_with_its_key_and_params() {
        let endpoint = serve(Fake::default()).await;
        let body = client(&endpoint, 1).execute(3, "clientlist", vec![("a".to_owned(), "1".to_owned())], Priority::Normal).await.unwrap();
        assert_eq!(body, json!([{"path":"/3/clientlist","a":"1"}]));
        let instance = client(&endpoint, 1).execute_post(0, "version", Vec::new(), Priority::High).await.unwrap();
        assert_eq!(instance[0]["path"], "/version");
    }

    #[tokio::test]
    async fn a_wrong_key_is_an_api_error() {
        let endpoint = serve(Fake::default()).await;
        let bad = WebQueryClient::new(&endpoint, "wrong", false).unwrap();
        let error = bad.execute(1, "x", Vec::new(), Priority::Normal).await.unwrap_err();
        assert_eq!(error, Error::Api { code: 5122, message: "invalid apikey".to_owned() });
        assert_eq!(bad.test_connection().await.unwrap(), Err("TeamSpeak API error 5122: invalid apikey".to_owned()));
    }

    #[tokio::test]
    async fn an_invalid_command_never_leaves() {
        let fake = Fake::default();
        let endpoint = serve(fake.clone()).await;
        assert!(matches!(client(&endpoint, 1).execute(1, "//evil", Vec::new(), Priority::Normal).await, Err(Error::Invalid(_))));
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn commands_are_spaced_out_and_never_overlap() {
        let fake = Fake::default();
        let endpoint = serve(fake.clone()).await;
        let client = Arc::new(client(&endpoint, 40));
        let mut waiting = Vec::new();
        for _ in 0..4 {
            let client = client.clone();
            waiting.push(tokio::spawn(async move { client.execute(1, "x", Vec::new(), Priority::Normal).await }));
        }
        for job in waiting {
            job.await.unwrap().unwrap();
        }
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        for pair in calls.windows(2) {
            assert!(pair[1].1 - pair[0].1 >= Duration::from_millis(35), "{:?}", pair[1].1 - pair[0].1);
        }
    }

    #[tokio::test]
    async fn urgent_commands_jump_the_queue() {
        let fake = Fake::default();
        let endpoint = serve(fake.clone()).await;
        let client = Arc::new(client(&endpoint, 20));
        let first = {
            let client = client.clone();
            tokio::spawn(async move { client.execute(1, "slow", Vec::new(), Priority::Normal).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut others = Vec::new();
        for (command, priority) in [("low", Priority::Low), ("normal", Priority::Normal), ("high", Priority::High)] {
            let client = client.clone();
            others.push(tokio::spawn(async move { client.execute(1, command, Vec::new(), priority).await }));
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        first.await.unwrap().unwrap();
        for job in others {
            job.await.unwrap().unwrap();
        }
        let order: Vec<String> = fake.calls.lock().unwrap().iter().map(|(path, _)| path.clone()).collect();
        assert_eq!(order, ["/1/slow", "/1/high", "/1/normal", "/1/low"]);
    }

    #[tokio::test]
    async fn a_flood_answer_pauses_everything_and_empties_the_queue() {
        let fake = Fake::default();
        let endpoint = serve(fake.clone()).await;
        let client = Arc::new(client(&endpoint, 30));
        let slow = {
            let client = client.clone();
            tokio::spawn(async move { client.execute(1, "flood", Vec::new(), Priority::Normal).await })
        };
        tokio::time::sleep(Duration::from_millis(5)).await;
        let queued = {
            let client = client.clone();
            tokio::spawn(async move { client.execute(1, "x", Vec::new(), Priority::Normal).await })
        };
        assert_eq!(slow.await.unwrap().unwrap_err(), Error::Flood { retry_after_seconds: 60 });
        // Refused at once, however it got in, and it never reached the server.
        let refused = tokio::time::timeout(Duration::from_secs(5), queued).await.expect("no waiting for the cooldown");
        assert!(matches!(refused.unwrap().unwrap_err(), Error::Flood { .. }));
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
        // Refused here, without another request to the server.
        let count = fake.calls.lock().unwrap().len();
        assert!(matches!(client.execute(1, "x", Vec::new(), Priority::High).await, Err(Error::Flood { .. })));
        assert_eq!(fake.calls.lock().unwrap().len(), count);
        assert!(client.flood_cooldown() > Duration::from_secs(50));
        assert!(matches!(client.test_connection().await, Err(Error::Flood { .. })));
    }

    #[test]
    fn repeated_floods_wait_longer_up_to_five_minutes() {
        let mut flood = Flood::default();
        let now = Instant::now();
        let pauses: Vec<u64> = (0..6).map(|_| flood.strike(now).as_secs()).collect();
        assert_eq!(pauses, [60, 120, 240, 300, 300, 300]);
        flood.clear();
        assert_eq!(flood.strike(now).as_secs(), 60);
    }

    #[test]
    fn floods_are_recognized() {
        assert!(is_flood(524, ""));
        assert!(is_flood(3331, ""));
        assert!(is_flood(1, "Client Is Flooding"));
        assert!(!is_flood(2568, "insufficient"));
    }

    #[tokio::test]
    async fn a_server_that_is_not_there_is_unavailable_not_a_failure() {
        // A port nothing listens on: bind, read it, close.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let endpoint = Endpoint::new("127.0.0.1", Some(i64::from(port)), false, 10080, HostPolicy::default()).unwrap();
        let client = client(&endpoint, 1);
        assert_eq!(client.execute(1, "x", Vec::new(), Priority::Normal).await.unwrap_err(), Error::Unavailable { retry_after_seconds: 5 });
        assert!(matches!(client.test_connection().await, Err(Error::Unavailable { .. })));
    }

    #[test]
    fn responses_are_read_like_the_old_client() {
        let ok = reqwest::StatusCode::OK;
        assert_eq!(read_response(ok, br#"{"body":[1],"status":{"code":0}}"#).unwrap(), json!([1]));
        assert_eq!(read_response(ok, br#"{"status":{"code":0,"message":"ok"}}"#).unwrap()["status"]["message"], "ok");
        assert!(matches!(read_response(reqwest::StatusCode::BAD_GATEWAY, b"<html>"), Err(Error::Api { code: -1, .. })));
        assert!(matches!(read_response(reqwest::StatusCode::BAD_REQUEST, br#"{"status":{"code":1538,"message":"m"}}"#), Err(Error::Api { code: 1538, .. })));
    }
}
