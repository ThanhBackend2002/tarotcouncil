//! Cấu hình TarotCouncil (schema v1):
//! - `providers`: nơi DUY NHẤT quản lý nguồn API (base URL + API key) + cache danh sách model.
//! - `roles`: các **thầy bói** (persona) — mỗi người có lối luận (`instructions`) và model riêng
//!   (provider + model + temperature) + ngoại hình pixel. Tên trường giữ `roles` để dùng lại UI.
//! - `tarot`: tuỳ chọn đọc bài (bài ngược, kiểu trải / thầy bói mặc định, hiệu ứng, quyền riêng tư).
//!
//! Bảo mật API key (giữ nguyên từ PlanCouncil):
//! - `Debug` được implement thủ công để KHÔNG BAO GIỜ in key ra log.
//! - `redacted()` che key trước khi gửi lên webview; key thật chỉ sống ở Rust.
//! - `merge_secrets()`: giá trị mask (`••••abcd`) = giữ key cũ, nhưng chỉ khi base_url không đổi.
//! - Key thật KHÔNG BAO GIỜ được serialize (`ser_masked_only`). Key nằm trong keyring hoặc RAM
//!   theo `key_storage` — xem `secrets.rs`.

use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::roster;

pub const MASK_PREFIX: &str = "••••";
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_PROVIDER_ID: &str = "default";
const DEFAULT_TEMPERATURE: f32 = 0.7;
/// Số thầy bói tối đa trong 1 lượt đọc (plan §3: 1–4).
pub const MAX_READERS: usize = 4;
/// Ngân sách token tối thiểu khi người dùng đặt `tarot.max_total_tokens`.
pub const MIN_TOTAL_TOKENS: u64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JsonMode {
    /// Chỉ dựa vào prompt + lenient parser (tương thích mọi proxy).
    Off,
    /// Gửi `response_format: {"type":"json_object"}`; tự fallback về Off nếu provider trả 400/422.
    #[default]
    JsonObject,
}

// ───────────────────────────── Nơi lưu API key ─────────────────────────────

/// Nơi lưu API key (key thật chỉ sống trong RAM của Rust, không nằm trong config.json).
///
/// Người dùng chỉ chọn được `keyring` hoặc `memory` (plan §2 bước 3). `file` chỉ còn để
/// đọc/báo cáo key plaintext sót lại trong config.json — `set_secret` từ chối chế độ này.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyStorage {
    /// Keychain (macOS) / Credential Manager (Windows) / Secret Service (Linux).
    #[default]
    Keyring,
    /// Chỉ dùng để báo cáo key plaintext còn sót — không thể chọn.
    File,
    /// Chỉ trong RAM — mất khi đóng app (dùng khi keyring khoá / không khả dụng).
    Memory,
}

/// Chỉ cho phép serialize giá trị đã mask; key thật → `null` (config.json, webview, log JSON).
fn ser_masked_only<S: serde::Serializer>(key: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
    match key.as_deref() {
        Some(k) if k.starts_with(MASK_PREFIX) => s.serialize_some(k),
        _ => s.serialize_none(),
    }
}

// ───────────────────────────── Provider ─────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(serialize_with = "ser_masked_only")]
    pub api_key: Option<String>,
    pub key_storage: KeyStorage,
    /// Cache kết quả `GET /models` gần nhất để dropdown hiện ngay.
    pub models: Vec<String>,
    pub models_fetched_at: Option<DateTime<Utc>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: DEFAULT_PROVIDER_ID.into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: None,
            key_storage: KeyStorage::default(),
            models: vec![],
            models_fetched_at: None,
        }
    }
}

// ───────────────────────────── Thầy bói (persona) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoleLevel {
    CLevel,
    Head,
    Manager,
    Lead,
    #[default]
    Engineer,
    Specialist,
}

/// Ngoại hình nhân vật pixel. Chuỗi rỗng = tự sinh từ id (UI quyết định, backend chỉ lưu).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// "male" | "female" | "" (tự động)
    pub gender: String,
    pub skin: String,
    pub hair_style: String,
    pub hair_color: String,
    pub outfit: String,
    pub outfit_color: String,
    pub eyewear: String,
    pub accessory: String,
}

