//! Bản ghi 1 lượt đọc (plan §4 — Lưu trữ): `{app_data}/tarotcouncil/readings/{id}.json`.
//! JSON là nguồn sự thật; FTS chỉ là dữ liệu phụ (xem `journal.rs`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{CallUsage, TokenUsage};
use crate::validate::{ReaderOutput, Synthesis};

pub const READING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    #[default]
    Drawing,
    Drawn,
    Interpreting,
    Done,
    Failed,
    Cancelled,
    Support,
}

impl ReadingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Drawing => "drawing",
            Self::Drawn => "drawn",
            Self::Interpreting => "interpreting",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Support => "support",
        }
    }
}

/// Snapshot đầy đủ của 1 lá đã bốc (tên + nhãn vị trí) — đổi bản dịch ở bản sau không làm sai lượt cũ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Draw {
    pub position: u8,
    pub position_label_vi: String,
    /// Chỉ số CỐ ĐỊNH trong `deck_order` (0–77).
    pub fan_index: u8,
    pub card_id: String,
    pub name_vi: String,
    pub name_en: String,
    pub reversed: bool,
    pub art_key: String,
}

/// Nhân vật tham gia (ảnh chụp lúc tạo lượt đọc → Nhật ký hiển thị đúng dù persona đã đổi).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CastMember {
    pub id: String,
    pub name: String,
    pub title: String,
    pub color: String,
    pub icon: String,
    pub model: String,
    pub level: crate::config::RoleLevel,
    pub appearance: crate::config::Appearance,
}

impl CastMember {
    pub fn of(r: &crate::config::RoleDef) -> Self {
        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            title: r.title.clone(),
            color: r.color.clone(),
            icon: r.icon.clone(),
            model: r.model.clone(),
            level: r.level,
            appearance: r.appearance.clone(),
        }
    }
}

/// Kết quả của 1 thầy bói trong attempt hiện tại.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReaderResult {
    pub reader_id: String,
    pub output: Option<ReaderOutput>,
    /// Lý do thất bại (UI hiện nghĩa gốc tĩnh thay thế).
    pub failed: Option<String>,
    /// Tín hiệu bổ trợ (vd nhắc tên lá ngoài lượt bốc) — không quyết định đạt/trượt.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    pub schema_version: u32,
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub app_version: String,
    pub deck_version: u32,

    pub question: String,
    pub spread_id: String,
    pub reader_ids: Vec<String>,
    pub chief_reader_id: String,
    pub cast: Vec<CastMember>,
    /// Nhóm nhạy cảm của câu hỏi (y tế / tài chính / pháp lý) → disclaimer bắt buộc.
    pub safety_flags: Vec<crate::safety::Category>,

    // ── Tráo và bốc (KHÔNG gửi lên UI — xem `public_view`) ──
    pub seed_hex: String,
    pub shuffle_algo_version: u32,
    pub reversed_enabled: bool,
    pub deck_order: Vec<u8>,
    pub reversed_flags: Vec<bool>,
    pub picks: Vec<u8>,
    pub draws: Vec<Draw>,

    // ── Luận giải ──
    pub attempt_id: u32,
    pub outputs: Vec<ReaderResult>,
    pub synthesis: Option<Synthesis>,
    pub usage: TokenUsage,
    pub calls: Vec<CallUsage>,
    pub status: ReadingStatus,
    pub error: Option<String>,
}

impl Default for Reading {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            schema_version: READING_SCHEMA_VERSION,
            id: Uuid::nil(),
            created_at: now,
            updated_at: now,
            app_version: env!("CARGO_PKG_VERSION").into(),
            deck_version: 0,
            question: String::new(),
            spread_id: String::new(),
            reader_ids: vec![],
            chief_reader_id: String::new(),
            cast: vec![],
            safety_flags: vec![],
            seed_hex: String::new(),
            shuffle_algo_version: 0,
            reversed_enabled: true,
            deck_order: vec![],
            reversed_flags: vec![],
            picks: vec![],
            draws: vec![],
            attempt_id: 0,
            outputs: vec![],
            synthesis: None,
            usage: TokenUsage::default(),
            calls: vec![],
            status: ReadingStatus::Drawing,
            error: None,
        }
    }
}

impl Reading {
    /// Bản gửi lên UI: lược bỏ `seed_hex`, `deck_order`, `reversed_flags` (plan §4).
    pub fn public_view(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            for k in ["seed_hex", "deck_order", "reversed_flags"] {
                o.remove(k);
            }
        }
        v
    }

    pub fn card_names(&self) -> String {
        self.draws.iter().map(|d| format!("{} {}", d.name_vi, d.name_en)).collect::<Vec<_>>().join(" · ")
    }

    pub fn summary_text(&self) -> String {
        self.synthesis.as_ref().map(|s| s.summary.clone()).unwrap_or_default()
    }

    pub fn summary(&self) -> ReadingSummary {
        ReadingSummary {
            id: self.id,
            created_at: self.created_at,
            question: self.question.clone(),
            spread_id: self.spread_id.clone(),
            status: self.status,
            cards: self.draws.iter().map(|d| (d.name_vi.clone(), d.reversed)).collect(),
            summary: self.summary_text(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingSummary {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub question: String,
    pub spread_id: String,
    pub status: ReadingStatus,
    /// (tên lá, ngược?)
    pub cards: Vec<(String, bool)>,
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_view_hides_shuffle_secrets() {
        let r = Reading { seed_hex: "ab".repeat(32), deck_order: vec![1, 2], reversed_flags: vec![true], ..Reading::default() };
        let v = r.public_view();
        for k in ["seed_hex", "deck_order", "reversed_flags"] {
            assert!(v.get(k).is_none(), "{k}");
        }
        assert_eq!(v["status"], "drawing");
    }

    #[test]
    fn tolerant_loader_fills_defaults() {
        let r: Reading = serde_json::from_str(r#"{"id":"00000000-0000-0000-0000-000000000001","question":"q","status":"done","future_field":1}"#).unwrap();
        assert_eq!(r.status, ReadingStatus::Done);
        assert!(r.draws.is_empty() && r.synthesis.is_none());
    }
}
