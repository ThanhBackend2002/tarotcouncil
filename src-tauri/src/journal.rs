//! Nhật ký lượt đọc (plan §3/§4 — Lưu trữ).
//!
//! - **JSON là nguồn sự thật**: `readings/{id}.json`, ghi atomic. FTS cập nhật SAU khi ghi JSON;
//!   FTS lỗi chỉ ghi log.
//! - Bảng `readings_fts(question, summary, card_names, status UNINDEXED, created_at UNINDEXED)` +
//!   các cột UNINDEXED để liệt kê mà không phải đọc file. Cột tìm kiếm chứa văn bản đã bỏ dấu
//!   (`normalize::fold`) → gõ không dấu vẫn tìm được.
//! - Khởi động: `sync` dựng lại index nếu thiếu/lệch; `recover_interrupted` query cột `status`
//!   (không quét toàn bộ file) để đưa lượt `interpreting` về `drawn`.
//!
//! Mọi hàm là **blocking** — caller gọi qua `spawn_blocking`.

use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::normalize::{fold, fold_query};
use crate::reading::{Reading, ReadingStatus, ReadingSummary};
use crate::storage::{write_atomic, Storage, StorageError};

const INDEX_FILE: &str = "journal.sqlite";

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("chỉ mục tìm kiếm chưa sẵn sàng")]
    Unavailable,
    #[error("lỗi chỉ mục: {0}")]
    Index(String),
}

impl From<rusqlite::Error> for JournalError {
    fn from(e: rusqlite::Error) -> Self {
        JournalError::Index(e.to_string())
    }
}

pub struct Journal {
    storage: Storage,
    index: Option<Mutex<Connection>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub rebuilt: bool,
    pub indexed: usize,
    pub failed: Vec<String>,
}

fn ts(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn open_index(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE VIRTUAL TABLE IF NOT EXISTS readings_fts USING fts5(
             id UNINDEXED, question, summary, card_names,
             status UNINDEXED, created_at UNINDEXED, updated_at UNINDEXED, summary_json UNINDEXED,
             tokenize = 'unicode61'
         );",
    )?;
    Ok(c)
}

impl Journal {
    pub fn open(storage: Storage) -> Self {
        let index = match open_index(&storage.root().join(INDEX_FILE)) {
            Ok(c) => Some(Mutex::new(c)),
            Err(e) => {
                log::error!("không mở được chỉ mục Nhật ký ({e}); dùng chế độ quét file");
                None
            }
        };
        Self { storage, index }
    }

