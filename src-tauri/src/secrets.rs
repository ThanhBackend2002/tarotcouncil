//! API key nằm NGOÀI config.json.
//!
//! Người dùng chọn được 2 chế độ (plan §2 bước 3):
//! - `keyring` (mặc định): keyring của hệ điều hành, service `com.tarotcouncil.app`
//!   (tách hẳn khỏi PlanCouncil), account `provider:<id>`.
//! - `memory`: chỉ trong RAM, mất khi đóng app — dùng khi keyring khoá / không khả dụng.
//!
//! `set_secret` từ chối `file`; nhánh đọc `secrets.json` chỉ còn để không làm mất key nếu file
//! đó tồn tại (vd người dùng tự đặt), và mọi lần lưu đều đồng bộ lại file.
//!
//! Runtime vẫn giữ key thật ở `api_key` (hydrate sau `load_config()`), nên `llm_client`,
//! `health`, `tools` không cần biết key nằm ở đâu.
//!
//! Migration lúc khởi động (từng key): ghi keyring → đọc lại so khớp → ghi config.json (atomic)
//! không còn key đó → đọc lại → rồi mới bỏ bản plaintext. Bước nào lỗi thì key giữ nguyên chỗ
//! cũ (plaintext trong config.json), app vẫn chạy, lỗi được ghi vào trạng thái cho UI. KHÔNG BAO
//! GIỜ tự chuyển sang `file`.
//!
//! Mọi hàm ở đây là **blocking** (keyring có thể hiện hộp thoại mở khoá) — caller dùng `spawn_blocking`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{AppConfig, KeyStorage, MASK_PREFIX};
use crate::storage::write_atomic;

pub const SERVICE: &str = "com.tarotcouncil.app";
pub const SECRETS_FILE: &str = "secrets.json";
const CONFIG_FILE: &str = "config.json";

// ───────────────────────────── Lỗi ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// Không có backend keyring (Linux không có Secret Service / D-Bus, nền tảng không hỗ trợ…).
    #[error("Keyring của hệ điều hành không khả dụng ({0})")]
    Unavailable(String),
    /// Keyring đang khoá, bị từ chối truy cập hoặc người dùng huỷ hộp thoại.
    #[error("Keyring đang khoá hoặc bị từ chối truy cập ({0})")]
    Locked(String),
    #[error("Lỗi ghi/đọc file: {0}")]
    Io(String),
    #[error("{0}")]
    Invalid(String),
}

impl SecretError {
    /// Mã lỗi gửi lên frontend (`CommandError.kind`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "keyring_unavailable",
            Self::Locked(_) => "keyring_locked",
            Self::Io(_) => "storage",
            Self::Invalid(_) => "invalid",
        }
    }
}

impl From<io::Error> for SecretError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for SecretError {
    fn from(e: serde_json::Error) -> Self {
        Self::Io(e.to_string())
    }
}

// ───────────────────────────── Store ─────────────────────────────

/// Kho bí mật dạng key-value. Test dùng bản giả trong RAM.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError>;
    /// Không có entry → Ok.
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// Keyring thật của hệ điều hành (crate `keyring` v3, feature bật theo target trong Cargo.toml).
pub struct KeyringStore;

/// Phân loại lỗi keyring theo loại (`no_access` = `NoStorageAccess`) + nội dung thông báo.
/// Tách riêng để test được trên mọi OS.
fn classify(no_access: bool, msg: String) -> SecretError {
    let lower = msg.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    // Không có dịch vụ: errSecNotAvailable/NoSuchKeychain/InvalidKeychain (macOS),
    // ERROR_NO_SUCH_LOGON_SESSION (Windows), không có D-Bus / Secret Service (Linux).
    let no_service = has(&[
        "not available",
        "no such keychain",
        "invalid keychain",
        "logon session",
        "serviceunknown",
        "org.freedesktop.dbus.error",
        "-25291",
        "-25294",
        "-25295",
        "1312",
    ]);
    // Bị từ chối / người dùng huỷ: errSecUserCanceled, errSecAuthFailed, errSecInteractionNotAllowed…
    let denied = has(&["cancel", "denied", "not allowed", "passphrase", "locked", "dismissed", "-128", "-25293", "-25308"]);
    if no_access {
        if no_service { SecretError::Unavailable(msg) } else { SecretError::Locked(msg) }
    } else if denied {
        SecretError::Locked(msg)
    } else {
        SecretError::Unavailable(msg)
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
))]
mod os {
    use super::{classify, SecretError, SERVICE};
    use keyring::{Entry, Error};

    fn map(e: Error) -> SecretError {
        let no_access = matches!(e, Error::NoStorageAccess(_));
        classify(no_access, e.to_string())
    }

    fn entry(account: &str) -> Result<Entry, SecretError> {
        Entry::new(SERVICE, account).map_err(map)
    }

    pub fn get(account: &str) -> Result<Option<String>, SecretError> {
        match entry(account)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(map(e)),
        }
    }

    pub fn set(account: &str, secret: &str) -> Result<(), SecretError> {
        entry(account)?.set_password(secret).map_err(map)
    }

    pub fn delete(account: &str) -> Result<(), SecretError> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(map(e)),
        }
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
mod os {
    use super::SecretError;
    fn none() -> SecretError {
        SecretError::Unavailable("nền tảng không hỗ trợ keyring".into())
    }
    pub fn get(_: &str) -> Result<Option<String>, SecretError> {
        Err(none())
    }
    pub fn set(_: &str, _: &str) -> Result<(), SecretError> {
        Err(none())
    }
    pub fn delete(_: &str) -> Result<(), SecretError> {
        Err(none())
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        os::get(account)
    }
    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        os::set(account, secret)
    }
    fn delete(&self, account: &str) -> Result<(), SecretError> {
        os::delete(account)
    }
}

