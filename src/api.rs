//! Local Web UI HTTP API (bound to 127.0.0.1 only).
//!
//! ## Security model
//!
//! * Every `/api/*` request must carry the per-process random token in the
//!   `x-polycopier-token` header (the token is appended to the URL the bot opens).
//! * `Host` must be `127.0.0.1:<port>` or `localhost:<port>` (anti DNS-rebinding).
//! * If an `Origin` header is present it must be the same local origin; no CORS
//!   headers are emitted, so cross-origin pages cannot read responses.
//! * The private key is write-only: it is never returned by any endpoint.

use crate::config::{BotConfig, TargetsConfig};
use crate::copy_ledger::CopyLedger;
use crate::models::{ActiveApiOrder, EvaluatedTrade, Position, QueuedOrder, TargetPosition};
use crate::risk::SharedRiskEngine;
use crate::state::BotState;
use axum::{
    extract::{Json, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Port the Web UI / API listens on.
pub const UI_PORT: u16 = 3000;
/// Header carrying the per-process API token.
pub const TOKEN_HEADER: &str = "x-polycopier-token";

// ── Token & request guard ─────────────────────────────────────────────────

/// Return the UI token: reuse one inherited across a self-restart, otherwise
/// generate 32 random bytes (hex).
pub fn load_or_generate_token() -> String {
    if let Ok(t) = std::env::var(crate::process::UI_TOKEN_ENV) {
        if t.len() >= 32 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            return t;
        }
    }
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("OS random number generator unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// URL to open in the browser (token passed as a query parameter).
pub fn dashboard_url(token: &str) -> String {
    format!("http://localhost:{UI_PORT}/?token={token}")
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Validate Host / Origin / token for an API request. Pure, for testability.
pub fn check_request(headers: &HeaderMap, token: &str) -> Result<(), StatusCode> {
    let allowed_hosts = [
        format!("127.0.0.1:{UI_PORT}"),
        format!("localhost:{UI_PORT}"),
    ];
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !allowed_hosts.iter().any(|h| h == host) {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = headers.get(axum::http::header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if !allowed_hosts
            .iter()
            .any(|h| origin == format!("http://{h}"))
        {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let supplied = headers
        .get(TOKEN_HEADER)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !constant_time_eq(supplied.as_bytes(), token.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

async fn guard(State(token): State<Arc<String>>, req: Request, next: Next) -> Response {
    match check_request(req.headers(), &token) {
        Ok(()) => next.run(req).await,
        Err(code) => (code, "Forbidden").into_response(),
    }
}

fn json_error(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

fn static_files() -> tower_http::services::ServeDir<tower_http::services::ServeFile> {
    use tower_http::services::{ServeDir, ServeFile};
    let root = std::path::PathBuf::from("web/dist");
    ServeDir::new(&root).fallback(ServeFile::new(root.join("index.html")))
}

fn schedule_restart(token: Arc<String>) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let err = crate::process::self_restart(Some(&token));
        tracing::error!("Restart failed: {err}");
        std::process::exit(1);
    });
}

// ── Setup router (secrets missing) ────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetupPayload {
    pub private_key: String,
    pub funder_address: String,
}

pub fn create_setup_router(token: Arc<String>) -> Router {
    Router::new()
        .route(
            "/api/state",
            get(|| async { Json(serde_json::json!({ "status": "setup_required" })) }),
        )
        .route("/api/setup", post(handle_setup))
        .route_layer(middleware::from_fn_with_state(token.clone(), guard))
        .with_state(token)
        .fallback_service(static_files())
}

async fn handle_setup(State(token): State<Arc<String>>, Json(p): Json<SetupPayload>) -> Response {
    if !crate::config::is_valid_private_key_format(&p.private_key) {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Invalid Private Key: MUST be exactly 64 hex characters (32 bytes) long.",
        );
    }
    if p.funder_address.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "Funder address is required.");
    }
    if let Err(e) = crate::config::write_secrets_env(&p.private_key, p.funder_address.trim()) {
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write .env: {e}"),
        );
    }
    if !std::path::Path::new(crate::config::CONFIG_TOML_PATH).exists() {
        let default_cfg = BotConfig {
            targets: TargetsConfig { wallets: vec![] },
            ..Default::default()
        };
        if let Err(e) = crate::config::write_toml(&default_cfg) {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write config.toml: {e}"),
            );
        }
    }
    schedule_restart(token);
    Json(serde_json::json!({ "success": true })).into_response()
}

// ── Operational router ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiState {
    pub bot_state: Arc<RwLock<BotState>>,
    pub copy_ledger: Arc<Mutex<CopyLedger>>,
    pub risk_engine: SharedRiskEngine,
    pub is_sim: bool,
    pub max_consecutive_losses: u32,
    pub token: Arc<String>,
}

