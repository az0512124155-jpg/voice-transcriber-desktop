//! Local HTTPS bridge between the desktop app and the Android lecture notebook.
//!
//! The server is OFF by default and only starts from an explicit settings
//! action. It never receives audio: the phone transcribes on-device and sends
//! text only. Pairing uses a 6-character, single-use code plus a TLS
//! certificate fingerprint that is displayed on the Mac and confirmed on the
//! phone. The bearer token is random 256-bit and the code exchange is rate
//! limited globally (not per source IP, which a LAN attacker can spoof).

use crate::settings::get_settings;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const ALPHABET: &[u8; 30] = b"ABCDEFGHJKLMNPQRSTUVWXYZ234567";
const CODE_TTL: Duration = Duration::from_secs(300);
const MAX_ATTEMPTS: u32 = 5;
const LOCK_TIME: Duration = Duration::from_secs(60);
pub const DEFAULT_PORT: u16 = 38381;
/// UDP port used only for LAN discovery beacons. No secrets travel on it.
pub const DISCOVERY_PORT: u16 = 38382;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PhoneLinkStatus {
    pub running: bool,
    pub port: u16,
    pub code: Option<String>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PairRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PairResponse {
    pub token: String,
    pub fingerprint: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AskRequest {
    pub question: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub materials: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AskResponse {
    pub answer: String,
    pub confidence: f64,
    pub abstain: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SummarizeRequest {
    pub transcript: String,
    #[serde(default)]
    pub materials: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SummarizeResponse {
    pub summary: String,
    pub key_points: Vec<String>,
    pub available: bool,
}

struct Shared {
    handle: Option<Handle>,
    beacon_stop: Option<Arc<AtomicBool>>,
    token: String,
    fingerprint: String,
    code: String,
    code_created: Instant,
    failures: u32,
    locked_until: Option<Instant>,
    port: u16,
}

fn shared() -> &'static Mutex<Option<Shared>> {
    static STATE: OnceLock<Mutex<Option<Shared>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("system RNG unavailable");
    bytes
}

fn make_code() -> String {
    let bytes = random_bytes(6);
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

/// Constant-time-ish comparison: visit every character and accumulate diffs.
fn code_matches(a: &str, b: &str) -> bool {
    let left = a.as_bytes();
    let right = b.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for index in 0..left.len() {
        diff |= left[index] ^ right[index];
    }
    diff == 0
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::json!({ "error": message }).to_string(),
    )
        .into_response()
}

fn authorized(headers: &axum::http::HeaderMap, token: &str) -> bool {
    let sent = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("Bearer {token}");
    code_matches(sent, &expected)
}

#[derive(Clone)]
struct ServerState {
    app: AppHandle,
}

/// Health endpoint. Intentionally public and deliberately free of sensitive
/// data: it lets the phone discover a running bridge before asking for a code.
async fn status() -> Response {
    Json(serde_json::json!({
        "ok": true,
        "app": "Voice Transcriber",
        "version": env!("CARGO_PKG_VERSION"),
    }))
    .into_response()
}

fn check_auth(headers: &axum::http::HeaderMap) -> Result<(), Response> {
    let token = shared()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.token.clone()))
        .ok_or_else(|| json_error(StatusCode::SERVICE_UNAVAILABLE, "bridge not running"))?;
    if authorized(headers, &token) {
        Ok(())
    } else {
        Err(json_error(StatusCode::UNAUTHORIZED, "invalid token"))
    }
}

async fn pair(State(_state): State<ServerState>, body: Json<PairRequest>) -> Response {
    {
        let mut guard = match shared().lock() {
            Ok(guard) => guard,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "state lock failed"),
        };
        let Some(current) = guard.as_mut() else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "bridge not running");
        };
        if let Some(until) = current.locked_until {
            if Instant::now() < until {
                return json_error(StatusCode::TOO_MANY_REQUESTS, "pairing temporarily locked");
            }
            current.locked_until = None;
            current.failures = 0;
        }
        if current.code_created.elapsed() > CODE_TTL {
            return json_error(StatusCode::GONE, "pairing code expired");
        }
        if !code_matches(&body.code, &current.code) {
            current.failures += 1;
            if current.failures >= MAX_ATTEMPTS {
                current.locked_until = Some(Instant::now() + LOCK_TIME);
                current.failures = 0;
            }
            return json_error(StatusCode::UNAUTHORIZED, "invalid pairing code");
        }
        // Single use: invalidate immediately after a successful exchange.
        current.code = String::new();
        current.code_created = Instant::now() - CODE_TTL;
        let _ = current;
    }
    let (token, fingerprint) = {
        let guard = shared().lock().ok();
        match guard.and_then(|g| g.as_ref().map(|s| (s.token.clone(), s.fingerprint.clone()))) {
            Some(values) => values,
            None => return json_error(StatusCode::SERVICE_UNAVAILABLE, "bridge not running"),
        }
    };
    Json(PairResponse {
        token,
        fingerprint,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    })
    .into_response()
}

