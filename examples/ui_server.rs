use std::{
    collections::HashMap,
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bip32::{DerivationPath, XPrv};
use bip39::Mnemonic;
use k256::ecdsa::SigningKey as K256SigningKey;
use lumera_sdk_rs::{CascadeConfig, CascadeSdk, RegisterTicketRequest};
use rand::{distributions::Alphanumeric, Rng};
use ripemd::Ripemd160;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::Builder;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

struct AppState {
    sdk: CascadeSdk,
    creator: String,
    mnemonic: String,
    latest: Arc<Mutex<LatestState>>,
    chain_id: String,
    auth_required: bool,
    sessions: Arc<Mutex<HashMap<String, SessionState>>>,
    challenges: Arc<Mutex<HashMap<String, ChallengeState>>>,
}

#[derive(Default)]
struct LatestState {
    action_id: Option<String>,
    upload_task_id: Option<String>,
    download_task_id: Option<String>,
}

#[derive(Clone)]
struct SessionState {
    expires_at: u64,
}

#[derive(Clone)]
struct ChallengeState {
    address: String,
    message: String,
    expires_at: u64,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

#[derive(Debug, Serialize)]
struct TaskResponse {
    task_id: String,
}

#[derive(Debug, Serialize)]
struct UploadWorkflowResponse {
    action_id: String,
    upload_task_id: String,
}

#[derive(Debug, Deserialize)]
struct DownloadWorkflowRequest {
    action_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthChallengeQuery {
    address: String,
}

#[derive(Debug, Serialize)]
struct AuthChallengeResponse {
    challenge_id: String,
    message: String,
    chain_id: String,
    expires_in_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct AuthVerifyRequest {
    challenge_id: String,
    address: String,
    signature: String,
    pubkey: String,
}

#[derive(Debug, Serialize)]
struct AuthVerifyResponse {
    token: String,
    address: String,
    expires_in_seconds: u64,
}

fn unix_now() -> Result<u64, ApiError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| ApiError {
            error: format!("clock error: {e}"),
        })
}

fn random_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn derive_signing_keys(
    mnemonic: &str,
) -> Result<(cosmrs::crypto::secp256k1::SigningKey, K256SigningKey), ApiError> {
    lumera_sdk_rs::keys::derive_signing_keys_from_mnemonic(mnemonic).map_err(|e| ApiError {
        error: e.to_string(),
    })
}

fn extract_state(v: &serde_json::Value) -> String {
    for key in ["status", "state", "task_status"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            return s.to_ascii_lowercase();
        }
    }
    if let Some(task) = v.get("task") {
        for key in ["status", "state", "task_status"] {
            if let Some(s) = task.get(key).and_then(|x| x.as_str()) {
                return s.to_ascii_lowercase();
            }
        }
    }
    v.to_string().to_ascii_lowercase()
}

fn normalize_bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if !state.auth_required {
        return Ok(());
    }

    let token = normalize_bearer(headers).ok_or_else(|| ApiError {
        error: "missing or invalid Authorization bearer token".into(),
    })?;
    let now = unix_now()?;

    let sessions = state.sessions.lock().await;
    let s = sessions.get(&token).ok_or_else(|| ApiError {
        error: "invalid session token".into(),
    })?;
    if s.expires_at <= now {
        return Err(ApiError {
            error: "session token expired".into(),
        });
    }

    Ok(())
}

async fn auth_challenge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuthChallengeQuery>,
) -> Result<Json<AuthChallengeResponse>, ApiError> {
    if query.address.trim().is_empty() {
        return Err(ApiError {
            error: "address is required".into(),
        });
    }

    let now = unix_now()?;
    let challenge_id = random_token(24);
    let message = format!(
        "lumera-sdk-rs-ui-auth:{}:{}:{}",
        query.address.trim(),
        state.chain_id,
        random_token(16)
    );

    {
        let mut challenges = state.challenges.lock().await;
        challenges.insert(
            challenge_id.clone(),
            ChallengeState {
                address: query.address.trim().to_string(),
                message: message.clone(),
                expires_at: now + 120,
            },
        );
    }

    Ok(Json(AuthChallengeResponse {
        challenge_id,
        message,
        chain_id: state.chain_id.clone(),
        expires_in_seconds: 120,
    }))
}