#[derive(Serialize)]
pub struct RiskStatus {
    pub consecutive_losses: u32,
    pub max_consecutive_losses: u32,
    /// Seconds remaining in a loss cooldown, or `None` when not cooling down.
    pub cooldown_remaining_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct StateResponse {
    pub positions: HashMap<String, Position>,
    pub live_feed: Vec<EvaluatedTrade>,
    pub total_balance: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub target_positions: Vec<TargetPosition>,
    pub copies_executed: u32,
    pub trades_skipped: u32,
    pub copied_count: usize,
    pub next_scan_secs: u64,
    pub pending_orders: HashMap<String, QueuedOrder>,
    pub active_orders: Vec<ActiveApiOrder>,
    pub position_sources: HashMap<String, String>,
    pub is_sim: bool,
    pub risk: RiskStatus,
}

/// Source wallet for a token: the active ledger entry, else the most recent
/// entry (e.g. a position whose closing SELL is still pending).
fn source_for_token(ledger: &CopyLedger, token_id: &str) -> Option<String> {
    ledger
        .find_active_for_token(token_id)
        .or_else(|| ledger.entries.iter().rev().find(|e| e.token_id == token_id))
        .map(|e| e.source_wallet.clone())
}

async fn get_state(State(s): State<ApiState>) -> Json<StateResponse> {
    let risk = {
        let mut r = s.risk_engine.lock().await;
        RiskStatus {
            cooldown_remaining_secs: r.cooldown_remaining_secs(),
            consecutive_losses: r.consecutive_losses(),
            max_consecutive_losses: s.max_consecutive_losses,
        }
    };
    let guard = s.bot_state.read().await;
    let ledger = s.copy_ledger.lock().await;

    let position_sources = guard
        .positions
        .keys()
        .chain(guard.active_orders.iter().map(|o| &o.token_id))
        .filter_map(|t| source_for_token(&ledger, t).map(|w| (t.clone(), w)))
        .collect();

    Json(StateResponse {
        positions: guard.positions.clone(),
        live_feed: guard.live_feed.iter().cloned().collect(),
        total_balance: guard.total_balance,
        unrealized_pnl: guard.unrealized_pnl,
        realized_pnl: guard.realized_pnl,
        target_positions: guard.target_positions.clone(),
        copies_executed: guard.copies_executed,
        trades_skipped: guard.trades_skipped,
        copied_count: guard.copied_count,
        next_scan_secs: guard.next_scan_secs,
        pending_orders: guard.pending_orders.clone(),
        active_orders: guard.active_orders.clone(),
        position_sources,
        is_sim: s.is_sim,
        risk,
    })
}

async fn get_config() -> Response {
    let raw = match std::fs::read_to_string(crate::config::CONFIG_TOML_PATH) {
        Ok(r) => r,
        Err(e) => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("config.toml unreadable: {e}"),
            )
        }
    };
    match raw.parse::<toml::Value>() {
        Ok(v) => Json(v).into_response(),
        Err(e) => json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("config.toml parse error: {e}"),
        ),
    }
}

async fn post_config(Json(payload): Json<BotConfig>) -> Response {
    match crate::config::write_toml(&payload) {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// Public view of the secrets in `.env` — never includes the private key.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct EnvStatus {
    pub private_key_set: bool,
    /// Signer address derived from the private key, if it is set and valid.
    pub signer_address: Option<String>,
    pub funder_address: String,
}

/// Update payload. An empty / absent `private_key` keeps the current key.
#[derive(Deserialize)]
pub struct EnvUpdate {
    #[serde(default)]
    pub private_key: Option<String>,
    pub funder_address: String,
}

/// Derive the signer address for a private key (None if invalid).
pub fn derive_address(private_key: &str) -> Option<String> {
    use std::str::FromStr;
    let k = private_key.trim().trim_matches('"');
    if !crate::config::is_valid_private_key_format(k) {
        return None;
    }
    alloy::signers::local::PrivateKeySigner::from_str(k)
        .ok()
        .map(|s| s.address().to_checksum(None))
}

pub fn env_status(private_key: &str, funder_address: &str) -> EnvStatus {
    let signer_address = derive_address(private_key);
    EnvStatus {
        private_key_set: signer_address.is_some(),
        signer_address,
        funder_address: funder_address.to_string(),
    }
}

async fn get_env() -> Json<EnvStatus> {
    let _ = dotenvy::dotenv_override();
    let pk = std::env::var("PRIVATE_KEY").unwrap_or_default();
    let funder = std::env::var("FUNDER_ADDRESS").unwrap_or_default();
    Json(env_status(&pk, &funder))
}

async fn post_env(Json(p): Json<EnvUpdate>) -> Response {
    let _ = dotenvy::dotenv_override();
    let new_key = p.private_key.filter(|k| !k.trim().is_empty());
    let key = match new_key {
        Some(k) => k,
        None => std::env::var("PRIVATE_KEY").unwrap_or_default(),
    };
    if !crate::config::is_valid_private_key_format(&key) {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Invalid Private Key: MUST be exactly 64 hex characters (32 bytes) long.",
        );
    }
    match crate::config::write_secrets_env(&key, p.funder_address.trim()) {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn restart(State(s): State<ApiState>) -> Json<serde_json::Value> {
    schedule_restart(s.token.clone());
    Json(serde_json::json!({ "success": true }))
}

pub fn create_router(state: ApiState) -> Router {
    let token = state.token.clone();
    Router::new()
        .route("/api/state", get(get_state))
        .route("/api/config", get(get_config).post(post_config))
        .route("/api/env", get(get_env).post(post_env))
        .route("/api/action/restart", post(restart))
        .route_layer(middleware::from_fn_with_state(token, guard))
        .with_state(state)
        .fallback_service(static_files())
}
