//! `ReadingEmitter` (plan §2/§3 — Event contract `reading-event`).
//!
//! - Envelope `{reading_id, attempt_id, seq, ts, payload}`.
//! - `seq` tăng đơn điệu theo TỪNG lượt đọc, xuyên suốt mọi attempt, cấp dưới mutex; event được
//!   đẩy ra sink ngay trong lúc giữ mutex → thứ tự emit = thứ tự cấp.
//! - `ReaderProgress` được throttle: tối đa 1 event / 50 ms / thầy bói, trừ khi tăng ≥ 4 KB ký tự
//!   (cơ chế giữ từ `orchestrator.rs:322-349` của PlanCouncil).
//! - `with_seq` cho phép đọc snapshot dưới CÙNG mutex (dùng cho `get_reading`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::reading::ReaderResult;
use crate::validate::Synthesis;

pub const READING_EVENT: &str = "reading-event";
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);
pub const PROGRESS_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload {
    ReaderStarted { reader_id: String },
    ReaderProgress { reader_id: String, chars: u64 },
    ReaderDone { reader_id: String, output: Box<ReaderResult> },
    ReaderFailed { reader_id: String, reason: String },
    SynthesisStarted,
    SynthesisDone { synthesis: Synthesis },
    SupportRequired,
    Usage { tokens: u64, cost_usd: Option<f64> },
    ReadingDone,
    Cancelled,
    Error { code: String, message: String, retryable: bool },
}

#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub reading_id: Uuid,
    pub attempt_id: u32,
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub payload: Payload,
}

pub trait EventSink: Send + Sync {
    fn emit(&self, env: &Envelope);
}

#[derive(Default)]
struct Lane {
    seq: u64,
    /// reader_id → (lần emit progress gần nhất, số ký tự lúc đó)
    progress: HashMap<String, (Instant, u64)>,
}

pub struct ReadingEmitter {
    sink: Arc<dyn EventSink>,
    lanes: Mutex<HashMap<Uuid, Lane>>,
}

impl ReadingEmitter {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self { sink, lanes: Mutex::new(HashMap::new()) }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, Lane>> {
        self.lanes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, lane: &mut Lane, reading_id: Uuid, attempt_id: u32, payload: Payload) -> u64 {
        lane.seq += 1;
        let env = Envelope { reading_id, attempt_id, seq: lane.seq, ts: Utc::now(), payload };
        self.sink.emit(&env);
        lane.seq
    }

    /// Cấp seq + emit. Trả về seq đã cấp.
    pub fn emit(&self, reading_id: Uuid, attempt_id: u32, payload: Payload) -> u64 {
        let mut g = self.lock();
        let lane = g.entry(reading_id).or_default();
        if let Payload::ReaderStarted { reader_id } = &payload {
            lane.progress.remove(reader_id);
        }
        self.push(lane, reading_id, attempt_id, payload)
    }

    /// Progress có throttle. Trả `true` nếu thật sự emit.
    pub fn progress(&self, reading_id: Uuid, attempt_id: u32, reader_id: &str, chars: u64) -> bool {
        self.progress_at(reading_id, attempt_id, reader_id, chars, Instant::now())
    }

    fn progress_at(&self, reading_id: Uuid, attempt_id: u32, reader_id: &str, chars: u64, now: Instant) -> bool {
        let mut g = self.lock();
        let lane = g.entry(reading_id).or_default();
        let due = match lane.progress.get(reader_id) {
            None => true,
            Some((at, last)) => now.duration_since(*at) >= PROGRESS_INTERVAL || chars.saturating_sub(*last) >= PROGRESS_BYTES,
        };
        if due {
            lane.progress.insert(reader_id.to_owned(), (now, chars));
            self.push(lane, reading_id, attempt_id, Payload::ReaderProgress { reader_id: reader_id.to_owned(), chars });
        }
        due
    }

    /// Chạy `f(last_seq)` dưới mutex cấp seq → snapshot nhất quán với chuỗi event.
    pub fn with_seq<T>(&self, reading_id: Uuid, f: impl FnOnce(u64) -> T) -> T {
        let g = self.lock();
        f(g.get(&reading_id).map_or(0, |l| l.seq))
    }

    /// Xoá trạng thái throttle sau 1 attempt — KHÔNG xoá seq (seq xuyên suốt mọi attempt).
    pub fn forget_progress(&self, reading_id: Uuid) {
        if let Some(l) = self.lock().get_mut(&reading_id) {
            l.progress.clear();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[derive(Default)]
    pub(crate) struct Collect(pub Mutex<Vec<Envelope>>);
    impl EventSink for Collect {
        fn emit(&self, env: &Envelope) {
            self.0.lock().unwrap().push(env.clone());
        }
    }
    impl Collect {
        pub(crate) fn types(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|e| serde_json::to_value(&e.payload).unwrap()["type"].as_str().unwrap().to_owned())
                .collect()
        }
    }

    #[test]
    fn seq_is_per_reading_monotonic_across_attempts() {
        let sink = Arc::new(Collect::default());
        let em = ReadingEmitter::new(sink.clone());
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(em.emit(a, 1, Payload::SynthesisStarted), 1);
        assert_eq!(em.emit(b, 1, Payload::SynthesisStarted), 1);
        assert_eq!(em.emit(a, 2, Payload::Cancelled), 2); // attempt mới, seq không reset
        assert_eq!(em.with_seq(a, |s| s), 2);
        assert_eq!(em.with_seq(Uuid::nil(), |s| s), 0);
        let envs = sink.0.lock().unwrap();
        assert_eq!(envs.iter().filter(|e| e.reading_id == a).map(|e| (e.attempt_id, e.seq)).collect::<Vec<_>>(), vec![(1, 1), (2, 2)]);
    }

    #[test]
    fn envelope_wire_format() {
        let sink = Arc::new(Collect::default());
        let em = ReadingEmitter::new(sink.clone());
        em.emit(Uuid::nil(), 3, Payload::Error { code: "persist_failed".into(), message: "x".into(), retryable: true });
        let v = serde_json::to_value(&sink.0.lock().unwrap()[0]).unwrap();
        assert_eq!(v["payload"]["type"], "error");
        assert_eq!(v["payload"]["code"], "persist_failed");
        assert_eq!((v["attempt_id"].as_u64(), v["seq"].as_u64()), (Some(3), Some(1)));
        assert!(v["ts"].is_string());
    }

    #[test]
    fn progress_is_throttled_by_time_and_size() {
        let sink = Arc::new(Collect::default());
        let em = ReadingEmitter::new(sink.clone());
        let id = Uuid::new_v4();
        let t0 = Instant::now();
        assert!(em.progress_at(id, 1, "r", 10, t0));
        assert!(!em.progress_at(id, 1, "r", 20, t0 + Duration::from_millis(10)));
        assert!(em.progress_at(id, 1, "r", 30, t0 + Duration::from_millis(60)));
        assert!(em.progress_at(id, 1, "r", 30 + PROGRESS_BYTES, t0 + Duration::from_millis(61)));
        assert!(em.progress_at(id, 1, "other", 1, t0 + Duration::from_millis(61)), "mỗi thầy bói throttle riêng");
        // ReaderStarted reset throttle của thầy bói đó.
        em.emit(id, 2, Payload::ReaderStarted { reader_id: "r".into() });
        assert!(em.progress_at(id, 2, "r", 1, t0 + Duration::from_millis(62)));
        assert_eq!(sink.types().iter().filter(|t| *t == "reader_progress").count(), 5);
    }
}