fn llm_parts(app: &AppHandle) -> Result<(crate::settings::PostProcessProvider, String, String), Response> {
    let settings = get_settings(app);
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or_else(|| json_error(StatusCode::SERVICE_UNAVAILABLE, "no AI provider configured"))?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    if model.trim().is_empty() || key.trim().is_empty() {
        return Err(json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI provider has no model or API key configured",
        ));
    }
    Ok((provider, model, key))
}

fn strip_think(text: &str) -> String {
    let cleaned = text.trim();
    if let Some(start) = cleaned.find("<think>") {
        if let Some(end) = cleaned.find("</think>") {
            if end > start {
                return cleaned[end + 8..].trim().to_string();
            }
        }
    }
    cleaned.to_string()
}

async fn ask(State(state): State<ServerState>, headers: axum::http::HeaderMap, body: Json<AskRequest>) -> Response {
    if let Err(error) = check_auth(&headers) {
        return error;
    }
    let (provider, model, key) = match llm_parts(&state.app) {
        Ok(parts) => parts,
        Err(error) => return error,
    };
    let materials = body.materials.join("\n\n---\n\n");
    let prompt = format!(
        "You are helping a student during a live lesson. Answer ONLY from the course materials below and from the transcript context. If the answer is not present, set abstain=true and write a short Hebrew message telling the student to ask the instructor. Never guess, especially for medical or first-aid topics. Answer in Hebrew, concise, max 80 words.\n\nCOURSE MATERIALS:\n{materials}\n\nTRANSCRIPT CONTEXT:\n{}\n\nQUESTION: {}\n",
        body.context, body.question
    );
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" },
            "confidence": { "type": "number" },
            "abstain": { "type": "boolean" },
            "source": { "type": "string" }
        },
        "required": ["answer", "confidence", "abstain", "source"],
        "additionalProperties": false
    });
    match crate::llm_client::send_chat_completion_with_schema(
        &provider,
        key,
        &model,
        prompt,
        Some("Heuristic question answering for lecture notes. Cite the exact source when possible; abstain rather than speculate.".to_string()),
        Some(schema),
        true,
    )
    .await
    {
        Ok(Some(content)) => {
            let text = strip_think(&content);
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(json) => {
                    let abstain = json.get("abstain").and_then(|v| v.as_bool()).unwrap_or(false);
                    let answer = json.get("answer").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let source = json.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let confidence = json.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let answer = if abstain || answer.trim().is_empty() {
                        "לא מצאתי תשובה ודאית בחומר שלך — שאל את המדריך".to_string()
                    } else {
                        answer
                    };
                    Json(AskResponse {
                        answer,
                        confidence: confidence.clamp(0.0, 1.0),
                        abstain: abstain || confidence < 0.35,
                        source,
                    })
                    .into_response()
                }
                Err(_) => Json(AskResponse {
                    answer: "לא מצאתי תשובה ודאית בחומר שלך — שאל את המדריך".to_string(),
                    confidence: 0.0,
                    abstain: true,
                    source: String::new(),
                })
                .into_response(),
            }
        }
        Ok(None) => json_error(StatusCode::BAD_GATEWAY, "empty AI response"),
        Err(error) => json_error(StatusCode::BAD_GATEWAY, &format!("AI request failed: {error}")),
    }
}