// ───────────────────────────── Target ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Provider,
}

/// Key nào: `{kind: "provider", id: <provider id>}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Target {
    pub kind: TargetKind,
    pub id: String,
}

impl Target {
    pub fn provider(id: &str) -> Self {
        Self { kind: TargetKind::Provider, id: id.to_owned() }
    }

    /// Account trong keyring / khoá trong secrets.json.
    pub fn account(&self) -> String {
        match self.kind {
            TargetKind::Provider => format!("provider:{}", self.id),
        }
    }
}

/// Mọi chỗ có thể chứa key trong config: từng provider.
pub fn targets(cfg: &AppConfig) -> Vec<Target> {
    cfg.providers.iter().map(|p| Target::provider(&p.id)).collect()
}

fn slot<'a>(cfg: &'a AppConfig, t: &Target) -> Option<(Option<&'a str>, KeyStorage)> {
    let key = |k: &'a Option<String>| k.as_deref().filter(|s| !s.trim().is_empty());
    match t.kind {
        TargetKind::Provider => cfg.provider(&t.id).map(|p| (key(&p.api_key), p.key_storage)),
    }
}

fn slot_mut<'a>(cfg: &'a mut AppConfig, t: &Target) -> Option<(&'a mut Option<String>, &'a mut KeyStorage)> {
    match t.kind {
        TargetKind::Provider => cfg
            .providers
            .iter_mut()
            .find(|p| p.id == t.id)
            .map(|p| (&mut p.api_key, &mut p.key_storage)),
    }
}

// ───────────────────────────── Trạng thái ─────────────────────────────

#[derive(Clone, Debug)]
struct Issue {
    code: &'static str,
    message: String,
}

impl Issue {
    fn new(context: &str, e: &SecretError) -> Self {
        Self { code: e.code(), message: format!("{context}: {e}") }
    }
}

#[derive(Default)]
struct Inner {
    /// Account còn key plaintext trong config.json (migrate chưa thành công).
    legacy: HashSet<String>,
    /// Lỗi gần nhất theo account (hydrate / migrate / lưu key).
    issues: HashMap<String, Issue>,
}