    fn idx(&self) -> Option<MutexGuard<'_, Connection>> {
        self.index.as_ref().map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
    }

    // ── JSON (nguồn sự thật) ──

    pub fn save(&self, r: &Reading) -> Result<(), StorageError> {
        let bytes = serde_json::to_vec_pretty(r)?;
        write_atomic(&self.storage.reading_path(r.id), &bytes, false)?;
        if let Some(c) = self.idx() {
            if let Err(e) = upsert(&c, r) {
                log::warn!("cập nhật FTS cho {} thất bại: {e}", r.id);
            }
        }
        Ok(())
    }

    pub fn load(&self, id: Uuid) -> Result<Reading, StorageError> {
        match fs::read(self.storage.reading_path(id)) {
            Ok(b) => Ok(serde_json::from_slice(&b)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(StorageError::NotFound(id)),
            Err(e) => Err(e.into()),
        }
    }

    /// Xoá file JSON + dòng FTS tương ứng.
    pub fn delete(&self, id: Uuid) -> Result<bool, StorageError> {
        let existed = match fs::remove_file(self.storage.reading_path(id)) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        if let Some(c) = self.idx() {
            if let Err(e) = c.execute("DELETE FROM readings_fts WHERE id = ?1", params![id.to_string()]) {
                log::warn!("xoá dòng FTS {id} thất bại: {e}");
            }
        }
        Ok(existed)
    }

    /// Đọc mọi file (bỏ qua file hỏng + ghi log).
    fn scan_files(&self) -> Vec<Reading> {
        let Ok(entries) = fs::read_dir(self.storage.readings_dir()) else { return vec![] };
        let mut out = Vec::new();
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match fs::read(&path).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice::<Reading>(&b).map_err(|e| e.to_string())) {
                Ok(r) if self.storage.reading_path(r.id) == path => out.push(r),
                Ok(_) => log::warn!("bỏ qua {}: tên file không khớp id", path.display()),
                Err(e) => log::warn!("bỏ qua lượt đọc hỏng {}: {e}", path.display()),
            }
        }
        out
    }

    // ── Liệt kê / tìm kiếm ──

    pub fn list(&self, limit: usize) -> Vec<ReadingSummary> {
        if let Some(c) = self.idx() {
            match query_summaries(&c, "SELECT summary_json FROM readings_fts ORDER BY created_at DESC LIMIT ?1", params![limit as i64]) {
                Ok(v) => return v,
                Err(e) => log::warn!("list qua FTS lỗi ({e}); quét file"),
            }
        }
        let mut v: Vec<_> = self.scan_files().iter().map(Reading::summary).collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.created_at));
        v.truncate(limit);
        v
    }

    /// Tìm không phân biệt dấu / hoa thường trong câu hỏi, tổng hợp và tên lá. Câu rỗng → [].
    pub fn search(&self, q: &str, limit: usize) -> Result<Vec<ReadingSummary>, JournalError> {
        let fq = fold_query(q);
        if fq.is_empty() {
            return Ok(vec![]);
        }
        let c = self.idx().ok_or(JournalError::Unavailable)?;
        query_summaries(
            &c,
            "SELECT summary_json FROM readings_fts WHERE readings_fts MATCH ?1 ORDER BY created_at DESC LIMIT ?2",
            params![fq, limit as i64],
        )
    }

    // ── Khởi động ──

    /// Dựng lại index từ file nếu thiếu hoặc lệch (id / updated_at khác nhau).
    pub fn sync(&self) -> SyncReport {
        let Some(c) = self.idx() else { return SyncReport::default() };
        let files = self.scan_files();
        let want: BTreeMap<String, String> = files.iter().map(|r| (r.id.to_string(), ts(&r.updated_at))).collect();
        let have: BTreeMap<String, String> = c
            .prepare("SELECT id, updated_at FROM readings_fts")
            .and_then(|mut st| st.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect())
            .unwrap_or_default();
        if want == have {
            return SyncReport { rebuilt: false, indexed: 0, failed: vec![] };
        }
        let mut report = SyncReport { rebuilt: true, ..SyncReport::default() };
        if let Err(e) = c.execute("DELETE FROM readings_fts", []) {
            report.failed.push(e.to_string());
            return report;
        }
        for r in &files {
            match upsert(&c, r) {
                Ok(()) => report.indexed += 1,
                Err(e) => report.failed.push(format!("{}: {e}", r.id)),
            }
        }
        report
    }

    /// Lượt `interpreting` còn sót (app tắt giữa chừng) → `drawn`, bỏ output dở dang. Trả id đã sửa.
    pub fn recover_interrupted(&self) -> Vec<Uuid> {
        let ids: Vec<Uuid> = match self.idx() {
            Some(c) => c
                .prepare("SELECT id FROM readings_fts WHERE status = 'interpreting'")
                .and_then(|mut st| st.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|s| Uuid::parse_str(&s).ok())
                .collect(),
            None => self.scan_files().into_iter().filter(|r| r.status == ReadingStatus::Interpreting).map(|r| r.id).collect(),
        };
        let mut fixed = Vec::new();
        for id in ids {
            let Ok(mut r) = self.load(id) else { continue };
            if r.status != ReadingStatus::Interpreting {
                continue;
            }
            r.status = ReadingStatus::Drawn;
            r.outputs.clear();
            r.synthesis = None;
            r.error = Some("Ứng dụng đã đóng khi đang luận giải — bấm Luận giải để chạy lại".into());
            r.updated_at = Utc::now();
            match self.save(&r) {
                Ok(()) => fixed.push(id),
                Err(e) => log::warn!("không khôi phục được {id}: {e}"),
            }
        }
        fixed
    }

    /// Bản đọc nhanh cho test: map id → status trong index.
    #[cfg(test)]
    fn index_statuses(&self) -> HashMap<String, String> {
        let c = self.idx().unwrap();
        let mut st = c.prepare("SELECT id, status FROM readings_fts").unwrap();
        st.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().map(Result::unwrap).collect()
    }
}

fn upsert(c: &Connection, r: &Reading) -> rusqlite::Result<()> {
    let id = r.id.to_string();
    let summary = r.summary();
    c.execute("DELETE FROM readings_fts WHERE id = ?1", params![id])?;
    c.execute(
        "INSERT INTO readings_fts (id, question, summary, card_names, status, created_at, updated_at, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            fold(&r.question).0,
            fold(&r.summary_text()).0,
            fold(&r.card_names()).0,
            r.status.as_str(),
            ts(&r.created_at),
            ts(&r.updated_at),
            serde_json::to_string(&summary).unwrap_or_default(),
        ],
    )?;
    Ok(())
}

