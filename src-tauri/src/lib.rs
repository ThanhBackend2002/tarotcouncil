//! Tauri glue: state + commands. Bất biến giữ từ PlanCouncil: UI không chạm API key,
//! mọi lời gọi AI chạy trong Rust.

// budget / normalize / llm_client: đồng bộ thủ công với upstream (UPSTREAM.md) → giữ nguyên API.
#[allow(dead_code)]
mod budget;
mod config;
mod cost;
mod deck;
mod draw;
mod events;
mod health;
mod journal;
#[allow(dead_code)]
mod llm_client;
mod models;
#[allow(dead_code)]
mod normalize;
mod prompts;
mod reading;
mod reading_orchestrator;
mod roster;
mod safety;
mod secrets;
mod spreads;
mod storage;
mod support;
mod table;
#[cfg(test)]
pub(crate) mod test_support;
mod validate;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use config::{api_root, AppConfig, MASK_PREFIX};
use events::{Envelope, EventSink, ReadingEmitter, READING_EVENT};
use journal::{Journal, JournalError};
use llm_client::{ChatMessage, Expect, LlmClient, LlmError};
use reading::{Reading, ReadingSummary};
use secrets::{SecretError, Secrets, Target};
use storage::{Storage, StorageError};
use table::TableError;

// ───────────────────────────── State ─────────────────────────────

type Locks = Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>;

pub struct AppState {
    /// `Arc` để lúc khởi động giữ write-lock (owned) trong lúc hydrate key ở nền.
    config: Arc<tokio::sync::RwLock<AppConfig>>,
    /// API key: keyring / RAM + trạng thái (xem `secrets.rs`).
    secrets: Arc<Secrets>,
    journal: Arc<Journal>,
    emitter: Arc<ReadingEmitter>,
    http: reqwest::Client,
    /// Lock theo lượt đọc: chống click đúp khi bốc lá, chống chạy 2 attempt cùng lúc.
    locks: Locks,
    /// Lượt đọc đang luận giải → token huỷ.
    active: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    fn reading_lock(&self, id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        lock(&self.locks).entry(id).or_default().clone()
    }
}

struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, env: &Envelope) {
        if let Err(e) = self.0.emit(READING_EVENT, env) {
            log::warn!("emit {READING_EVENT} thất bại: {e}");
        }
    }
}

// ───────────────────────────── Errors ─────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Busy(String),
    #[error("Cần bạn đồng ý thông báo quyền riêng tư trước khi luận giải")]
    PrivacyRequired,
    #[error("Không lưu được lượt đọc: {0}")]
    PersistFailed(String),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Llm(#[from] LlmError),
    /// `keyring_unavailable` / `keyring_locked` / storage / invalid.
    #[error(transparent)]
    Secret(#[from] SecretError),
    /// Key chế độ RAM đã mất sau khi khởi động lại (hoặc keyring khoá) → cần nhập lại.
    #[error("Thiếu API key của {label} (chỉ lưu trong RAM hoặc keyring đang khoá) — vào Cài đặt để nhập lại")]
    SecretMissing { target: Target, label: String },
    #[error("lỗi nội bộ: {0}")]
    Internal(String),
}

impl From<TableError> for CommandError {
    fn from(e: TableError) -> Self {
        match e {
            TableError::Invalid(m) => CommandError::Invalid(m),
            TableError::Busy(m) => CommandError::Busy(m),
        }
    }
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
            CommandError::Busy(_) => "busy",
            CommandError::PrivacyRequired => "privacy_required",
            CommandError::PersistFailed(_) => "persist_failed",
            CommandError::Storage(StorageError::NotFound(_)) => "not_found",
            CommandError::Storage(_) => "storage",
            CommandError::Journal(JournalError::Unavailable) => "index_unavailable",
            CommandError::Journal(JournalError::Index(_)) => "index",
            CommandError::Llm(_) => "llm",
            CommandError::Internal(_) => "internal",
        };
        Wire { kind, message: self.to_string(), target }.serialize(s)
    }
}

type CmdResult<T> = Result<T, CommandError>;

async fn run_blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CommandError::Internal(e.to_string()))
}

/// Thao tác keyring / secrets là blocking (có thể hiện hộp thoại OS) → chạy ở `spawn_blocking`.
async fn secret_blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, SecretError> + Send + 'static,
{
    run_blocking(f).await?.map_err(Into::into)
}

