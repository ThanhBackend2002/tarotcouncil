//! Tauri glue: state + commands. Bất biến giữ từ PlanCouncil: UI không chạm API key,
//! mọi lời gọi AI chạy trong Rust.

// Giữ từ PlanCouncil cho luận giải 2 pha (M1/M2): chưa được gọi ở M0.
#[allow(dead_code)]
mod budget;
mod config;
#[allow(dead_code)]
mod cost;
mod health;
// Đồng bộ thủ công với upstream (xem UPSTREAM.md) → không cắt API chưa dùng (tool calling, stream).
#[allow(dead_code)]
mod llm_client;
mod models;
#[allow(dead_code)] // M1: chuẩn hoá văn bản cho safety.rs, M2: tìm kiếm Journal
mod normalize;
mod roster;
mod secrets;
mod storage;
#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{Manager, State};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use config::{api_root, AppConfig, MASK_PREFIX};
use llm_client::{ChatMessage, Expect, LlmClient, LlmError};
use secrets::{SecretError, Secrets, Target};
use storage::{Storage, StorageError};

// ───────────────────────────── State ─────────────────────────────

pub struct AppState {
    /// `Arc` để lúc khởi động giữ write-lock (owned) trong lúc hydrate/migrate key ở nền.
    config: Arc<tokio::sync::RwLock<AppConfig>>,
    /// API key: keyring / RAM + trạng thái (xem `secrets.rs`).
    secrets: Arc<Secrets>,
    #[allow(dead_code)] // M1: lưu lượt đọc
    storage: Storage,
    http: reqwest::Client,
}

// ───────────────────────────── Errors ─────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Invalid(String),
    #[error("Tính năng chưa được triển khai")]
    NotImplemented,
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Llm(#[from] LlmError),
    /// `keyring_unavailable` / `keyring_locked` / storage / invalid.
    #[error(transparent)]
    Secret(#[from] SecretError),
    /// Key chế độ RAM đã mất sau khi khởi động lại (hoặc keyring khoá) → cần nhập lại.
    #[allow(dead_code)] // M1: kiểm tra trước khi luận giải
    #[error("Thiếu API key của {label} (chỉ lưu trong RAM hoặc keyring đang khoá) — vào Cài đặt để nhập lại")]
    SecretMissing { target: Target, label: String },
    #[error("lỗi nội bộ: {0}")]
    Internal(String),
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            kind: &'a str,
            message: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            target: Option<&'a Target>,
        }
        let target = match self {
            CommandError::SecretMissing { target, .. } => Some(target),
            _ => None,
        };
        let kind = match self {
            CommandError::Secret(e) => e.code(),
            CommandError::SecretMissing { .. } => "secret_missing",
            CommandError::Invalid(_) => "invalid",
            CommandError::NotImplemented => "not_implemented",
            CommandError::Storage(StorageError::NotFound(_)) => "not_found",
            CommandError::Storage(_) => "storage",
            CommandError::Llm(_) => "llm",
            CommandError::Internal(_) => "internal",
        };
        Wire { kind, message: self.to_string(), target }.serialize(s)
    }
}

type CmdResult<T> = Result<T, CommandError>;

/// Thao tác keyring / secrets là blocking (có thể hiện hộp thoại OS) → chạy ở `spawn_blocking`.
async fn secret_blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, SecretError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(Into::into)
}

// ───────────────────────────── Config ─────────────────────────────

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> CmdResult<AppConfig> {
    Ok(state.config.read().await.redacted())
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, config: AppConfig) -> CmdResult<AppConfig> {
    // Giữ write-lock suốt quá trình merge + persist → 2 lần save đồng thời không ghi đè lẫn nhau.
    let mut guard = state.config.write().await;
    let merged = config.merge_secrets(&guard);
    merged.validate().map_err(CommandError::Invalid)?;
    let secrets = state.secrets.clone();
    let old = guard.clone();
    let merged = secret_blocking(move || {
        let mut m = merged;
        secrets.persist(&mut m, &old)?;
        Ok(m)
    })
    .await?;
    *guard = merged;
    log::info!("config saved: {:?}", guard.providers); // Debug đã redact key
    Ok(guard.redacted())
}