async fn auth_verify(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthVerifyRequest>,
) -> Result<Json<AuthVerifyResponse>, ApiError> {
    let now = unix_now()?;

    let challenge = {
        let mut challenges = state.challenges.lock().await;
        challenges
            .remove(&body.challenge_id)
            .ok_or_else(|| ApiError {
                error: "challenge not found or already used".into(),
            })?
    };

    if challenge.expires_at <= now {
        return Err(ApiError {
            error: "challenge expired".into(),
        });
    }
    if challenge.address != body.address.trim() {
        return Err(ApiError {
            error: "address mismatch for challenge".into(),
        });
    }

    let pubkey_bytes = B64.decode(body.pubkey.trim()).map_err(|e| ApiError {
        error: format!("invalid pubkey base64: {e}"),
    })?;
    if pubkey_bytes.is_empty() {
        return Err(ApiError {
            error: "empty pubkey".into(),
        });
    }

    // Verify that provided pubkey maps to provided lumera address (Cosmos format):
    // account = RIPEMD160(SHA256(pubkey_bytes))
    let sha = Sha256::digest(&pubkey_bytes);
    let ripe = Ripemd160::digest(sha);
    let account: &[u8] = ripe.as_ref();
    let account_id = cosmrs::AccountId::new("lumera", account).map_err(|e| ApiError {
        error: format!("failed to derive lumera address from pubkey: {e}"),
    })?;

    if account_id.to_string() != body.address.trim() {
        return Err(ApiError {
            error: "pubkey/address verification failed".into(),
        });
    }

    let sig = B64.decode(body.signature.trim()).map_err(|e| ApiError {
        error: format!("invalid signature base64: {e}"),
    })?;
    if sig.len() < 64 {
        return Err(ApiError {
            error: "invalid signature length".into(),
        });
    }

    if challenge.message.is_empty() {
        return Err(ApiError {
            error: "invalid challenge message".into(),
        });
    }

    let token = format!("lumera-ui-{}", random_token(40));
    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(
            token.clone(),
            SessionState {
                expires_at: now + 8 * 60 * 60,
            },
        );
    }

    Ok(Json(AuthVerifyResponse {
        token,
        address: body.address.trim().to_string(),
        expires_in_seconds: 8 * 60 * 60,
    }))
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "creator": state.creator,
        "snapi_base": state.sdk.snapi.base,
        "chain_id": state.chain_id,
        "auth_required": state.auth_required,
    }))
}

async fn latest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize(&state, &headers).await?;

    let latest = state.latest.lock().await;
    Ok(Json(json!({
        "action_id": latest.action_id,
        "upload_task_id": latest.upload_task_id,
        "download_task_id": latest.download_task_id,
    })))
}

async fn workflow_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<UploadWorkflowResponse>, ApiError> {
    authorize(&state, &headers).await?;

    let mut file_name = String::from("upload.bin");
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError {
        error: format!("invalid multipart body: {e}"),
    })? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if let Some(n) = field.file_name() {
                file_name = n.to_string();
            }
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ApiError {
                        error: format!("failed to read file: {e}"),
                    })?
                    .to_vec(),
            );
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError {
        error: "missing file".into(),
    })?;

    let tmp = Builder::new()
        .prefix("sdk-rs-ui-")
        .suffix(".bin")
        .tempfile()
        .map_err(|e| ApiError {
            error: format!("failed to create temp file: {e}"),
        })?;

    tokio::fs::write(tmp.path(), &file_bytes)
        .await
        .map_err(|e| ApiError {
            error: format!("failed writing temp file: {e}"),
        })?;

    let (chain_sk, arb_sk) = derive_signing_keys(&state.mnemonic)?;

    let exp_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ApiError {
            error: format!("clock error: {e}"),
        })?
        .as_secs()
        + 172800;

    let registered = state
        .sdk
        .register_ticket(
            &chain_sk,
            &arb_sk,
            &state.creator,
            tmp.path(),
            RegisterTicketRequest {
                file_name,
                is_public: false,
                expiration_time: exp_secs.to_string(),
            },
        )
        .await
        .map_err(|e| ApiError {
            error: format!("register ticket failed: {e}"),
        })?;

    let upload_task_id = state
        .sdk
        .upload_via_snapi(
            &registered.action_id,
            &registered.auth_signature,
            tmp.path(),
        )
        .await
        .map_err(|e| ApiError {
            error: format!("upload failed: {e}"),
        })?;

    {
        let mut latest = state.latest.lock().await;
        latest.action_id = Some(registered.action_id.clone());
        latest.upload_task_id = Some(upload_task_id.clone());
        latest.download_task_id = None;
    }

    Ok(Json(UploadWorkflowResponse {
        action_id: registered.action_id,
        upload_task_id,
    }))
}

async fn upload_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize(&state, &headers).await?;

    let status = state
        .sdk
        .snapi
        .upload_status(&task_id)
        .await
        .map_err(|e| ApiError {
            error: e.to_string(),
        })?;
    Ok(Json(status))
}