async fn load_reading(state: &AppState, id: Uuid) -> CmdResult<Reading> {
    let j = state.journal.clone();
    Ok(run_blocking(move || j.load(id)).await??)
}

/// Ghi atomic; lỗi → `persist_failed` (UI không lật lá / không coi là xong).
async fn save_reading(state: &AppState, r: &Reading) -> CmdResult<()> {
    let (j, r) = (state.journal.clone(), r.clone());
    run_blocking(move || j.save(&r)).await?.map_err(|e| CommandError::PersistFailed(e.to_string()))
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

// ───────────────────────────── Bộ bài & kiểu trải ─────────────────────────────

#[tauri::command]
fn list_spreads() -> Vec<&'static spreads::Spread> {
    spreads::all().iter().collect()
}

#[tauri::command]
fn get_deck() -> &'static deck::Deck {
    deck::deck()
}

#[tauri::command]
fn get_support_info() -> support::SupportInfo {
    support::info()
}

// ───────────────────────────── Bốc bài ─────────────────────────────

#[derive(Serialize)]
struct CreateReadingResult {
    /// `"self_harm"` → UI mở SupportScreen; không có lượt đọc nào được tạo.
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reading: Option<serde_json::Value>,
}

#[tauri::command(rename_all = "snake_case")]
async fn create_reading(
    state: State<'_, AppState>,
    question: String,
    spread_id: String,
    reader_ids: Vec<String>,
) -> CmdResult<CreateReadingResult> {
    let cfg = state.config.read().await.clone();
    let seed = draw::new_seed().map_err(CommandError::Internal)?;
    match table::create(&cfg, &question, &spread_id, &reader_ids, seed)? {
        table::Created::Blocked => {
            log::info!("create_reading: câu hỏi trúng nhóm tự hại → SupportScreen");
            Ok(CreateReadingResult { blocked: Some("self_harm"), reading: None })
        }
        table::Created::Reading(r) => {
            save_reading(&state, &r).await?;
            log::info!("create_reading {} spread={} readers={}", r.id, r.spread_id, r.reader_ids.len());
            Ok(CreateReadingResult { blocked: None, reading: Some(r.public_view()) })
        }
    }
}

/// Chạy `f` trên lượt đọc dưới lock của lượt đó, ghi atomic rồi mới trả kết quả.
async fn mutate<T: Send + 'static>(
    state: &AppState,
    id: Uuid,
    f: impl FnOnce(&mut Reading) -> Result<T, TableError>,
) -> CmdResult<(T, Reading)> {
    let l = state.reading_lock(id);
    let _g = l.lock().await;
    let mut r = load_reading(state, id).await?;
    let out = f(&mut r)?;
    save_reading(state, &r).await?;
    Ok((out, r))
}

#[derive(Serialize)]
struct DrawResult {
    draw: reading::Draw,
    reading: serde_json::Value,
}

#[tauri::command(rename_all = "snake_case")]
async fn draw_card(state: State<'_, AppState>, reading_id: Uuid, fan_index: u8) -> CmdResult<DrawResult> {
    let (draw, r) = mutate(&state, reading_id, |r| table::draw_card(r, fan_index)).await?;
    Ok(DrawResult { draw, reading: r.public_view() })
}