impl Appearance {
    pub fn new(gender: &str, hair_style: &str, hair_color: &str, outfit: &str, eyewear: &str, accessory: &str) -> Self {
        Self {
            gender: gender.into(),
            hair_style: hair_style.into(),
            hair_color: hair_color.into(),
            outfit: outfit.into(),
            eyewear: eyewear.into(),
            accessory: accessory.into(),
            ..Self::default()
        }
    }
}

/// Một thầy bói (`ReaderPersona` trong plan §4): `instructions` = style_prompt,
/// `provider_id` + `model` = model_endpoint, `appearance` = ngoại hình pixel.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleDef {
    pub id: String,
    pub name: String,
    pub title: String,
    pub level: RoleLevel,
    pub icon: String,
    pub color: String,
    pub description: String,
    /// Lối luận bài của thầy bói — chèn vào system prompt.
    pub instructions: String,
    pub provider_id: String,
    pub model: String,
    pub temperature: f32,
    pub appearance: Appearance,
}

impl Default for RoleDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            title: String::new(),
            level: RoleLevel::default(),
            icon: "user".into(),
            color: "#a1a1aa".into(),
            description: String::new(),
            instructions: String::new(),
            provider_id: String::new(),
            model: String::new(),
            temperature: DEFAULT_TEMPERATURE,
            appearance: Appearance::default(),
        }
    }
}

// ───────────────────────────── Tuỳ chọn đọc bài ─────────────────────────────

/// Hiệu ứng tráo / lật bài. `system` = theo `prefers-reduced-motion`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Animations {
    #[default]
    System,
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TarotSettings {
    pub reversed_enabled: bool,
    pub default_spread: String,
    /// Thầy bói mặc định (1..=4).
    pub default_readers: Vec<String>,
    /// Trưởng thầy — persona tổng hợp (pha 2).
    pub chief_reader_id: String,
    pub animations: Animations,
    /// Người dùng đã đồng ý thông báo quyền riêng tư (câu hỏi + lá được gửi tới provider).
    pub privacy_ack: bool,
    /// Giới hạn token MỀM cho 1 lượt luận giải (None = không giới hạn). Xem `budget.rs`.
    pub max_total_tokens: Option<u64>,
}

impl Default for TarotSettings {
    fn default() -> Self {
        Self {
            reversed_enabled: true,
            default_spread: "three".into(),
            default_readers: vec![roster::CLASSIC_READER.into()],
            chief_reader_id: roster::CHIEF_READER.into(),
            animations: Animations::System,
            privacy_ack: false,
            max_total_tokens: None,
        }
    }
}

/// Giá của 1 model (USD / 1 triệu token). Người dùng tự nhập — không tải từ mạng.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelPrice {
    pub input_per_m: f64,
    pub cached_per_m: f64,
    pub output_per_m: f64,
}

impl ModelPrice {
    /// Chi phí 1 lần gọi. `prompt` đã gồm phần `cached` (theo quy ước OpenAI).
    pub fn cost(&self, prompt: u64, cached: u64, completion: u64) -> f64 {
        let cached = cached.min(prompt);
        ((prompt - cached) as f64 * self.input_per_m
            + cached as f64 * self.cached_per_m
            + completion as f64 * self.output_per_m)
            / 1_000_000.0
    }
}

/// Bảng giá theo tên model (đúng chuỗi `model` của thầy bói).
pub type Pricing = std::collections::BTreeMap<String, ModelPrice>;

