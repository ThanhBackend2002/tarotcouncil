//! Persist config + lượt đọc bài dưới dạng JSON file.
//!
//! Vị trí (qua `dirs::config_dir()`), TÁCH HẲN khỏi PlanCouncil:
//! - Linux:   `~/.config/tarotcouncil/`
//! - macOS:   `~/Library/Application Support/tarotcouncil/`
//! - Windows: `%APPDATA%\tarotcouncil\`
//!
//! Mọi hàm ở đây là **blocking** — caller (lib.rs) gọi qua `spawn_blocking`.
//! Ghi file theo kiểu atomic (tmp + fsync + rename) để crash giữa chừng không làm hỏng JSON.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::config::AppConfig;

pub const APP_DIR: &str = "tarotcouncil";
const CONFIG_FILE: &str = "config.json";
const READINGS_DIR: &str = "readings";

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("không xác định được thư mục cấu hình của hệ điều hành")]
    NoConfigDir,
    #[error("lỗi I/O: {0}")]
    Io(#[from] io::Error),
    #[error("lỗi JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("không tìm thấy lượt đọc {0}")]
    NotFound(Uuid),
}

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

/// Thư mục dữ liệu mặc định của app (không tạo thư mục).
pub fn default_root() -> Result<PathBuf, StorageError> {
    Ok(dirs::config_dir().ok_or(StorageError::NoConfigDir)?.join(APP_DIR))
}

impl Storage {
    pub fn new() -> Result<Self, StorageError> {
        Self::with_root(default_root()?)
    }

    pub fn with_root(root: PathBuf) -> Result<Self, StorageError> {
        fs::create_dir_all(root.join(READINGS_DIR))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    /// Thư mục chứa các file `<uuid>.json` của lượt đọc.
    #[allow(dead_code)] // M1: lưu lượt đọc
    pub fn readings_dir(&self) -> PathBuf {
        self.root.join(READINGS_DIR)
    }

    #[allow(dead_code)] // M1: lưu lượt đọc
    pub fn reading_path(&self, id: Uuid) -> PathBuf {
        // `id` là Uuid đã parse → không thể path traversal.
        self.readings_dir().join(format!("{id}.json"))
    }

    /// Không bao giờ fail: file thiếu → default; file hỏng → backup rồi default.
    pub fn load_config(&self) -> AppConfig {
        let path = self.config_path();
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return AppConfig::default(),
            Err(e) => {
                log::error!("không đọc được {}: {e}", path.display());
                return AppConfig::default();
            }
        };
        match AppConfig::from_json(&bytes) {
            Ok(c) => c,
            Err(e) => {
                let backup = path.with_extension(format!("corrupt-{}.json", chrono::Utc::now().format("%Y%m%d%H%M%S")));
                log::error!("config.json hỏng ({e}); backup sang {}", backup.display());
                let _ = fs::rename(&path, &backup);
                AppConfig::default()
            }
        }
    }

    #[cfg_attr(not(test), allow(dead_code))] // app lưu config qua `Secrets::save_config`
    pub fn save_config(&self, cfg: &AppConfig) -> Result<(), StorageError> {
        let bytes = serde_json::to_vec_pretty(cfg)?;
        write_atomic(&self.config_path(), &bytes, true)?;
        Ok(())
    }
}

/// tmp (cùng thư mục) → fsync → rename. `rename` thay thế file đích atomically trên
/// cả ext4/NTFS (Rust dùng `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` trên Windows).
pub fn write_atomic(path: &Path, bytes: &[u8], secret: bool) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4().simple()));
    let result = (|| {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            if secret {
                opts.mode(0o600); // chỉ owner đọc được
            }
        }
        #[cfg(not(unix))]
        let _ = secret; // Windows: %APPDATA% đã có ACL per-user
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn tmp_storage() -> Storage {
        let dir = std::env::temp_dir().join(format!("tarotcouncil-test-{}", Uuid::new_v4()));
        Storage::with_root(dir).unwrap()
    }

    #[test]
    fn config_roundtrip_and_corruption_recovery() {
        let s = tmp_storage();
        let mut c = AppConfig { max_retries: 5, ..Default::default() };
        c.tarot.reversed_enabled = false;
        s.save_config(&c).unwrap();
        let back = s.load_config();
        assert_eq!(back.max_retries, 5);
        assert!(!back.tarot.reversed_enabled);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(s.config_path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        fs::write(s.config_path(), b"{not json").unwrap();
        assert_eq!(s.load_config().max_retries, AppConfig::default().max_retries);
        assert!(!s.config_path().exists()); // đã bị move sang .corrupt-*
        fs::remove_dir_all(s.root()).ok();
    }

    /// Plan §2 bước 5: dữ liệu tách hẳn khỏi PlanCouncil.
    #[test]
    fn data_dir_is_isolated_from_plancouncil() {
        let root = default_root().unwrap();
        assert_eq!(root.file_name().unwrap(), APP_DIR);
        assert!(!root.to_string_lossy().to_lowercase().contains("plancouncil"));

        // Mọi thứ app ghi đều nằm dưới root được cấp (ở đây là HOME tạm).
        let home = std::env::temp_dir().join(format!("tarotcouncil-home-{}", Uuid::new_v4()));
        let s = Storage::with_root(home.join(APP_DIR)).unwrap();
        s.save_config(&AppConfig::default()).unwrap();
        let entries: Vec<_> = fs::read_dir(&home).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(entries, vec![std::ffi::OsString::from(APP_DIR)]);
        fs::remove_dir_all(&home).ok();
    }
}