/// Người dùng bấm "Đồng ý" ở `PrivacyNotice` (plan §3). Chỉ đổi được qua command này.
#[tauri::command]
async fn acknowledge_privacy(state: State<'_, AppState>) -> CmdResult<AppConfig> {
    let mut guard = state.config.write().await;
    let mut next = guard.clone();
    next.tarot.privacy_ack = true;
    let secrets = state.secrets.clone();
    let to_save = next.clone();
    secret_blocking(move || secrets.save_config(&to_save)).await?;
    *guard = next;
    Ok(guard.redacted())
}

/// Danh mục thầy bói mặc định (không chứa bí mật) — UI dùng để khôi phục persona / ngoại hình mẫu.
#[tauri::command]
fn default_roles() -> Vec<config::RoleDef> {
    roster::default_roles("", "", 0.7)
}

// ───────────────────────────── Đọc bài (M1) ─────────────────────────────

/// Stub cho M0 (plan §2 bước 2): luận giải được triển khai ở M1.
#[tauri::command(rename_all = "snake_case")]
async fn interpret_reading(reading_id: Uuid) -> CmdResult<()> {
    log::info!("interpret_reading {reading_id}: chưa triển khai (M0)");
    Err(CommandError::NotImplemented)
}

// ───────────────────────────── Provider ─────────────────────────────

#[derive(Serialize)]
struct ConnectionTestResult {
    agent: String,
    model: String,
    latency_ms: u128,
    reply: String,
}

/// Ping nhanh endpoint của 1 thầy bói (dùng ở màn Settings). Không retry để báo lỗi ngay.
#[tauri::command]
async fn test_connection(state: State<'_, AppState>, agent: String) -> CmdResult<ConnectionTestResult> {
    let cfg = state.config.read().await.clone();
    let mut ep = cfg.resolve(&agent).ok_or_else(|| {
        CommandError::Invalid(format!(
            "thầy bói '{agent}' không tồn tại hoặc chưa chọn provider (hãy lưu cấu hình trước)"
        ))
    })?;
    ep.max_tokens = Some(32);
    let llm = LlmClient::new(state.http.clone(), &cfg).with_limits(0, Duration::from_secs(30));
    let msgs = [
        ChatMessage::system("You are a health check endpoint."),
        ChatMessage::user("Reply with exactly: OK"),
    ];
    let t0 = Instant::now();
    let reply = llm.chat(&ep, &msgs, Expect::Text, &CancellationToken::new()).await?;
    Ok(ConnectionTestResult {
        agent,
        model: ep.model,
        latency_ms: t0.elapsed().as_millis(),
        reply: models::truncate_chars(reply.text.trim(), 100),
    })
}

#[derive(Serialize)]
struct ModelList {
    provider_id: String,
    models: Vec<String>,
    fetched_at: DateTime<Utc>,
    /// true nếu danh sách đã được lưu vào cache của provider trong config.
    cached: bool,
}