async fn summarize(State(state): State<ServerState>, headers: axum::http::HeaderMap, body: Json<SummarizeRequest>) -> Response {
    if let Err(error) = check_auth(&headers) {
        return error;
    }
    let (provider, model, key) = match llm_parts(&state.app) {
        Ok(parts) => parts,
        Err(_error) => {
            return Json(SummarizeResponse {
                summary: "סיכום AI זמין רק כשהמחשב מחובר לספק AI מוגדר בהגדרות.".to_string(),
                key_points: Vec::new(),
                available: false,
            })
            .into_response();
        }
    };
    let materials = body.materials.join("\n\n---\n\n");
    let prompt = format!(
        "Summarize this Hebrew lesson for the student. Use the course materials as the primary source of truth. Return concise Hebrew summary and 3-8 key points. Do not invent facts.\n\nMATERIALS:\n{materials}\n\nTRANSCRIPT:\n{}\n",
        body.transcript
    );
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "key_points": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["summary", "key_points"],
        "additionalProperties": false
    });
    match crate::llm_client::send_chat_completion_with_schema(
        &provider,
        key,
        &model,
        prompt,
        None,
        Some(schema),
        true,
    )
    .await
    {
        Ok(Some(content)) => {
            let text = strip_think(&content);
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(json) => Json(SummarizeResponse {
                    summary: json.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    key_points: json
                        .get("key_points")
                        .and_then(|v| v.as_array())
                        .map(|values| values.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                        .unwrap_or_default(),
                    available: true,
                })
                .into_response(),
                Err(_) => json_error(StatusCode::BAD_GATEWAY, "invalid AI summary"),
            }
        }
        _ => json_error(StatusCode::BAD_GATEWAY, "AI summary failed"),
    }
}

fn make_router(app: AppHandle) -> Router {
    let state = ServerState { app };
    Router::new()
        .route("/status", get(status))
        .route("/pair", post(pair))
        .route("/ask", post(ask))
        .route("/summarize", post(summarize))
        .with_state(state)
}

/// Best-effort LAN IPv4. Connecting a UDP socket does not send a packet; it
/// only asks the routing table which local address would be used.
fn lan_ipv4() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket
        .local_addr()
        .ok()
        .map(|address| address.ip().to_string())
}

fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Voice Transcriber PC".to_string())
}