/// Trạng thái 1 key cho UI — KHÔNG chứa key.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct TargetStatus {
    pub target: Target,
    /// Nơi key đang thực sự nằm (key plaintext chưa migrate trong config.json = `file`).
    pub storage: KeyStorage,
    /// false = key vẫn nằm plaintext trong config.json.
    pub migrated: bool,
    pub has_key: bool,
    /// Key đáng lẽ phải có nhưng không có (RAM sau khi khởi động lại, keyring khoá…).
    pub missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `keyring_unavailable` | `keyring_locked` | `storage` | `invalid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SecretStatusReport {
    pub targets: Vec<TargetStatus>,
    /// File backup/hỏng (`config.*.json`, `config.json.*`) còn chứa api_key plaintext.
    pub leftover_files: Vec<String>,
    /// Đường dẫn secrets.json (hiển thị trong hộp thoại đồng ý lưu dạng file).
    pub secrets_file: String,
}

// ───────────────────────────── Secrets ─────────────────────────────

pub struct Secrets {
    store: Arc<dyn SecretStore>,
    root: PathBuf,
    inner: Mutex<Inner>,
    /// Test: giả lập lỗi ở bước "config_write" / "config_verify".
    #[cfg(test)]
    fail_at: Mutex<Option<&'static str>>,
}

impl Secrets {
    pub fn new(store: Arc<dyn SecretStore>, root: &Path) -> Self {
        Self {
            store,
            root: root.to_path_buf(),
            inner: Mutex::new(Inner::default()),
            #[cfg(test)]
            fail_at: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn secrets_path(&self) -> PathBuf {
        self.root.join(SECRETS_FILE)
    }

    fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    #[cfg(test)]
    fn fault(&self, step: &'static str) -> Result<(), SecretError> {
        match *self.fail_at.lock().unwrap() {
            Some(s) if s == step => Err(SecretError::Io(format!("giả lập lỗi ở bước {step}"))),
            _ => Ok(()),
        }
    }

    #[cfg(not(test))]
    fn fault(&self, _: &'static str) -> Result<(), SecretError> {
        Ok(())
    }

    // ── Khởi động ──

    /// Gọi ngay sau `Storage::load_config()`: hydrate key từ keyring / secrets.json rồi migrate
    /// key plaintext còn trong config.json sang keyring. Không bao giờ fail — lỗi ghi vào trạng thái.
    pub fn startup(&self, cfg: &mut AppConfig) {
        let mut inner = self.lock();
        self.load_keys(&mut inner, cfg);
        let legacy: Vec<Target> = targets(cfg).into_iter().filter(|t| inner.legacy.contains(&t.account())).collect();
        for t in legacy {
            match self.migrate_one(&mut inner, cfg, &t) {
                Ok(()) => log::info!("đã chuyển key '{}' từ config.json vào keyring", t.account()),
                Err(e) => {
                    log::warn!("chưa chuyển được key '{}' vào keyring: {e}", t.account());
                    inner.issues.insert(t.account(), Issue::new("Chưa chuyển được key vào keyring", &e));
                }
            }
        }
        drop(inner);
        let left = self.leftover_files();
        if !left.is_empty() {
            log::warn!("{} file backup/hỏng còn chứa API key plaintext", left.len());
        }
    }

    /// Nạp key từ keyring / secrets.json vào `cfg`; key plaintext trong config.json được giữ và đánh dấu `legacy`.
    fn load_keys(&self, inner: &mut Inner, cfg: &mut AppConfig) {
        inner.legacy.clear();
        inner.issues.clear();
        let file = read_file_map(&self.secrets_path()).inspect_err(|e| log::error!("không đọc được {SECRETS_FILE}: {e}"));
        for t in targets(cfg) {
            let acct = t.account();
            let Some((key, storage)) = slot_mut(cfg, &t) else { continue };
            if key.as_deref().is_some_and(|k| !k.trim().is_empty() && !k.starts_with(MASK_PREFIX)) {
                // Key plaintext đọc được từ config.json → cần migrate.
                inner.legacy.insert(acct);
                continue;
            }
            *key = None;
            match *storage {
                KeyStorage::Keyring => match self.store.get(&acct) {
                    Ok(v) => *key = v,
                    Err(e) => {
                        log::warn!("không đọc được key '{acct}' từ keyring: {e}");
                        inner.issues.insert(acct, Issue::new("Không đọc được key từ keyring", &e));
                    }
                },
                KeyStorage::File => match &file {
                    Ok(m) => *key = m.get(&acct).cloned(),
                    Err(e) => {
                        inner.issues.insert(acct, Issue::new("Không đọc được secrets.json", e));
                    }
                },
                KeyStorage::Memory => {}
            }
        }
    }

    /// 1 key plaintext → keyring. Lỗi ở bước nào thì key giữ nguyên trong config.json.
    fn migrate_one(&self, inner: &mut Inner, cfg: &mut AppConfig, t: &Target) -> Result<(), SecretError> {
        let acct = t.account();
        let key = slot(cfg, t)
            .and_then(|(k, _)| k.map(str::to_owned))
            .ok_or_else(|| SecretError::Invalid("không có key để chuyển".into()))?;
        // 1–2. ghi keyring rồi đọc lại so khớp
        self.store_verified(&acct, &key)?;
        // 3–4. ghi config.json không còn key này rồi đọc lại
        let mut next = cfg.clone();
        if let Some((_, storage)) = slot_mut(&mut next, t) {
            *storage = KeyStorage::Keyring;
        }
        let mut legacy = inner.legacy.clone();
        legacy.remove(&acct);
        self.write_config_checked(&next, &legacy)?;
        // 5. giờ mới bỏ bản plaintext
        inner.legacy = legacy;
        inner.issues.remove(&acct);
        *cfg = next;
        Ok(())
    }

    fn store_verified(&self, acct: &str, key: &str) -> Result<(), SecretError> {
        self.store.set(acct, key)?;
        match self.store.get(acct)? {
            Some(k) if k == key => Ok(()),
            _ => Err(SecretError::Unavailable("đọc lại từ keyring không khớp với key vừa ghi".into())),
        }
    }

    // ── Ghi config.json / secrets.json ──

    /// Ghi config.json không chứa key (trừ key plaintext chưa migrate được — giữ nguyên chỗ cũ).
    pub fn save_config(&self, cfg: &AppConfig) -> Result<(), SecretError> {
        let legacy = self.lock().legacy.clone();
        self.write_config(cfg, &legacy)
    }

    fn write_config(&self, cfg: &AppConfig, legacy: &HashSet<String>) -> Result<(), SecretError> {
        self.fault("config_write")?;
        let bytes = config_bytes(cfg, legacy)?;
        write_atomic(&self.config_path(), &bytes, true)?;
        Ok(())
    }

    /// Ghi rồi đọc lại: file phải parse được và chỉ chứa đúng các key plaintext trong `legacy`.
    fn write_config_checked(&self, cfg: &AppConfig, legacy: &HashSet<String>) -> Result<(), SecretError> {
        self.write_config(cfg, legacy)?;
        self.fault("config_verify")?;
        let bytes = fs::read(self.config_path())?;
        let back = AppConfig::from_json(&bytes)?;
        for t in targets(&back) {
            let on_disk = slot(&back, &t).and_then(|(k, _)| k).is_some();
            if on_disk != legacy.contains(&t.account()) {
                return Err(SecretError::Io(format!("config.json đọc lại không khớp ở '{}'", t.account())));
            }
        }
        Ok(())
    }

    /// secrets.json = đúng các key đang ở chế độ `file`. Không có key nào → xoá file.
    fn sync_file(&self, cfg: &AppConfig) -> Result<(), SecretError> {
        let want: BTreeMap<String, String> = targets(cfg)
            .into_iter()
            .filter_map(|t| match slot(cfg, &t) {
                Some((Some(k), KeyStorage::File)) => Some((t.account(), k.to_owned())),
                _ => None,
            })
            .collect();
        let path = self.secrets_path();
        if read_file_map(&path).ok().as_ref() == Some(&want) && (path.exists() || want.is_empty()) {
            return Ok(());
        }
        if want.is_empty() {
            match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        } else {
            write_atomic(&path, &serde_json::to_vec_pretty(&want)?, true)?;
            Ok(())
        }
    }

    // ── save_config ──

    /// Lưu config đã `merge_secrets`: key mới/đổi → nơi lưu của target (keyring mặc định),
    /// key bị xoá → xoá khỏi keyring + secrets.json. Keyring lỗi khi lưu key mới → key chỉ giữ
    /// trong RAM (`memory`) kèm lỗi để UI hỏi người dùng — KHÔNG tự ghi ra file.
    pub fn persist(&self, new: &mut AppConfig, old: &AppConfig) -> Result<(), SecretError> {
        let mut inner = self.lock();
        let mut legacy = inner.legacy.clone();
        let mut issues = inner.issues.clone();
        let mut deletes: Vec<String> = Vec::new();
        let new_targets = targets(new);

        for t in &new_targets {
            let acct = t.account();
            let prev = slot(old, t);
            let prev_key = prev.and_then(|(k, _)| k).map(str::to_owned);
            let Some((key, storage)) = slot_mut(new, t) else { continue };
            if key.as_deref().is_some_and(|k| k.trim().is_empty()) {
                *key = None;
            }
            match key.as_deref() {
                None => {
                    // Chỉ xoá khi trước đó thực sự có key (key chưa hydrate được thì giữ nguyên keyring).
                    if prev_key.is_some() {
                        deletes.push(acct.clone());
                        legacy.remove(&acct);
                        issues.remove(&acct);
                        *storage = KeyStorage::Keyring;
                    }
                }
                Some(k) if Some(k) == prev_key.as_deref() => {}
                Some(k) => {
                    // Key mới thay hẳn key plaintext cũ (nếu có).
                    legacy.remove(&acct);
                    issues.remove(&acct);
                    match *storage {
                        KeyStorage::Keyring => {
                            if let Err(e) = self.store_verified(&acct, k) {
                                log::warn!("không lưu được key '{acct}' vào keyring: {e} → tạm giữ trong RAM");
                                *storage = KeyStorage::Memory;
                                issues.insert(acct, Issue::new("Chưa lưu được key vào keyring, key đang chỉ ở RAM", &e));
                            }
                        }
                        KeyStorage::File => {}
                        KeyStorage::Memory => deletes.push(acct),
                    }
                }
            }
        }
        // Target biến mất (xoá provider, đổi dịch vụ tìm web) → xoá key cũ.
        let alive: HashSet<String> = new_targets.iter().map(Target::account).collect();
        for t in targets(old) {
            let acct = t.account();
            if !alive.contains(&acct) {
                if slot(old, &t).is_some_and(|(k, s)| k.is_some() || s == KeyStorage::Keyring) {
                    deletes.push(acct.clone());
                }
                legacy.remove(&acct);
                issues.remove(&acct);
            }
        }

        self.sync_file(new)?;
        self.write_config(new, &legacy)?;
        inner.legacy = legacy;
        inner.issues = issues;
        drop(inner);
        for acct in deletes {
            if let Err(e) = self.store.delete(&acct) {
                log::warn!("không xoá được key '{acct}' khỏi keyring: {e}");
            }
        }
        Ok(())
    }

    // ── set_secret / delete_secret ──

    /// Lưu key của `target` vào `storage`. `api_key` là giá trị mask = dùng key hiện tại
    /// (chuyển nơi lưu / thử lại migrate / mở khoá keyring). Lỗi → không thay đổi gì.
    pub fn set_secret(
        &self,
        cfg: &mut AppConfig,
        target: &Target,
        api_key: &str,
        storage: KeyStorage,
    ) -> Result<KeyStorage, SecretError> {
        if storage == KeyStorage::File {
            return Err(SecretError::Invalid(
                "TarotCouncil chỉ lưu key trong keyring hoặc RAM — không hỗ trợ lưu ra file".into(),
            ));
        }
        let acct = target.account();
        let (cur, _) = slot(cfg, target).ok_or_else(|| {
            SecretError::Invalid(format!("'{acct}' không có trong cấu hình đã lưu — hãy Lưu cấu hình trước"))
        })?;
        let input = api_key.trim();
        let key = if input.starts_with(MASK_PREFIX) {
            match cur {
                Some(k) => k.to_owned(),
                // Key chưa hydrate được (keyring khoá lúc mở app) → thử đọc lại.
                None if storage == KeyStorage::Keyring => self
                    .store
                    .get(&acct)?
                    .ok_or_else(|| SecretError::Invalid("Chưa có key — hãy nhập key".into()))?,
                None => return Err(SecretError::Invalid("Chưa có key — hãy nhập key".into())),
            }
        } else if input.is_empty() {
            return Err(SecretError::Invalid("Key trống — dùng Xoá key để xoá".into()));
        } else {
            input.to_owned()
        };

        let mut inner = self.lock();
        if storage == KeyStorage::Keyring {
            self.store_verified(&acct, &key)?;
        }
        let mut next = cfg.clone();
        if let Some((k, s)) = slot_mut(&mut next, target) {
            *k = Some(key);
            *s = storage;
        }
        let mut legacy = inner.legacy.clone();
        legacy.remove(&acct);
        self.sync_file(&next)?;
        self.write_config_checked(&next, &legacy)?;
        inner.legacy = legacy;
        inner.issues.remove(&acct);
        drop(inner);
        if storage != KeyStorage::Keyring {
            if let Err(e) = self.store.delete(&acct) {
                log::warn!("không xoá được bản cũ của '{acct}' trong keyring: {e}");
            }
        }
        *cfg = next;
        Ok(storage)
    }

    /// Xoá key ở mọi nơi (keyring, secrets.json, RAM, plaintext trong config.json).
    pub fn delete_secret(&self, cfg: &mut AppConfig, target: &Target) -> Result<(), SecretError> {
        let acct = target.account();
        match self.store.delete(&acct) {
            Ok(()) => {}
            // Không có keyring thì cũng không có gì trong keyring để xoá.
            Err(SecretError::Unavailable(e)) => log::warn!("bỏ qua keyring khi xoá '{acct}': {e}"),
            Err(e) => return Err(e),
        }
        let mut inner = self.lock();
        let mut next = cfg.clone();
        if let Some((k, s)) = slot_mut(&mut next, target) {
            *k = None;
            *s = KeyStorage::Keyring;
        }
        let mut legacy = inner.legacy.clone();
        legacy.remove(&acct);
        self.sync_file(&next)?;
        self.write_config_checked(&next, &legacy)?;
        inner.legacy = legacy;
        inner.issues.remove(&acct);
        *cfg = next;
        Ok(())
    }

    // ── Trạng thái ──

    pub fn status(&self, cfg: &AppConfig) -> SecretStatusReport {
        SecretStatusReport {
            targets: self.target_statuses(cfg),
            leftover_files: self.leftover_files().iter().map(|p| p.display().to_string()).collect(),
            secrets_file: self.secrets_path().display().to_string(),
        }
    }

    fn target_statuses(&self, cfg: &AppConfig) -> Vec<TargetStatus> {
        let inner = self.lock();
        targets(cfg)
            .into_iter()
            .filter_map(|t| {
                let (key, storage) = slot(cfg, &t)?;
                let acct = t.account();
                let legacy = inner.legacy.contains(&acct);
                let issue = inner.issues.get(&acct);
                let has_key = key.is_some();
                Some(TargetStatus {
                    storage: if legacy { KeyStorage::File } else { storage },
                    migrated: !legacy,
                    has_key,
                    missing: !has_key && (storage == KeyStorage::Memory || issue.is_some()),
                    error: issue.map(|i| i.message.clone()),
                    error_code: issue.map(|i| i.code),
                    target: t,
                })
            })
            .collect()
    }

    /// Target đầu tiên trong `wanted` bị thiếu key (RAM sau khởi động lại, keyring khoá…).
    pub fn first_missing(&self, cfg: &AppConfig, wanted: &[Target]) -> Option<Target> {
        let list = self.target_statuses(cfg);
        wanted.iter().find(|w| list.iter().any(|s| &s.target == *w && s.missing)).cloned()
    }

    // ── File sót lại ──

    /// `config.json.*`, `config.*.json` (backup migrate, file hỏng…) và file tạm `config.tmp-*`
    /// còn chứa api_key plaintext. Chỉ báo cáo, không tự xoá.
    pub fn leftover_files(&self) -> Vec<PathBuf> {
        let Ok(dir) = fs::read_dir(&self.root) else { return vec![] };
        let mut out: Vec<PathBuf> = dir
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                name != CONFIG_FILE
                    && (name.starts_with("config.json.")
                        || name.starts_with("config.tmp-")
                        || (name.starts_with("config.") && name.ends_with(".json")))
            })
            .filter(|p| fs::read(p).is_ok_and(|b| contains_plain_key(&b)))
            .collect();
        out.sort();
        out
    }

    /// Chỉ xoá file nằm trong danh sách `leftover_files()` hiện tại. Trả về danh sách còn lại.
    pub fn delete_leftovers(&self, files: &[String]) -> Result<Vec<String>, SecretError> {
        let current = self.leftover_files();
        for f in files {
            match current.iter().find(|p| p.display().to_string() == *f) {
                Some(p) => fs::remove_file(p)?,
                None => log::warn!("bỏ qua file không nằm trong danh sách sót lại: {f}"),
            }
        }
        Ok(self.leftover_files().iter().map(|p| p.display().to_string()).collect())
    }
}

/// JSON của config, chỉ giữ `api_key` plaintext cho account trong `legacy`.
fn config_bytes(cfg: &AppConfig, legacy: &HashSet<String>) -> serde_json::Result<Vec<u8>> {
    let mut v = serde_json::to_value(cfg)?;
    let put = |obj: Option<&mut serde_json::Map<String, Value>>, t: Target| {
        let Some(obj) = obj else { return };
        match slot(cfg, &t).and_then(|(k, _)| k) {
            Some(k) if legacy.contains(&t.account()) => {
                obj.insert("api_key".into(), Value::String(k.to_owned()));
            }
            _ => {
                obj.remove("api_key");
            }
        }
    };
    if let Some(arr) = v.get_mut("providers").and_then(Value::as_array_mut) {
        for (p, item) in cfg.providers.iter().zip(arr.iter_mut()) {
            put(item.as_object_mut(), Target::provider(&p.id));
        }
    }
    serde_json::to_vec_pretty(&v)
}

fn read_file_map(path: &Path) -> Result<BTreeMap<String, String>, SecretError> {
    match fs::read(path) {
        Ok(b) => Ok(serde_json::from_slice(&b)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.into()),
    }
}

/// Có `"api_key": "<khác rỗng, không phải mask>"` ở bất kỳ đâu. File không phải JSON (hỏng)
/// thì dò theo text.
fn contains_plain_key(bytes: &[u8]) -> bool {
    fn walk(v: &Value) -> bool {
        match v {
            Value::Object(m) => m.iter().any(|(k, v)| {
                (k == "api_key"
                    && v.as_str().is_some_and(|s| !s.trim().is_empty() && !s.starts_with(MASK_PREFIX)))
                    || walk(v)
            }),
            Value::Array(a) => a.iter().any(walk),
            _ => false,
        }
    }
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        return walk(&v);
    }
    let text = String::from_utf8_lossy(bytes);
    text.match_indices("\"api_key\"").any(|(i, m)| {
        let rest = text[i + m.len()..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else { return false };
        let Some(val) = rest.trim_start().strip_prefix('"') else { return false };
        !val.starts_with('"') && !val.starts_with(MASK_PREFIX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Provider;
    use crate::storage::Storage;

    /// Kho giả trong RAM; có thể giả lập lỗi từng thao tác.
    #[derive(Default)]
    struct FakeStore {
        map: Mutex<HashMap<String, String>>,
        fail_set: Mutex<Option<SecretError>>,
        fail_get: Mutex<Option<SecretError>>,
        /// Đọc lại trả giá trị sai (giả lập keyring ghi hỏng).
        corrupt_readback: Mutex<bool>,
    }

    impl SecretStore for FakeStore {
        fn get(&self, a: &str) -> Result<Option<String>, SecretError> {
            if let Some(e) = self.fail_get.lock().unwrap().clone() {
                return Err(e);
            }
            if *self.corrupt_readback.lock().unwrap() {
                return Ok(Some("rác".into()));
            }
            Ok(self.map.lock().unwrap().get(a).cloned())
        }
        fn set(&self, a: &str, s: &str) -> Result<(), SecretError> {
            if let Some(e) = self.fail_set.lock().unwrap().clone() {
                return Err(e);
            }
            self.map.lock().unwrap().insert(a.into(), s.into());
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<(), SecretError> {
            self.map.lock().unwrap().remove(a);
            Ok(())
        }
    }

    impl FakeStore {
        fn val(&self, a: &str) -> Option<String> {
            self.map.lock().unwrap().get(a).cloned()
        }
    }

    struct Env {
        dir: PathBuf,
        storage: Storage,
        fake: Arc<FakeStore>,
        secrets: Secrets,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn env() -> Env {
        let dir = std::env::temp_dir().join(format!("tarotcouncil-secrets-{}", uuid::Uuid::new_v4()));
        let storage = Storage::with_root(dir.clone()).unwrap();
        let fake = Arc::new(FakeStore::default());
        let secrets = Secrets::new(fake.clone(), &dir);
        Env { dir, storage, fake, secrets }
    }

    impl Env {
        fn config_text(&self) -> String {
            fs::read_to_string(self.dir.join(CONFIG_FILE)).unwrap()
        }
        /// Ghi config.json kiểu cũ (key plaintext) rồi mở app.
        fn boot_legacy(&self) -> AppConfig {
            fs::write(
                self.dir.join(CONFIG_FILE),
                br#"{"schema_version":1,
                    "providers":[
                        {"id":"a","name":"A","base_url":"https://a.example/v1","api_key":"sk-aaaa-11111111"},
                        {"id":"b","name":"B","base_url":"https://b.example/v1","api_key":"sk-bbbb-22222222"}]}"#,
            )
            .unwrap();
            self.boot()
        }
        fn boot(&self) -> AppConfig {
            let mut cfg = self.storage.load_config();
            self.secrets.startup(&mut cfg);
            cfg
        }
        fn status_of(&self, cfg: &AppConfig, t: &Target) -> TargetStatus {
            self.secrets.status(cfg).targets.into_iter().find(|s| &s.target == t).unwrap()
        }
    }

    fn pa() -> Target {
        Target::provider("a")
    }

    fn with_provider(id: &str) -> AppConfig {
        let mut c = AppConfig::default();
        c.providers.push(Provider {
            id: id.into(),
            name: id.to_uppercase(),
            base_url: format!("https://{id}.example/v1"),
            ..Provider::default()
        });
        c
    }

    #[test]
    fn migration_moves_every_key_into_keyring() {
        let e = env();
        let cfg = e.boot_legacy();
        assert_eq!(e.fake.val("provider:a").as_deref(), Some("sk-aaaa-11111111"));
        assert_eq!(e.fake.val("provider:b").as_deref(), Some("sk-bbbb-22222222"));
        // Runtime vẫn có key → llm_client/health không cần đổi.
        assert_eq!(cfg.resolve_key("a"), Some("sk-aaaa-11111111"));
        let text = e.config_text();
        assert!(!text.contains("api_key") && !text.contains("sk-"), "{text}");
        let s = e.status_of(&cfg, &pa());
        assert_eq!((s.storage, s.migrated, s.has_key, s.missing), (KeyStorage::Keyring, true, true, false));
        assert!(s.error.is_none());

        // Mở lại app: hydrate từ keyring.
        let again = e.boot();
        assert_eq!(again.resolve_key("b"), Some("sk-bbbb-22222222"));
    }

    #[test]
    fn migration_failure_on_keyring_write_keeps_plaintext_and_app_working() {
        let e = env();
        *e.fake.fail_set.lock().unwrap() = Some(SecretError::Unavailable("no dbus".into()));
        let cfg = e.boot_legacy();
        assert_eq!(cfg.resolve_key("a"), Some("sk-aaaa-11111111"));
        let text = e.config_text();
        assert!(text.contains("sk-aaaa-11111111"));
        assert!(!e.dir.join(SECRETS_FILE).exists(), "không được tự fallback ra file");
        let s = e.status_of(&cfg, &pa());
        assert_eq!((s.storage, s.migrated, s.has_key), (KeyStorage::File, false, true));
        assert_eq!(s.error_code, Some("keyring_unavailable"));

        // Lưu config khác (không đổi key) vẫn giữ key plaintext ở chỗ cũ.
        let mut next = cfg.redacted().merge_secrets(&cfg);
        next.max_retries = 5;
        e.secrets.persist(&mut next, &cfg).unwrap();
        assert!(e.config_text().contains("sk-aaaa-11111111"));

        // Thử lại khi keyring đã có: set_secret với giá trị mask.
        *e.fake.fail_set.lock().unwrap() = None;
        let mut cfg = next;
        let masked = cfg.redacted().providers[0].api_key.clone().unwrap();
        assert_eq!(e.secrets.set_secret(&mut cfg, &pa(), &masked, KeyStorage::Keyring).unwrap(), KeyStorage::Keyring);
        assert_eq!(e.fake.val("provider:a").as_deref(), Some("sk-aaaa-11111111"));
        let text = e.config_text();
        assert!(!text.contains("sk-aaaa") && text.contains("sk-bbbb"), "chỉ key a được chuyển: {text}");
        assert!(e.status_of(&cfg, &pa()).migrated);
    }

    #[test]
    fn migration_failure_on_keyring_readback() {
        let e = env();
        *e.fake.corrupt_readback.lock().unwrap() = true;
        let cfg = e.boot_legacy();
        assert!(e.config_text().contains("sk-aaaa-11111111"));
        assert_eq!(cfg.resolve_key("a"), Some("sk-aaaa-11111111"));
        assert!(!e.status_of(&cfg, &pa()).migrated);
    }

    #[test]
    fn migration_failure_on_config_write_and_verify() {
        for step in ["config_write", "config_verify"] {
            let e = env();
            *e.secrets.fail_at.lock().unwrap() = Some(step);
            let cfg = e.boot_legacy();
            assert_eq!(cfg.resolve_key("a"), Some("sk-aaaa-11111111"), "{step}");
            let s = e.status_of(&cfg, &pa());
            assert!(!s.migrated && s.error.as_deref().unwrap().contains(step), "{step}: {s:?}");
            if step == "config_write" {
                assert!(e.config_text().contains("sk-aaaa-11111111"));
            }
            // Lần lưu sau vẫn ghi lại key plaintext (giữ chỗ cũ) vì chưa migrate xong.
            *e.secrets.fail_at.lock().unwrap() = None;
            e.secrets.save_config(&cfg).unwrap();
            assert!(e.config_text().contains("sk-aaaa-11111111"), "{step}");
            // Khởi động lại → migrate lần nữa, lần này thành công.
            let again = e.boot();
            assert!(e.status_of(&again, &pa()).migrated);
            assert!(!e.config_text().contains("sk-aaaa"));
        }
    }

    #[test]
    fn save_config_keeps_masked_deletes_empty_and_stores_new_keys() {
        let e = env();
        let mut cfg = e.boot_legacy();

        // Webview gửi lại bản mask → giữ key; "" → xoá; key mới → keyring.
        let mut incoming = cfg.redacted();
        incoming.providers[1].api_key = Some(String::new());
        let mut merged = incoming.merge_secrets(&cfg);
        e.secrets.persist(&mut merged, &cfg).unwrap();
        cfg = merged;
        assert_eq!(cfg.resolve_key("a"), Some("sk-aaaa-11111111"));
        assert_eq!(e.fake.val("provider:a").as_deref(), Some("sk-aaaa-11111111"));
        assert_eq!(cfg.resolve_key("b"), None);
        assert_eq!(e.fake.val("provider:b"), None);
        assert!(!e.config_text().contains("api_key"));

        // Xoá provider → xoá luôn key trong keyring.
        let mut incoming = cfg.redacted();
        incoming.providers.retain(|p| p.id != "a");
        let mut merged = incoming.merge_secrets(&cfg);
        e.secrets.persist(&mut merged, &cfg).unwrap();
        assert_eq!(e.fake.val("provider:a"), None);
    }

    #[test]
    fn hydrate_failure_does_not_delete_keyring_entry_on_save() {
        let e = env();
        let cfg = e.boot_legacy();
        *e.fake.fail_get.lock().unwrap() = Some(SecretError::Locked("user canceled".into()));
        let locked = e.boot();
        assert_eq!(locked.resolve_key("a"), None);
        let s = e.status_of(&locked, &pa());
        assert!(s.missing && s.error_code == Some("keyring_locked"), "{s:?}");
        // Webview không có key (null) → lưu config không được xoá key trong keyring.
        let mut merged = locked.redacted().merge_secrets(&locked);
        e.secrets.persist(&mut merged, &locked).unwrap();
        assert_eq!(e.fake.val("provider:a").as_deref(), Some("sk-aaaa-11111111"));
        // Mở khoá / thử lại: set_secret với mask đọc lại từ keyring.
        *e.fake.fail_get.lock().unwrap() = None;
        let mut cfg2 = merged;
        e.secrets.set_secret(&mut cfg2, &pa(), MASK_PREFIX, KeyStorage::Keyring).unwrap();
        assert_eq!(cfg2.resolve_key("a"), cfg.resolve_key("a"));
        assert!(!e.status_of(&cfg2, &pa()).missing);
    }

    #[test]
    fn new_key_with_keyring_down_stays_in_memory_until_user_chooses() {
        let e = env();
        let old = with_provider("a");
        e.secrets.save_config(&old).unwrap();
        *e.fake.fail_set.lock().unwrap() = Some(SecretError::Unavailable("no backend".into()));
        let mut incoming = old.redacted();
        incoming.providers[0].api_key = Some("sk-fresh-12345678".into());
        let mut cfg = incoming.merge_secrets(&old);
        e.secrets.persist(&mut cfg, &old).unwrap();
        assert_eq!(cfg.resolve_key("a"), Some("sk-fresh-12345678"));
        assert!(!e.config_text().contains("sk-fresh"));
        assert!(!e.dir.join(SECRETS_FILE).exists());
        let s = e.status_of(&cfg, &pa());
        assert_eq!((s.storage, s.error_code), (KeyStorage::Memory, Some("keyring_unavailable")));

        // set_secret keyring → trả lỗi keyring_unavailable, không đổi gì.
        let err = e.secrets.set_secret(&mut cfg, &pa(), MASK_PREFIX, KeyStorage::Keyring).unwrap_err();
        assert_eq!(err.code(), "keyring_unavailable");
        assert_eq!(cfg.providers[0].key_storage, KeyStorage::Memory);
    }

    #[test]
    fn memory_mode_is_lost_after_restart_and_reported_missing() {
        let e = env();
        let mut cfg = with_provider("a");
        e.secrets.save_config(&cfg).unwrap();
        e.secrets.set_secret(&mut cfg, &pa(), "sk-ram-12345678", KeyStorage::Memory).unwrap();
        assert_eq!(cfg.resolve_key("a"), Some("sk-ram-12345678"));
        assert!(!e.config_text().contains("sk-ram") && e.fake.val("provider:a").is_none());
        assert!(!e.dir.join(SECRETS_FILE).exists());
        assert!(e.secrets.first_missing(&cfg, &[pa()]).is_none());

        let after = e.boot();
        assert_eq!(after.providers[0].key_storage, KeyStorage::Memory);
        assert_eq!(after.resolve_key("a"), None);
        assert_eq!(e.secrets.first_missing(&after, &[pa()]), Some(pa()));
        let s = e.status_of(&after, &pa());
        assert_eq!((s.storage, s.missing), (KeyStorage::Memory, true));
    }

    /// Plan §2 bước 3: chỉ còn keyring + RAM; nhánh `keyring_locked` dùng RAM cho phiên hiện tại.
    #[test]
    fn file_mode_is_rejected_and_locked_keyring_falls_back_to_memory() {
        let e = env();
        let mut cfg = with_provider("a");
        e.secrets.save_config(&cfg).unwrap();
        let err = e.secrets.set_secret(&mut cfg, &pa(), "sk-file-12345678", KeyStorage::File).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(!e.dir.join(SECRETS_FILE).exists());
        assert_eq!(cfg.resolve_key("a"), None);

        // Keyring khoá → lỗi keyring_locked, UI chuyển sang RAM.
        *e.fake.fail_set.lock().unwrap() = Some(SecretError::Locked("user canceled".into()));
        let err = e.secrets.set_secret(&mut cfg, &pa(), "sk-ram-12345678", KeyStorage::Keyring).unwrap_err();
        assert_eq!(err.code(), "keyring_locked");
        assert_eq!(e.secrets.set_secret(&mut cfg, &pa(), "sk-ram-12345678", KeyStorage::Memory).unwrap(), KeyStorage::Memory);
        assert_eq!(cfg.resolve_key("a"), Some("sk-ram-12345678"));
        assert!(!e.config_text().contains("sk-ram") && !e.dir.join(SECRETS_FILE).exists());
    }

    #[test]
    fn delete_secret_clears_keyring_file_and_plaintext() {
        let e = env();
        *e.fake.fail_set.lock().unwrap() = Some(SecretError::Locked("denied".into()));
        let mut cfg = e.boot_legacy();
        assert!(e.config_text().contains("sk-aaaa"));
        *e.fake.fail_set.lock().unwrap() = None;
        e.secrets.delete_secret(&mut cfg, &pa()).unwrap();
        assert!(!e.config_text().contains("sk-aaaa") && e.config_text().contains("sk-bbbb"));
        let s = e.status_of(&cfg, &pa());
        assert!(!s.has_key && s.migrated && s.error.is_none());
    }

    #[test]
    fn set_secret_rejects_unknown_target_and_empty_key() {
        let e = env();
        let mut cfg = with_provider("a");
        assert_eq!(
            e.secrets.set_secret(&mut cfg, &Target::provider("ghost"), "sk-x", KeyStorage::Keyring).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(e.secrets.set_secret(&mut cfg, &pa(), "  ", KeyStorage::Keyring).unwrap_err().code(), "invalid");
    }

    #[test]
    fn leftover_backup_and_corrupt_files_are_reported_not_deleted() {
        let e = env();
        // Key plaintext trong config.json → migrate sang keyring; bản backup cũ còn key → báo cáo.
        fs::write(
            e.dir.join(CONFIG_FILE),
            br#"{"providers":[{"id":"default","name":"OpenAI","base_url":"https://api.openai.com/v1","api_key":"sk-old-12345678"}]}"#,
        )
        .unwrap();
        fs::write(e.dir.join("config.pre-migration-backup.json"), br#"{"api_key":"sk-old-12345678"}"#).unwrap();
        let cfg = e.boot();
        assert_eq!(cfg.providers[0].api_key.as_deref(), Some("sk-old-12345678"));
        assert!(!e.config_text().contains("sk-old"));
        fs::write(e.dir.join("config.corrupt-20260101.json"), b"{\"providers\":[{\"api_key\": \"sk-x-1\"").unwrap();
        fs::write(e.dir.join("config.json.bak"), r#"{"api_key":"••••1234"}"#).unwrap();
        fs::write(e.dir.join("config.other.json"), br#"{"api_key":null}"#).unwrap();
        let left = e.secrets.status(&cfg).leftover_files;
        let names: Vec<&str> = left.iter().map(|p| Path::new(p).file_name().unwrap().to_str().unwrap()).collect();
        assert_eq!(names, ["config.corrupt-20260101.json", "config.pre-migration-backup.json"]);
        assert!(e.dir.join("config.pre-migration-backup.json").exists());

        // Chỉ xoá file nằm trong danh sách.
        let outside = e.dir.join("readings").display().to_string();
        let remaining = e.secrets.delete_leftovers(&[left[0].clone(), outside]).unwrap();
        assert_eq!(remaining, vec![left[1].clone()]);
        assert!(e.dir.join("readings").exists());
    }

    #[test]
    fn keyring_errors_are_classified() {
        let c = |na, m: &str| classify(na, m.into()).code();
        assert_eq!(c(false, "Platform secure storage failure: org.freedesktop.DBus.Error.ServiceUnknown"), "keyring_unavailable");
        assert_eq!(c(false, "Platform secure storage failure: User canceled the operation."), "keyring_locked");
        assert_eq!(c(true, "Couldn't access platform secure storage: SecretService: object locked"), "keyring_locked");
        assert_eq!(c(true, "Couldn't access platform secure storage: errSecNoSuchKeychain (-25294)"), "keyring_unavailable");
        assert_eq!(c(true, "A specified logon session does not exist"), "keyring_unavailable");
        assert_eq!(c(false, "something odd"), "keyring_unavailable");
    }

    impl AppConfig {
        fn resolve_key(&self, provider_id: &str) -> Option<&str> {
            self.provider(provider_id).and_then(|p| p.api_key.as_deref())
        }
    }
}