/// Lấy danh sách model của 1 provider qua `GET {base_url}/models`.
///
/// `api_key` None hoặc giá trị mask → dùng key đã lưu, CHỈ KHI base_url trùng với bản đã lưu
/// (không gửi key cũ tới host khác); `""` → gọi không có key.
#[tauri::command]
async fn list_models(
    state: State<'_, AppState>,
    provider_id: String,
    base_url: Option<String>,
    api_key: Option<String>,
) -> CmdResult<ModelList> {
    let stored = state.config.read().await.provider(&provider_id).cloned();
    let base = base_url
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| stored.as_ref().map(|p| p.base_url.trim().to_owned()))
        .ok_or_else(|| CommandError::Invalid("Provider chưa có base URL".into()))?;
    config::validate_url(&base).map_err(|e| CommandError::Invalid(format!("base_url không hợp lệ: {e}")))?;

    let same_base = stored.as_ref().is_some_and(|p| api_root(&p.base_url) == api_root(&base));
    let stored_key = stored.as_ref().and_then(|p| p.api_key.clone());
    let key = match api_key.map(|k| k.trim().to_owned()) {
        Some(k) if k.starts_with(MASK_PREFIX) => same_base.then(|| stored_key.clone()).flatten(),
        Some(k) if k.is_empty() => None,
        Some(k) => Some(k),
        None => same_base.then(|| stored_key.clone()).flatten(),
    };

    let models = llm_client::list_models(&state.http, &base, key.as_deref(), Duration::from_secs(20)).await?;
    let fetched_at = Utc::now();

    let mut cached = false;
    if same_base && key == stored_key {
        let mut guard = state.config.write().await;
        let still_same = guard
            .provider(&provider_id)
            .is_some_and(|p| api_root(&p.base_url) == api_root(&base) && p.api_key == key);
        if still_same {
            let mut next = guard.clone();
            next.set_models(&provider_id, models.clone(), fetched_at);
            let secrets = state.secrets.clone();
            let to_save = next.clone();
            match secret_blocking(move || secrets.save_config(&to_save)).await {
                Ok(()) => {
                    *guard = next;
                    cached = true;
                }
                Err(e) => log::warn!("không lưu được cache model của '{provider_id}': {e}"),
            }
        }
    }
    log::info!("list_models '{provider_id}': {} model (cached={cached})", models.len());
    Ok(ModelList { provider_id, models, fetched_at, cached })
}

/// Kiểm tra model của các thầy bói (None = tất cả). Mỗi cặp provider + model chỉ gọi thử 1 lần.
#[tauri::command]
async fn check_models(state: State<'_, AppState>, role_ids: Option<Vec<String>>) -> CmdResult<Vec<health::ModelHealth>> {
    let cfg = state.config.read().await.clone();
    let res = health::check_roles(&state.http, &cfg, role_ids.as_deref()).await;
    let bad = res.iter().filter(|h| h.status != health::HealthStatus::Ok).count();
    log::info!("check_models: {} cặp provider/model, {bad} lỗi", res.len());
    Ok(res)
}

// ───────────────────────────── API key (secrets) ─────────────────────────────

#[derive(Serialize)]
struct SetSecretResult {
    storage: config::KeyStorage,
    /// Giá trị mask mới (`••••abcd`) để form cập nhật — KHÔNG phải key.
    masked: String,
}

/// Lưu key cho 1 provider vào `keyring` hoặc `memory`. Lỗi: `keyring_unavailable`,
/// `keyring_locked`, `invalid` (gồm cả khi yêu cầu `file`), `storage`.
#[tauri::command(rename_all = "snake_case")]
async fn set_secret(
    state: State<'_, AppState>,
    target: Target,
    api_key: String,
    storage: config::KeyStorage,
) -> CmdResult<SetSecretResult> {
    let mut guard = state.config.write().await;
    let secrets = state.secrets.clone();
    let mut cfg = guard.clone();
    let t = target.clone();
    let (cfg, storage) = secret_blocking(move || {
        let s = secrets.set_secret(&mut cfg, &t, &api_key, storage)?;
        Ok((cfg, s))
    })
    .await?;
    *guard = cfg;
    let key = guard.provider(&target.id).and_then(|p| p.api_key.as_deref());
    log::info!("đã lưu key '{}' ({storage:?})", target.account());
    Ok(SetSecretResult { storage, masked: key.map(config::mask_key).unwrap_or_default() })
}

/// Xoá key của provider khỏi keyring, RAM (và plaintext còn trong config.json).
#[tauri::command]
async fn delete_secret(state: State<'_, AppState>, target: Target) -> CmdResult<()> {
    let mut guard = state.config.write().await;
    let secrets = state.secrets.clone();
    let mut cfg = guard.clone();
    let t = target.clone();
    let cfg = secret_blocking(move || {
        secrets.delete_secret(&mut cfg, &t)?;
        Ok(cfg)
    })
    .await?;
    *guard = cfg;
    log::info!("đã xoá key '{}'", target.account());
    Ok(())
}