/// Advertises the running bridge so the phone can find it without the user
/// typing an IP address or a certificate fingerprint. The beacon contains no
/// code and no token — only what is needed to open a pinned TLS connection.
async fn beacon_loop(port: u16, fingerprint: String, stop: Arc<AtomicBool>) {
    let payload = serde_json::json!({
        "app": "Voice Transcriber",
        "v": env!("CARGO_PKG_VERSION"),
        "name": computer_name(),
        "ip": lan_ipv4().unwrap_or_else(|| "127.0.0.1".to_string()),
        "port": port,
        "fp": fingerprint,
    })
    .to_string();

    // Responder side: the phone broadcasts a short probe to DISCOVERY_PORT and
    // waits for a unicast reply on its own ephemeral port. This is more
    // reliable than the phone listening on a fixed broadcast port, because a
    // unicast reply always reaches the sender while a broadcast may be dropped
    // by the receiving Wi-Fi stack or blocked by AP isolation.
    let reply_payload = payload.clone();
    let reply_stop = stop.clone();
    tokio::spawn(async move {
        let listener = match tokio::net::UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await {
            Ok(socket) => socket,
            Err(error) => {
                log::debug!("phone link discovery listener unavailable: {error}");
                return;
            }
        };
        let mut buffer = [0u8; 512];
        while !reply_stop.load(Ordering::Relaxed) {
            let received = tokio::time::timeout(
                Duration::from_millis(500),
                listener.recv_from(&mut buffer),
            )
            .await;
            match received {
                Ok(Ok((_len, peer))) => {
                    let _ = listener.send_to(reply_payload.as_bytes(), peer).await;
                }
                Ok(Err(error)) => {
                    log::debug!("phone link discovery listener stopped: {error}");
                    break;
                }
                Err(_) => continue,
            }
        }
    });

    let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
        Ok(socket) => socket,
        Err(error) => {
            log::warn!("phone link discovery socket failed: {error}");
            return;
        }
    };
    if let Err(error) = socket.set_broadcast(true) {
        log::warn!("phone link discovery broadcast failed: {error}");
        return;
    }
    let target = format!("255.255.255.255:{DISCOVERY_PORT}");

    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = socket.send_to(payload.as_bytes(), &target).await {
            log::debug!("phone link discovery beacon failed: {error}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tauri::command]
#[specta::specta]
pub fn phone_link_start(app: AppHandle, port: Option<u16>) -> Result<PhoneLinkStatus, String> {
    stop_server_internal()?;

    let cert = rcgen::generate_simple_self_signed(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ])
    .map_err(|e| format!("certificate generation failed: {e}"))?;
    let cert_der = cert.cert.der().clone();
    let key_der = cert.key_pair.serialize_der();
    use sha2::{Digest, Sha256};
    let fingerprint = Sha256::digest(cert_der.as_ref())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let rustls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert_der.as_ref().to_vec())],
            rustls::pki_types::PrivateKeyDer::Pkcs8(key_der.into()),
        )
        .map_err(|e| format!("TLS configuration failed: {e}"))?;
    let tls = RustlsConfig::from_config(std::sync::Arc::new(rustls_config));

    let token = random_bytes(32)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let port = port.unwrap_or(DEFAULT_PORT);
    let address = format!("0.0.0.0:{port}");
    let listener = std::net::TcpListener::bind(&address)
        .map_err(|e| format!("could not bind {address}: {e}"))?;
    drop(listener);

    let handle = Handle::new();
    let router = make_router(app.clone());
    let run_handle = handle.clone();
    let beacon_stop = Arc::new(AtomicBool::new(false));
    let beacon_stop_task = beacon_stop.clone();
    let beacon_fingerprint = fingerprint.clone();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
        Err(_error) => {
            log::error!("phone link runtime failed: {_error}");
                return;
            }
        };
        runtime.block_on(async move {
            tokio::spawn(beacon_loop(port, beacon_fingerprint, beacon_stop_task));
            if let Err(error) = axum_server::from_tcp_rustls(
                std::net::TcpListener::bind(format!("0.0.0.0:{port}"))
                    .expect("port was already validated"),
                tls,
            )
            .handle(run_handle)
            .serve(router.into_make_service())
            .await
            {
                log::error!("phone link server stopped: {error}");
            }
        });
    });

    let mut guard = shared().lock().map_err(|_| "state lock failed".to_string())?;
    *guard = Some(Shared {
        handle: Some(handle),
        beacon_stop: Some(beacon_stop),
        token,
        fingerprint: fingerprint.clone(),
        code: make_code(),
        code_created: Instant::now(),
        failures: 0,
        locked_until: None,
        port,
    });
    let code = guard.as_ref().map(|s| s.code.clone()).unwrap_or_default();
    let _ = app.emit("phone-link-changed", ());
    Ok(PhoneLinkStatus {
        running: true,
        port,
        code: Some(code),
        fingerprint: Some(fingerprint),
    })
}

#[tauri::command]
#[specta::specta]
pub fn phone_link_stop() -> Result<PhoneLinkStatus, String> {
    stop_server_internal()?;
    Ok(PhoneLinkStatus {
        running: false,
        port: DEFAULT_PORT,
        code: None,
        fingerprint: None,
    })
}

fn stop_server_internal() -> Result<(), String> {
    let (handle, beacon_stop) = {
        let mut guard = shared().lock().map_err(|_| "state lock failed".to_string())?;
        guard
            .take()
            .map(|state| (state.handle, state.beacon_stop))
            .unwrap_or((None, None))
    };
    if let Some(stop) = beacon_stop {
        stop.store(true, Ordering::Relaxed);
    }
    if let Some(handle) = handle {
        handle.shutdown();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn phone_link_status() -> PhoneLinkStatus {
    match shared().lock() {
        Ok(guard) => match guard.as_ref() {
            Some(state) => PhoneLinkStatus {
                running: true,
                port: state.port,
                code: Some(state.code.clone()),
                fingerprint: Some(state.fingerprint.clone()),
            },
            None => PhoneLinkStatus {
                running: false,
                port: DEFAULT_PORT,
                code: None,
                fingerprint: None,
            },
        },
        Err(_) => PhoneLinkStatus {
            running: false,
            port: DEFAULT_PORT,
            code: None,
            fingerprint: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{code_matches, make_code, ALPHABET};

    #[test]
    fn code_uses_only_unambiguous_characters() {
        for _ in 0..100 {
            let code = make_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| ALPHABET.contains(&(c as u8))));
        }
    }

    #[test]
    fn comparison_rejects_wrong_codes() {
        assert!(code_matches("ABCDEF", "ABCDEF"));
        assert!(!code_matches("ABCDEF", "ABCDEG"));
        assert!(!code_matches("ABCDEF", "ABCDE"));
    }
}