// ───────────────────────────── AppConfig ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub providers: Vec<Provider>,
    pub roles: Vec<RoleDef>,
    pub tarot: TarotSettings,
    /// Áp cho mọi thầy bói; None = mặc định của model.
    pub max_tokens: Option<u32>,
    pub request_timeout_secs: u64,
    pub stream_idle_timeout_secs: u64,
    pub max_retries: u32,
    pub json_mode: JsonMode,
    /// Giới hạn số request LLM chạy đồng thời.
    pub max_parallel_calls: u32,
    /// Bảng giá model để ước tính chi phí (mặc định rỗng → chi phí `null`).
    pub pricing: Pricing,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            // Không tạo sẵn provider: người dùng tự thêm (provider đầu tiên được gán cho mọi thầy bói).
            providers: vec![],
            roles: roster::default_roles("", "", DEFAULT_TEMPERATURE),
            tarot: TarotSettings::default(),
            max_tokens: None,
            request_timeout_secs: 120,
            stream_idle_timeout_secs: 60,
            max_retries: 3,
            json_mode: JsonMode::JsonObject,
            max_parallel_calls: 4,
            pricing: Pricing::new(),
        }
    }
}

/// Endpoint đã "phẳng hoá" cho 1 thầy bói — thứ duy nhất `llm_client` cần biết.
#[derive(Clone)]
pub struct ResolvedEndpoint {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub json_mode: JsonMode,
}

/// Bỏ `/chat/completions` thừa ở cuối để ghép được cả `/chat/completions` lẫn `/models`.
pub fn api_root(base_url: &str) -> &str {
    let base = base_url.trim().trim_end_matches('/');
    base.strip_suffix("/chat/completions").unwrap_or(base)
}