fn query_summaries(c: &Connection, sql: &str, p: impl rusqlite::Params) -> Result<Vec<ReadingSummary>, JournalError> {
    let mut st = c.prepare(sql)?;
    let rows = st.query_map(p, |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for json in rows {
        match serde_json::from_str(&json?) {
            Ok(s) => out.push(s),
            Err(e) => log::warn!("summary_json hỏng trong FTS: {e}"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reading::Draw;
    use crate::storage::tests::tmp_storage;
    use crate::validate::Synthesis;

    fn reading(q: &str, status: ReadingStatus, minutes_ago: i64) -> Reading {
        let t = Utc::now() - chrono::Duration::minutes(minutes_ago);
        Reading {
            id: Uuid::new_v4(),
            question: q.into(),
            status,
            created_at: t,
            updated_at: t,
            spread_id: "one".into(),
            draws: vec![Draw {
                position: 0,
                position_label_vi: "Thông điệp".into(),
                fan_index: 3,
                card_id: "major-17".into(),
                name_vi: "Ngôi Sao".into(),
                name_en: "The Star".into(),
                reversed: false,
                art_key: "m17".into(),
            }],
            ..Reading::default()
        }
    }

    #[test]
    fn save_list_search_delete() {
        let s = tmp_storage();
        let j = Journal::open(s.clone());
        let a = reading("Tình cảm của tôi năm nay?", ReadingStatus::Done, 10);
        let mut b = reading("Công việc mới thế nào", ReadingStatus::Drawn, 5);
        b.synthesis = Some(Synthesis {
            summary: "Thăng tiến nhẹ nhàng".into(),
            agreements: vec![],
            divergences: vec![],
            advice: vec!["x".into()],
            disclaimer: String::new(),
            crisis: false,
        });
        j.save(&a).unwrap();
        j.save(&b).unwrap();
        assert_eq!(j.list(10).iter().map(|x| x.id).collect::<Vec<_>>(), vec![b.id, a.id]);
        // Không dấu, không phân biệt hoa thường, tìm cả trong tổng hợp và tên lá.
        assert_eq!(j.search("TINH CAM", 10).unwrap()[0].id, a.id);
        assert_eq!(j.search("thang tien", 10).unwrap()[0].id, b.id);
        assert_eq!(j.search("ngôi sao", 10).unwrap().len(), 2);
        assert_eq!(j.search("the star", 10).unwrap().len(), 2);
        assert!(j.search("  ", 10).unwrap().is_empty());
        assert!(j.search("\" OR *", 10).is_ok(), "cú pháp FTS của người dùng không gây lỗi");
        assert!(j.delete(a.id).unwrap());
        assert!(!j.delete(a.id).unwrap());
        assert!(j.search("tinh cam", 10).unwrap().is_empty());
        assert!(matches!(j.load(a.id), Err(StorageError::NotFound(_))));
        fs::remove_dir_all(s.root()).ok();
    }

    #[test]
    fn index_is_rebuilt_when_missing_or_stale_and_files_win() {
        let s = tmp_storage();
        let j = Journal::open(s.clone());
        let mut a = reading("hỏi A", ReadingStatus::Done, 3);
        j.save(&a).unwrap();
        assert!(!j.sync().rebuilt);
        // File đổi ngoài luồng (vd crash sau khi ghi JSON, trước khi cập nhật FTS).
        a.status = ReadingStatus::Failed;
        a.updated_at = Utc::now();
        fs::write(s.reading_path(a.id), serde_json::to_vec(&a).unwrap()).unwrap();
        // File hỏng bị bỏ qua.
        fs::write(s.readings_dir().join(format!("{}.json", Uuid::new_v4())), b"{broken").unwrap();
        let rep = j.sync();
        assert!(rep.rebuilt && rep.indexed == 1, "{rep:?}");
        assert_eq!(j.index_statuses()[&a.id.to_string()], "failed");
        // Xoá DB → mở lại → sync dựng lại.
        drop(j);
        fs::remove_file(s.root().join(INDEX_FILE)).unwrap();
        let j = Journal::open(s.clone());
        assert_eq!(j.sync().indexed, 1);
        fs::remove_dir_all(s.root()).ok();
    }

    #[test]
    fn interrupted_readings_go_back_to_drawn_via_index() {
        let s = tmp_storage();
        let j = Journal::open(s.clone());
        let mut r = reading("q", ReadingStatus::Interpreting, 1);
        r.attempt_id = 2;
        r.outputs.push(crate::reading::ReaderResult { reader_id: "classic".into(), output: None, failed: Some("x".into()), warnings: vec![] });
        let done = reading("q2", ReadingStatus::Done, 2);
        j.save(&r).unwrap();
        j.save(&done).unwrap();
        assert_eq!(j.recover_interrupted(), vec![r.id]);
        let back = j.load(r.id).unwrap();
        assert_eq!((back.status, back.outputs.len(), back.attempt_id), (ReadingStatus::Drawn, 0, 2));
        assert!(j.recover_interrupted().is_empty());
        fs::remove_dir_all(s.root()).ok();
    }
}