async fn workflow_download_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DownloadWorkflowRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    authorize(&state, &headers).await?;

    let action_id = if let Some(id) = body.action_id {
        if id.trim().is_empty() {
            return Err(ApiError {
                error: "action_id cannot be empty".into(),
            });
        }
        id
    } else {
        let latest = state.latest.lock().await;
        latest.action_id.clone().ok_or_else(|| ApiError {
            error: "no previous upload found; upload a file first".into(),
        })?
    };

    let (_, arb_sk) = derive_signing_keys(&state.mnemonic)?;

    let task_id = state
        .sdk
        .request_download(&action_id, &arb_sk)
        .await
        .map_err(|e| ApiError {
            error: format!("download request failed: {e}"),
        })?;

    {
        let mut latest = state.latest.lock().await;
        latest.action_id = Some(action_id);
        latest.download_task_id = Some(task_id.clone());
    }

    Ok(Json(TaskResponse { task_id }))
}

async fn download_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize(&state, &headers).await?;

    let status = state
        .sdk
        .snapi
        .download_status(&task_id)
        .await
        .map_err(|e| ApiError {
            error: e.to_string(),
        })?;
    Ok(Json(status))
}

async fn download_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers).await?;

    let bytes = state
        .sdk
        .snapi
        .download_file(&task_id)
        .await
        .map_err(|e| ApiError {
            error: e.to_string(),
        })?;

    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream"),
        (
            header::CONTENT_DISPOSITION,
            "attachment; filename=sdk-rs-download.bin",
        ),
    ];

    Ok((headers, bytes))
}

async fn upload_status_summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize(&state, &headers).await?;

    let status = state
        .sdk
        .snapi
        .upload_status(&task_id)
        .await
        .map_err(|e| ApiError {
            error: e.to_string(),
        })?;
    Ok(Json(json!({
        "task_id": task_id,
        "state": extract_state(&status),
        "raw": status,
    })))
}

async fn download_status_summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize(&state, &headers).await?;

    let status = state
        .sdk
        .snapi
        .download_status(&task_id)
        .await
        .map_err(|e| ApiError {
            error: e.to_string(),
        })?;
    Ok(Json(json!({
        "task_id": task_id,
        "state": extract_state(&status),
        "raw": status,
    })))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rest = std::env::var("LUMERA_REST").unwrap_or_else(|_| "http://127.0.0.1:1317".into());
    let rpc = std::env::var("LUMERA_RPC").unwrap_or_else(|_| "http://127.0.0.1:26657".into());
    let grpc = std::env::var("LUMERA_GRPC").unwrap_or_else(|_| "http://127.0.0.1:9090".into());
    let snapi = std::env::var("SNAPI_BASE").unwrap_or_else(|_| "http://127.0.0.1:8089".into());
    let chain_id = std::env::var("LUMERA_CHAIN_ID").unwrap_or_else(|_| "lumera-devnet".into());
    let creator = std::env::var("LUMERA_CREATOR")
        .unwrap_or_else(|_| "lumera158ulqepc5wnlx04eqqs7hkhr9rs2een275qkpp".into());
    let mnemonic = std::env::var("LUMERA_MNEMONIC").expect("LUMERA_MNEMONIC is required");

    let ui_port: u16 = std::env::var("SDK_RS_UI_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3002);

    let auth_required = std::env::var("SDK_RS_AUTH_REQUIRED")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    let cfg = CascadeConfig {
        chain: lumera_sdk_rs::chain::ChainConfig {
            chain_id: chain_id.clone(),
            grpc_endpoint: grpc,
            rpc_endpoint: rpc,
            rest_endpoint: rest,
            gas_price: "0.025ulume".into(),
        },
        snapi_base: snapi.clone(),
    };

    let state = Arc::new(AppState {
        sdk: CascadeSdk::new(cfg),
        creator,
        mnemonic,
        latest: Arc::new(Mutex::new(LatestState::default())),
        chain_id,
        auth_required,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        challenges: Arc::new(Mutex::new(HashMap::new())),
    });

    let api = Router::new()
        .route("/health", get(health))
        .route("/auth/challenge", get(auth_challenge))
        .route("/auth/verify", post(auth_verify))
        .route("/latest", get(latest))
        .route("/workflow/upload", post(workflow_upload))
        .route("/upload/{task_id}/status", get(upload_status))
        .route("/upload/{task_id}/summary", get(upload_status_summary))
        .route("/workflow/download", post(workflow_download_request))
        .route("/download/{task_id}/status", get(download_status))
        .route("/download/{task_id}/summary", get(download_status_summary))
        .route("/download/{task_id}/file", get(download_file))
        .with_state(state);

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("examples/ui"));

    let addr = SocketAddr::from(([127, 0, 0, 1], ui_port));
    println!(
        "sdk-rs UI running on http://{} (SNAPI_BASE={}, auth_required={})",
        addr, snapi, auth_required
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
