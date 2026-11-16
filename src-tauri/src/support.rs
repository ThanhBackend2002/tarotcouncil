//! Nội dung `SupportScreen` (plan §5). Số đường dây cụ thể CHỈ hiển thị khi có trong
//! `assets/support.json` kèm `verified_at`; mục quá 12 tháng chưa xác minh lại bị ẩn.
//! Không mặc định theo quốc gia — luôn ưu tiên hướng dẫn gọi dịch vụ khẩn cấp địa phương.

use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

const SUPPORT_JSON: &str = include_str!("../assets/support.json");
pub const MAX_AGE_DAYS: i64 = 365;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub country: String,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub hours: Option<String>,
    /// YYYY-MM-DD — ngày một người đã gọi/kiểm tra lại thông tin.
    pub verified_at: String,
}

#[derive(Debug, Deserialize)]
struct File {
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupportInfo {
    pub entries: Vec<Entry>,
    /// Số mục bị ẩn vì quá hạn xác minh (để log / hiển thị ghi chú).
    pub hidden_stale: usize,
}

fn fresh(e: &Entry, today: NaiveDate) -> bool {
    NaiveDate::parse_from_str(&e.verified_at, "%Y-%m-%d").is_ok_and(|d| d <= today && today - d <= Duration::days(MAX_AGE_DAYS))
}

pub fn info_at(json: &str, today: NaiveDate) -> SupportInfo {
    let all = serde_json::from_str::<File>(json).map(|f| f.entries).unwrap_or_else(|e| {
        log::error!("support.json không hợp lệ: {e}");
        vec![]
    });
    let total = all.len();
    let entries: Vec<Entry> = all.into_iter().filter(|e| fresh(e, today)).collect();
    SupportInfo { hidden_stale: total - entries.len(), entries }
}

pub fn info() -> SupportInfo {
    info_at(SUPPORT_JSON, Utc::now().date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_or_unverified_entries_are_hidden() {
        let json = r#"{"entries":[
            {"country":"X","name":"Mới","phone":"1","verified_at":"2026-06-01"},
            {"country":"X","name":"Quá hạn","phone":"2","verified_at":"2025-01-01"},
            {"country":"X","name":"Sai ngày","phone":"3","verified_at":"không rõ"},
            {"country":"X","name":"Tương lai","phone":"4","verified_at":"2027-01-01"}]}"#;
        let i = info_at(json, NaiveDate::from_ymd_opt(2026, 11, 17).unwrap());
        assert_eq!(i.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Mới"]);
        assert_eq!(i.hidden_stale, 3);
        assert!(info_at("{broken", NaiveDate::MIN).entries.is_empty());
    }

    #[test]
    fn bundled_file_parses() {
        serde_json::from_str::<File>(SUPPORT_JSON).unwrap();
    }
}