/// Trạng thái từng key (nơi lưu, đã migrate chưa, lỗi) + file sót lại còn chứa key. Không trả key.
#[tauri::command]
async fn secret_status(state: State<'_, AppState>) -> CmdResult<secrets::SecretStatusReport> {
    let cfg = state.config.read().await.clone();
    let secrets = state.secrets.clone();
    secret_blocking(move || Ok(secrets.status(&cfg))).await
}

/// Xoá file backup/hỏng còn chứa key (chỉ file đang nằm trong `leftover_files`).
#[tauri::command]
async fn delete_leftover_files(state: State<'_, AppState>, files: Vec<String>) -> CmdResult<Vec<String>> {
    let secrets = state.secrets.clone();
    secret_blocking(move || secrets.delete_leftovers(&files)).await
}

// ───────────────────────────── Entry ─────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("hyper", log::LevelFilter::Warn)
                .level_for("reqwest", log::LevelFilter::Warn)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .max_file_size(5 * 1024 * 1024)
                .build(),
        )
        .setup(|app| {
            let storage = Storage::new()?;
            let config = storage.load_config();
            log::info!("TarotCouncil data dir: {}", storage.root().display());
            let secrets = Arc::new(Secrets::new(Arc::new(secrets::KeyringStore), storage.root()));
            let config = Arc::new(tokio::sync::RwLock::new(config));
            {
                // Hydrate key từ keyring ở nền (keyring có thể hiện hộp thoại mở khoá).
                // Lấy write-lock NGAY tại đây → mọi command đọc config đều chờ xong bước này.
                let mut guard = config.clone().try_write_owned().expect("config lock chưa ai giữ lúc setup");
                let secrets = secrets.clone();
                tauri::async_runtime::spawn(async move {
                    let mut cfg = guard.clone();
                    match tauri::async_runtime::spawn_blocking(move || {
                        secrets.startup(&mut cfg);
                        cfg
                    })
                    .await
                    {
                        Ok(cfg) => *guard = cfg,
                        Err(e) => log::error!("task nạp API key panic: {e}"),
                    }
                });
            }
            app.manage(AppState { config, secrets, storage, http: llm_client::build_http_client()? });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            acknowledge_privacy,
            default_roles,
            interpret_reading,
            test_connection,
            list_models,
            check_models,
            set_secret,
            delete_secret,
            secret_status,
            delete_leftover_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TarotCouncil");
}

#[cfg(test)]
mod tests {
    use super::{secrets::SecretError, secrets::Target, CommandError};

    #[test]
    fn errors_have_stable_codes_and_target() {
        let v = serde_json::to_value(CommandError::SecretMissing { target: Target::provider("p1"), label: "x".into() }).unwrap();
        assert_eq!(v["kind"], "secret_missing");
        assert_eq!(v["target"], serde_json::json!({"kind": "provider", "id": "p1"}));
        let v = serde_json::to_value(CommandError::Secret(SecretError::Unavailable("no dbus".into()))).unwrap();
        assert_eq!(v["kind"], "keyring_unavailable");
        assert!(v.get("target").is_none());
        let v = serde_json::to_value(CommandError::Secret(SecretError::Locked("cancel".into()))).unwrap();
        assert_eq!(v["kind"], "keyring_locked");
        assert_eq!(serde_json::to_value(CommandError::NotImplemented).unwrap()["kind"], "not_implemented");
    }

    /// Plan §2 bước 5: keyring service tách hẳn khỏi PlanCouncil.
    #[test]
    fn keyring_service_is_isolated_from_plancouncil() {
        assert_eq!(super::secrets::SERVICE, "com.tarotcouncil.app");
        assert_ne!(super::secrets::SERVICE, "plancouncil");
    }
}