#[tauri::command(rename_all = "snake_case")]
async fn auto_draw(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<serde_json::Value> {
    let (_, r) = mutate(&state, reading_id, table::auto_draw).await?;
    Ok(r.public_view())
}

#[tauri::command(rename_all = "snake_case")]
async fn reshuffle_reading(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<serde_json::Value> {
    let seed = draw::new_seed().map_err(CommandError::Internal)?;
    let (_, r) = mutate(&state, reading_id, |r| table::reshuffle(r, seed)).await?;
    Ok(r.public_view())
}

// ───────────────────────────── Luận giải ─────────────────────────────

/// Key của provider mà các thầy bói tham gia dùng phải còn (RAM sau khởi động lại → `secret_missing`).
fn check_secrets_present(secrets: &Secrets, cfg: &AppConfig, ids: &[String]) -> CmdResult<()> {
    let mut wanted: Vec<Target> = Vec::new();
    for id in ids {
        if let Some(r) = cfg.role(id) {
            let t = Target::provider(&r.provider_id);
            if !wanted.contains(&t) {
                wanted.push(t);
            }
        }
    }
    match secrets.first_missing(cfg, &wanted) {
        None => Ok(()),
        Some(target) => {
            let name = cfg.provider(&target.id).map_or(target.id.as_str(), |p| p.name.as_str());
            Err(CommandError::SecretMissing { label: format!("provider \"{name}\""), target })
        }
    }
}

/// Bắt đầu 1 attempt luận giải ở nền. Trả `attempt_id`. Lỗi: `privacy_required`, `busy`,
/// `invalid` (thiếu model), `secret_missing`, `persist_failed`, `not_found`.
#[tauri::command(rename_all = "snake_case")]
async fn interpret_reading(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<u32> {
    let cfg = state.config.read().await.clone();
    if !cfg.tarot.privacy_ack {
        return Err(CommandError::PrivacyRequired);
    }
    let l = state.reading_lock(reading_id);
    let _g = l.lock().await;
    let mut r = load_reading(&state, reading_id).await?;
    cfg.validate_ready(&r.reader_ids, &r.chief_reader_id).map_err(CommandError::Invalid)?;
    let mut ids = r.reader_ids.clone();
    ids.push(r.chief_reader_id.clone());
    check_secrets_present(&state.secrets, &cfg, &ids)?;
    let attempt_id = table::begin_interpret(&mut r)?;
    save_reading(&state, &r).await?;

    let cancel = CancellationToken::new();
    lock(&state.active).insert(reading_id, cancel.clone());
    let job = reading_orchestrator::Job {
        reading_id,
        attempt_id,
        llm: LlmClient::new(state.http.clone(), &cfg),
        cfg,
        cancel,
    };
    let svc = reading_orchestrator::Services { journal: state.journal.clone(), emitter: state.emitter.clone() };
    let active = state.active.clone();
    tauri::async_runtime::spawn(async move {
        let t0 = Instant::now();
        reading_orchestrator::run(job, svc).await;
        lock(&active).remove(&reading_id);
        log::info!("reading {reading_id} attempt {attempt_id} xong sau {} ms", t0.elapsed().as_millis());
    });
    Ok(attempt_id)
}

/// Dừng luận giải; các lá đã bốc vẫn được giữ.
#[tauri::command(rename_all = "snake_case")]
fn cancel_reading(state: State<'_, AppState>, reading_id: Uuid) -> bool {
    match lock(&state.active).get(&reading_id) {
        Some(t) => {
            t.cancel();
            true
        }
        None => false,
    }
}

// ───────────────────────────── Nhật ký ─────────────────────────────

#[tauri::command]
async fn list_readings(state: State<'_, AppState>, limit: Option<usize>) -> CmdResult<Vec<ReadingSummary>> {
    let j = state.journal.clone();
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    run_blocking(move || j.list(limit)).await
}

/// Tìm không phân biệt dấu. Không có index → `index_unavailable`.
#[tauri::command]
async fn search_readings(state: State<'_, AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<ReadingSummary>> {
    let j = state.journal.clone();
    let limit = limit.unwrap_or(100).clamp(1, 500);
    Ok(run_blocking(move || j.search(&query, limit)).await??)
}

#[derive(Serialize)]
struct ReadingSnapshot {
    reading: serde_json::Value,
    attempt_id: u32,
    last_seq: u64,
    /// Đang có task luận giải chạy cho lượt này trong tiến trình hiện tại.
    live: bool,
}

/// Snapshot đọc dưới CÙNG mutex cấp seq của `ReadingEmitter` (plan §3 — đồng bộ ở client).
#[tauri::command(rename_all = "snake_case")]
async fn get_reading(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<ReadingSnapshot> {
    let (j, em) = (state.journal.clone(), state.emitter.clone());
    let live = lock(&state.active).contains_key(&reading_id);
    let (res, last_seq) = run_blocking(move || em.with_seq(reading_id, |seq| (j.load(reading_id), seq))).await?;
    let r = res?;
    Ok(ReadingSnapshot { attempt_id: r.attempt_id, reading: r.public_view(), last_seq, live })
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_reading(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<bool> {
    if lock(&state.active).contains_key(&reading_id) {
        return Err(CommandError::Busy("Hãy dừng luận giải trước khi xoá".into()));
    }
    let l = state.reading_lock(reading_id);
    let _g = l.lock().await;
    let j = state.journal.clone();
    let ok = run_blocking(move || j.delete(reading_id)).await??;
    lock(&state.locks).remove(&reading_id);
    Ok(ok)
}

#[tauri::command(rename_all = "snake_case")]
async fn verify_reading(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<table::Verification> {
    Ok(table::verify(&load_reading(&state, reading_id).await?))
}

#[tauri::command(rename_all = "snake_case")]
async fn get_usage(state: State<'_, AppState>, reading_id: Uuid) -> CmdResult<cost::UsageBreakdown> {
    let r = load_reading(&state, reading_id).await?;
    let pricing = state.config.read().await.pricing.clone();
    Ok(cost::usage_breakdown(&r.calls, &pricing))
}

/// Settings → "Mở thư mục log".
#[tauri::command]
fn open_log_dir(app: AppHandle) -> CmdResult<String> {
    let dir = app.path().app_log_dir().map_err(|e| CommandError::Internal(e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| CommandError::Internal(e.to_string()))?;
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(&dir).spawn().map_err(|e| CommandError::Internal(e.to_string()))?;
    Ok(dir.display().to_string())
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
        CommandError::Invalid(format!("thầy bói '{agent}' không tồn tại hoặc chưa chọn provider (hãy lưu cấu hình trước)"))
    })?;
    ep.max_tokens = Some(32);
    let llm = LlmClient::new(state.http.clone(), &cfg).with_limits(0, Duration::from_secs(30));
    let msgs = [ChatMessage::system("You are a health check endpoint."), ChatMessage::user("Reply with exactly: OK")];
    let t0 = Instant::now();
    let reply = llm.chat(&ep, &msgs, Expect::Text, &CancellationToken::new()).await?;
    Ok(ConnectionTestResult { agent, model: ep.model, latency_ms: t0.elapsed().as_millis(), reply: models::truncate_chars(reply.text.trim(), 100) })
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
async fn list_models(state: State<'_, AppState>, provider_id: String, base_url: Option<String>, api_key: Option<String>) -> CmdResult<ModelList> {
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
        let still_same = guard.provider(&provider_id).is_some_and(|p| api_root(&p.base_url) == api_root(&base) && p.api_key == key);
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
async fn set_secret(state: State<'_, AppState>, target: Target, api_key: String, storage: config::KeyStorage) -> CmdResult<SetSecretResult> {
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
            // Validate bộ bài lúc khởi động (panic sớm nếu file nhúng hỏng — CI đã chặn trước).
            log::info!("bộ bài v{}: {} lá", deck::deck().deck_version, deck::deck().cards.len());
            let storage = Storage::new()?;
            let config = storage.load_config();
            log::info!("TarotCouncil data dir: {}", storage.root().display());
            let journal = Arc::new(Journal::open(storage.clone()));
            {
                // Đồng bộ index + khôi phục lượt `interpreting` bị gián đoạn ở nền.
                let j = journal.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let rep = j.sync();
                    if rep.rebuilt || !rep.failed.is_empty() {
                        log::warn!("dựng lại chỉ mục Nhật ký: {} lượt, lỗi {:?}", rep.indexed, rep.failed);
                    }
                    let fixed = j.recover_interrupted();
                    if !fixed.is_empty() {
                        log::warn!("{} lượt đọc bị gián đoạn → drawn: {fixed:?}", fixed.len());
                    }
                });
            }
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
            app.manage(AppState {
                config,
                secrets,
                journal,
                emitter: Arc::new(ReadingEmitter::new(Arc::new(TauriSink(app.handle().clone())))),
                http: llm_client::build_http_client()?,
                locks: Mutex::new(HashMap::new()),
                active: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            acknowledge_privacy,
            default_roles,
            list_spreads,
            get_deck,
            get_support_info,
            create_reading,
            draw_card,
            auto_draw,
            reshuffle_reading,
            interpret_reading,
            cancel_reading,
            list_readings,
            search_readings,
            get_reading,
            delete_reading,
            verify_reading,
            get_usage,
            open_log_dir,
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
        for (e, k) in [
            (CommandError::PrivacyRequired, "privacy_required"),
            (CommandError::Busy("x".into()), "busy"),
            (CommandError::PersistFailed("x".into()), "persist_failed"),
            (CommandError::Journal(super::JournalError::Unavailable), "index_unavailable"),
        ] {
            assert_eq!(serde_json::to_value(e).unwrap()["kind"], k);
        }
    }

    /// Plan §2 bước 5: keyring service tách hẳn khỏi PlanCouncil.
    #[test]
    fn keyring_service_is_isolated_from_plancouncil() {
        assert_eq!(super::secrets::SERVICE, "com.tarotcouncil.app");
        assert_ne!(super::secrets::SERVICE, "plancouncil");
    }
}