impl ResolvedEndpoint {
    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", api_root(&self.base_url))
    }
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl AppConfig {
    pub fn provider(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    fn provider_mut(&mut self, id: &str) -> Option<&mut Provider> {
        self.providers.iter_mut().find(|p| p.id == id)
    }

    pub fn role(&self, id: &str) -> Option<&RoleDef> {
        self.roles.iter().find(|r| r.id == id)
    }

    /// Endpoint của thầy bói. None nếu không tồn tại hoặc chưa gán provider hợp lệ.
    pub fn resolve(&self, role_id: &str) -> Option<ResolvedEndpoint> {
        let r = self.role(role_id)?;
        let p = self.provider(&r.provider_id)?;
        Some(ResolvedEndpoint {
            base_url: p.base_url.trim().to_owned(),
            api_key: non_empty(&p.api_key).map(str::to_owned),
            model: r.model.trim().to_owned(),
            temperature: r.temperature,
            max_tokens: self.max_tokens,
            json_mode: self.json_mode,
        })
    }

    pub fn redacted(&self) -> AppConfig {
        let mut c = self.clone();
        for p in &mut c.providers {
            p.api_key = p.api_key.as_deref().map(mask_key);
        }
        c
    }

    /// Ghép config từ webview với key thật đang lưu (khớp provider theo `id`).
    pub fn merge_secrets(mut self, existing: &AppConfig) -> AppConfig {
        for p in &mut self.providers {
            let old = existing.provider(&p.id);
            // Nơi lưu key chỉ đổi qua `set_secret` → lấy theo bản đang lưu.
            p.key_storage = old.map(|o| o.key_storage).unwrap_or_default();
            p.api_key = match p.api_key.take() {
                Some(k) if k.starts_with(MASK_PREFIX) => match old {
                    Some(o) if api_root(&o.base_url) == api_root(&p.base_url) => o.api_key.clone(),
                    Some(_) => {
                        log::warn!("provider '{}' đổi base_url → bỏ key cũ, cần nhập lại", p.name);
                        None
                    }
                    None => None,
                },
                Some(k) if k.trim().is_empty() => None,
                Some(k) => Some(k.trim().to_owned()),
                None => None,
            };
        }
        // `privacy_ack` chỉ đổi qua command riêng (sau khi người dùng bấm Đồng ý), không qua form.
        self.tarot.privacy_ack = existing.tarot.privacy_ack;
        self.schema_version = CONFIG_SCHEMA_VERSION;
        self
    }

    pub fn set_models(&mut self, provider_id: &str, models: Vec<String>, at: DateTime<Utc>) -> bool {
        match self.provider_mut(provider_id) {
            Some(p) => {
                p.models = models;
                p.models_fetched_at = Some(at);
                true
            }
            None => false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut errs = Vec::new();
        if !(10..=1800).contains(&self.request_timeout_secs) {
            errs.push("request_timeout_secs phải trong khoảng 10..=1800".to_owned());
        }
        if !(10..=600).contains(&self.stream_idle_timeout_secs) {
            errs.push("stream_idle_timeout_secs phải trong khoảng 10..=600".to_owned());
        }
        if self.max_retries > 6 {
            errs.push("max_retries tối đa 6".to_owned());
        }
        if !(1..=16).contains(&self.max_parallel_calls) {
            errs.push("max_parallel_calls phải trong khoảng 1..=16".to_owned());
        }
        if self.max_tokens == Some(0) {
            errs.push("max_tokens phải ≥ 1 hoặc để trống".to_owned());
        }
        for (model, p) in &self.pricing {
            if model.trim().is_empty() {
                errs.push("bảng giá: tên model không được để trống".to_owned());
            }
            if [p.input_per_m, p.cached_per_m, p.output_per_m].iter().any(|v| !v.is_finite() || *v < 0.0) {
                errs.push(format!("bảng giá '{model}': giá phải là số hữu hạn ≥ 0"));
            }
        }

        let mut seen = HashSet::new();
        for p in &self.providers {
            let label = if p.name.trim().is_empty() { &p.id } else { &p.name };
            if p.id.trim().is_empty() {
                errs.push("provider thiếu id".to_owned());
            } else if !seen.insert(p.id.as_str()) {
                errs.push(format!("provider id '{}' bị trùng", p.id));
            }
            if p.name.trim().is_empty() {
                errs.push(format!("provider '{}': tên không được để trống", p.id));
            }
            if let Err(e) = validate_url(&p.base_url) {
                errs.push(format!("provider '{label}': base_url không hợp lệ ({e})"));
            }
        }

        let mut seen = HashSet::new();
        for r in &self.roles {
            let label = if r.name.trim().is_empty() { &r.id } else { &r.name };
            if r.id.trim().is_empty() {
                errs.push("có thầy bói thiếu id".to_owned());
            } else if !seen.insert(r.id.as_str()) {
                errs.push(format!("thầy bói id '{}' bị trùng", r.id));
            }
            if r.name.trim().is_empty() {
                errs.push(format!("thầy bói '{}': tên không được để trống", r.id));
            }
            if !r.provider_id.is_empty() && self.provider(&r.provider_id).is_none() {
                errs.push(format!("{label}: provider '{}' không tồn tại", r.provider_id));
            }
            if !(0.0..=2.0).contains(&r.temperature) {
                errs.push(format!("{label}: temperature phải trong 0..=2"));
            }
        }

        let t = &self.tarot;
        if t.default_readers.is_empty() || t.default_readers.len() > MAX_READERS {
            errs.push(format!("thầy bói mặc định: cần 1..={MAX_READERS} người"));
        }
        for id in &t.default_readers {
            if self.role(id).is_none() {
                errs.push(format!("thầy bói mặc định '{id}' không tồn tại"));
            }
        }
        if self.role(&t.chief_reader_id).is_none() {
            errs.push(format!("trưởng thầy '{}' không tồn tại", t.chief_reader_id));
        }
        if t.max_total_tokens.is_some_and(|v| v < MIN_TOTAL_TOKENS) {
            errs.push(format!("ngân sách token của lượt đọc phải ≥ {MIN_TOTAL_TOKENS} hoặc để trống"));
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs.join("; "))
        }
    }

    /// Trước khi luận giải: cấu trúc hợp lệ + các thầy bói tham gia (và trưởng thầy) đã có provider/model.
    pub fn validate_ready(&self, reader_ids: &[String], chief_id: &str) -> Result<(), String> {
        self.validate()?;
        if self.providers.is_empty() {
            return Err("Chưa có provider nào — vào Cài đặt → Provider để thêm API key".into());
        }
        let mut missing = Vec::new();
        for id in reader_ids.iter().map(String::as_str).chain([chief_id]) {
            let Some(r) = self.role(id) else {
                missing.push(format!("'{id}' không tồn tại"));
                continue;
            };
            if self.provider(&r.provider_id).is_none() || r.model.trim().is_empty() {
                missing.push(format!("{} chưa chọn provider/model", r.name));
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing.join("; "))
        }
    }

    /// Đọc config từ JSON (trường thiếu → mặc định nhờ `serde(default)`).
    pub fn from_json(bytes: &[u8]) -> Result<AppConfig, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

pub fn validate_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|e| e.to_string())?;
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if !matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") {
                log::warn!("base_url dùng http:// tới host không phải loopback ({host}) — API key sẽ đi plaintext");
            }
            Ok(())
        }
        s => Err(format!("scheme '{s}' không được hỗ trợ")),
    }
}

pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() > 8 {
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{MASK_PREFIX}{tail}")
    } else {
        MASK_PREFIX.to_owned()
    }
}

// ── Debug thủ công: tuyệt đối không lộ key ──

fn fmt_key(k: &Option<String>) -> &'static str {
    if k.as_deref().is_some_and(|s| !s.is_empty()) {
        "<redacted>"
    } else {
        "<none>"
    }
}

impl fmt::Debug for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Provider")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_key", &fmt_key(&self.api_key))
            .field("key_storage", &self.key_storage)
            .field("models", &self.models.len())
            .finish()
    }
}

impl fmt::Debug for ResolvedEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedEndpoint")
            .field("base_url", &self.base_url)
            .field("api_key", &fmt_key(&self.api_key))
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Config mặc định + 1 provider "default" được gán cho mọi thầy bói.
    pub(crate) fn with_default_provider(key: &str, model: &str) -> AppConfig {
        let mut c = AppConfig::default();
        c.providers.push(Provider { api_key: Some(key.into()), ..Provider::default() });
        for r in &mut c.roles {
            r.provider_id = DEFAULT_PROVIDER_ID.into();
            r.model = model.into();
        }
        c
    }

    fn with_openrouter() -> AppConfig {
        let mut c = with_default_provider("sk-default-123456", "gpt-4.1-mini");
        c.providers.push(Provider {
            id: "or".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1/".into(),
            api_key: Some("sk-or-abcdefgh9999".into()),
            ..Provider::default()
        });
        let jung = c.roles.iter_mut().find(|r| r.id == roster::JUNG_READER).unwrap();
        jung.provider_id = "or".into();
        jung.model = "anthropic/claude-big".into();
        c
    }

    #[test]
    fn default_config_is_valid_and_has_four_readers_plus_chief() {
        let c = AppConfig::default();
        c.validate().unwrap();
        assert!(c.providers.is_empty());
        assert!(c.validate_ready(&c.tarot.default_readers, &c.tarot.chief_reader_id).unwrap_err().contains("Chưa có provider"));
        for id in [roster::CLASSIC_READER, roster::JUNG_READER, roster::PRACTICAL_READER, roster::SKEPTIC_READER, roster::CHIEF_READER] {
            assert!(c.role(id).is_some(), "thiếu persona {id}");
        }
        assert!(!c.tarot.privacy_ack);
        assert!(c.tarot.reversed_enabled);
    }

    #[test]
    fn resolve_uses_reader_provider_and_model() {
        let c = with_openrouter();
        let a = c.resolve(roster::JUNG_READER).unwrap();
        assert_eq!(a.model, "anthropic/claude-big");
        assert_eq!(a.chat_url(), "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(a.api_key.as_deref(), Some("sk-or-abcdefgh9999"));
        let s = c.resolve(roster::CLASSIC_READER).unwrap();
        assert_eq!(s.api_key.as_deref(), Some("sk-default-123456"));
        assert!(c.resolve("nobody").is_none());
        assert_eq!(api_root("https://x/v1/chat/completions/"), "https://x/v1");
    }

    #[test]
    fn redact_and_merge_roundtrip_keeps_secret() {
        let stored = with_openrouter();
        let view = stored.redacted();
        assert_eq!(view.providers[0].api_key.as_deref(), Some("••••3456"));
        assert!(!format!("{view:?}{stored:?}").contains("sk-default"));

        let mut incoming = view.clone();
        incoming.providers[1].api_key = Some(String::new());
        incoming.providers.push(Provider {
            id: "new".into(),
            name: "Local".into(),
            base_url: "http://localhost:11434/v1".into(),
            api_key: Some(" sk-new ".into()),
            ..Provider::default()
        });
        let merged = incoming.merge_secrets(&stored);
        assert_eq!(merged.providers[0].api_key.as_deref(), Some("sk-default-123456"));
        assert_eq!(merged.providers[1].api_key, None);
        assert_eq!(merged.providers[2].api_key.as_deref(), Some("sk-new"));
    }

    #[test]
    fn real_keys_are_never_serialized_and_storage_comes_from_saved_config() {
        let mut stored = with_openrouter();
        stored.providers[1].key_storage = KeyStorage::Memory;
        let json = serde_json::to_string(&stored).unwrap();
        assert!(!json.contains("sk-default") && !json.contains("sk-or-"), "{json}");
        let view = serde_json::to_value(stored.redacted()).unwrap();
        assert_eq!(view["providers"][0]["api_key"], "••••3456");

        // Webview gửi key_storage khác → bỏ qua, chỉ `set_secret` mới đổi được nơi lưu.
        let mut incoming = stored.redacted();
        incoming.providers[0].key_storage = KeyStorage::Memory;
        incoming.providers[1].key_storage = KeyStorage::Keyring;
        let merged = incoming.merge_secrets(&stored);
        assert_eq!(merged.providers[0].key_storage, KeyStorage::Keyring);
        assert_eq!(merged.providers[1].key_storage, KeyStorage::Memory);
    }

    #[test]
    fn privacy_ack_cannot_be_set_through_save_config() {
        let stored = AppConfig::default();
        let mut incoming = stored.redacted();
        incoming.tarot.privacy_ack = true;
        assert!(!incoming.merge_secrets(&stored).tarot.privacy_ack);
    }

    #[test]
    fn masked_key_is_dropped_when_base_url_changes() {
        let stored = with_openrouter();
        let mut incoming = stored.redacted();
        incoming.providers[0].base_url = "https://evil.example.com/v1".into();
        assert_eq!(incoming.merge_secrets(&stored).providers[0].api_key, None);
    }

    #[test]
    fn validate_rejects_bad_providers_readers_and_tarot_settings() {
        let mut c = with_default_provider("sk-x", "m");
        c.providers[0].base_url = "ftp://x".into();
        c.roles[0].provider_id = "ghost-provider".into();
        c.roles.push(RoleDef { id: roster::CLASSIC_READER.into(), name: "Dup".into(), ..RoleDef::default() });
        c.tarot.default_readers = vec!["ghost".into(); 5];
        c.tarot.chief_reader_id = "nobody".into();
        c.tarot.max_total_tokens = Some(10);
        let e = c.validate().unwrap_err();
        for needle in ["base_url", "ghost-provider", "bị trùng", "1..=4", "'ghost' không tồn tại", "trưởng thầy", "≥ 1000"] {
            assert!(e.contains(needle), "thiếu '{needle}' trong: {e}");
        }
    }

    #[test]
    fn validate_ready_requires_model_for_readers_and_chief() {
        let mut c = with_default_provider("sk-x", "m");
        c.validate_ready(&[roster::CLASSIC_READER.into()], roster::CHIEF_READER).unwrap();
        c.roles.iter_mut().find(|r| r.id == roster::CHIEF_READER).unwrap().model.clear();
        assert!(c.validate_ready(&[roster::CLASSIC_READER.into()], roster::CHIEF_READER).unwrap_err().contains("chưa chọn provider/model"));
    }

    #[test]
    fn partial_json_fills_defaults() {
        let c = AppConfig::from_json(br#"{"providers":[{"id":"a","name":"A","base_url":"https://x/v1"}],"tarot":{"reversed_enabled":false}}"#).unwrap();
        assert!(!c.tarot.reversed_enabled);
        assert_eq!(c.tarot.chief_reader_id, roster::CHIEF_READER);
        assert_eq!(c.providers[0].key_storage, KeyStorage::Keyring);
        assert_eq!(c.roles.len(), AppConfig::default().roles.len());
    }
}
