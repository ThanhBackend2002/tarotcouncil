//! Luận giải 2 pha (plan §3 — `interpret_reading`).
//!
//! Pha 1: các thầy bói chạy song song (giới hạn `max_parallel_calls`). Output được GIỮ trong Rust
//! cho tới khi mọi thầy bói xong hoặc lỗi — UI chỉ nhận `ReaderStarted` / `ReaderProgress`.
//! Cổng an toàn: bất kỳ output nào `crisis` (cờ mô hình hoặc lọc từ khoá) → huỷ các lời gọi còn lại,
//! bỏ MỌI output, `status = support`, emit `SupportRequired`; không phát `ReaderDone` nào.
//! Commit pha 1: ghi atomic `outputs` hợp lệ RỒI mới emit `ReaderDone`/`ReaderFailed`.
//! Pha 2: Trưởng thầy tổng hợp → cổng an toàn + hậu kiểm → ghi → `SynthesisDone` → `ReadingDone`.
//! Ghi lỗi ở bất kỳ bước nào → `status = failed` + `Error{persist_failed}`; chưa ghi được thì không emit nội dung.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;
use futures_util::future::join_all;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::budget::BudgetLedger;
use crate::config::{AppConfig, RoleDef};
use crate::cost;
use crate::deck::deck;
use crate::events::{Payload, ReadingEmitter};
use crate::journal::Journal;
use crate::llm_client::{ChatMessage, LlmClient, LlmError, Usage};
use crate::models::CallUsage;
use crate::prompts;
use crate::reading::{ReaderResult, Reading, ReadingStatus};
use crate::spreads::{self, Spread};
use crate::validate::{self, ReaderOutput, Synthesis};

/// Ước lượng output mỗi lần gọi (token) — dùng cho giới hạn mềm khi provider không trả usage.
const EST_OUTPUT_TOKENS: u64 = 900;

pub struct Job {
    pub reading_id: Uuid,
    pub attempt_id: u32,
    pub cfg: AppConfig,
    pub llm: LlmClient,
    pub cancel: CancellationToken,
}

pub struct Services {
    pub journal: Arc<Journal>,
    pub emitter: Arc<ReadingEmitter>,
}

enum Outcome<T> {
    Ok(T, Vec<String>),
    Crisis,
    Failed(String),
    Cancelled,
}

/// Ghi lại token từng lần gọi (thứ tự theo thời gian kết thúc).
struct Ledger {
    budget: BudgetLedger,
    calls: Mutex<Vec<CallUsage>>,
    base: u32,
}

impl Ledger {
    #[allow(clippy::too_many_arguments)]
    fn record(&self, agent: &str, kind: &str, model: &str, usage: Option<Usage>, est_prompt: u64, finish: &str, t0: Instant) {
        let mut calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        let (prompt, cached, completion, estimated) = match usage {
            Some(u) => (u.prompt_tokens, u.cached_tokens, u.completion_tokens, false),
            None => (est_prompt, 0, if finish == "error" { 0 } else { EST_OUTPUT_TOKENS }, true),
        };
        let call_id = self.base + calls.len() as u32;
        // Log: thầy bói, loại lời gọi, model, độ trễ, token — KHÔNG log câu hỏi / key.
        log::info!(
            "llm call #{call_id} reader={agent} kind={kind} model={model} finish={finish} latency_ms={} tokens={prompt}+{completion}{}",
            t0.elapsed().as_millis(),
            if estimated { " (ước lượng)" } else { "" }
        );
        calls.push(CallUsage {
            call_id,
            agent: agent.to_owned(),
            kind: kind.to_owned(),
            model: model.to_owned(),
            prompt,
            cached,
            completion,
            estimated,
            finish: finish.to_owned(),
            latency_ms: t0.elapsed().as_millis() as u64,
        });
    }
}

fn est_tokens(msgs: &[ChatMessage]) -> u64 {
    (msgs.iter().map(|m| m.content.chars().count() as u64).sum::<u64>() / 3) + EST_OUTPUT_TOKENS
}

/// 1 lần gọi JSON có giữ chỗ ngân sách + ghi usage.
async fn call_json<T: serde::de::DeserializeOwned>(
    llm: &LlmClient,
    cfg: &AppConfig,
    reader: &RoleDef,
    msgs: &[ChatMessage],
    kind: &str,
    ledger: &Ledger,
    cancel: &CancellationToken,
) -> Result<(T, String), Result<String, ()>> {
    let ep = cfg.resolve(&reader.id).ok_or_else(|| Ok(format!("{} chưa chọn provider/model", reader.name)))?;
    let est = est_tokens(msgs);
    let Some(rid) = ledger.budget.reserve(est) else {
        return Err(Ok("Vượt giới hạn token mềm của lượt đọc".into()));
    };
    let t0 = Instant::now();
    let res = llm.chat_json::<T>(&ep, msgs, cancel).await;
    match res {
        Ok((v, raw, usage)) => {
            ledger.budget.settle(rid, usage, est);
            ledger.record(&reader.id, kind, &ep.model, usage, est - EST_OUTPUT_TOKENS, "stop", t0);
            Ok((v, raw))
        }
        Err(LlmError::Cancelled) => {
            ledger.budget.settle(rid, None, 0);
            Err(Err(()))
        }
        Err(e) => {
            ledger.budget.settle(rid, None, 0);
            ledger.record(&reader.id, kind, &ep.model, None, 0, "error", t0);
            log::warn!("gọi {} ({}) lỗi: {e}", reader.id, ep.model);
            Err(Ok(format!("Lỗi provider: {e}")))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_reader(
    llm: &LlmClient,
    cfg: &AppConfig,
    reader: &RoleDef,
    reading: &Reading,
    spread: &Spread,
    ledger: &Ledger,
    cancel: &CancellationToken,
    on_progress: &(dyn Fn(u64) + Sync),
) -> Outcome<ReaderOutput> {
    let base = prompts::reader_messages(reader, spread, &reading.question, &reading.draws, deck(), &reading.safety_flags);
    let mut msgs = base.clone();
    for attempt in 0..2 {
        let kind = if attempt == 0 { "reader" } else { "reader_retry" };
        let (out, raw) = match call_json::<ReaderOutput>(llm, cfg, reader, &msgs, kind, ledger, cancel).await {
            Ok(v) => v,
            Err(Ok(reason)) => return Outcome::Failed(reason),
            Err(Err(())) => return Outcome::Cancelled,
        };
        on_progress(raw.chars().count() as u64);
        let text = validate::reader_text(&out);
        if validate::is_crisis(out.crisis, &text) {
            return Outcome::Crisis;
        }
        match validate::check_reader(&out, &reading.draws) {
            Ok(()) => {
                let warnings = validate::foreign_cards(&text, &reading.draws, deck())
                    .into_iter()
                    .map(|id| format!("Nhắc tới lá ngoài lượt bốc: {}", deck().by_id(&id).map_or(id.clone(), |c| c.name_vi.clone())))
                    .collect();
                return Outcome::Ok(out, warnings);
            }
            Err(errs) if attempt == 0 => {
                log::info!("{}: hậu kiểm trượt ({}), retry 1 lần", reader.id, errs.join("; "));
                msgs = prompts::correction(&base, &raw, &errs);
            }
            Err(errs) => return Outcome::Failed(format!("Kết quả không qua hậu kiểm: {}", errs.join("; "))),
        }
    }
    unreachable!("vòng lặp luôn return")
}

async fn run_synthesis(
    llm: &LlmClient,
    cfg: &AppConfig,
    chief: &RoleDef,
    reading: &Reading,
    spread: &Spread,
    ledger: &Ledger,
    cancel: &CancellationToken,
) -> Outcome<Synthesis> {
    let pairs: Vec<(&RoleDef, &ReaderResult)> =
        reading.outputs.iter().filter(|o| o.output.is_some()).filter_map(|o| cfg.role(&o.reader_id).map(|r| (r, o))).collect();
    let base = prompts::synthesis_messages(chief, spread, &reading.question, &reading.draws, deck(), &reading.safety_flags, &pairs);
    let mut msgs = base.clone();
    for attempt in 0..2 {
        let kind = if attempt == 0 { "synthesis" } else { "synthesis_retry" };
        let (mut s, raw) = match call_json::<Synthesis>(llm, cfg, chief, &msgs, kind, ledger, cancel).await {
            Ok(v) => v,
            Err(Ok(reason)) => return Outcome::Failed(reason),
            Err(Err(())) => return Outcome::Cancelled,
        };
        if validate::is_crisis(s.crisis, &validate::synthesis_text(&s)) {
            return Outcome::Crisis;
        }
        match validate::check_synthesis(&s) {
            Ok(()) => {
                s.disclaimer = prompts::final_disclaimer(&s.disclaimer, &reading.safety_flags);
                return Outcome::Ok(s, vec![]);
            }
            Err(errs) if attempt == 0 => msgs = prompts::correction(&base, &raw, &errs),
            Err(errs) => return Outcome::Failed(format!("Tổng hợp không qua hậu kiểm: {}", errs.join("; "))),
        }
    }
    unreachable!()
}

/// Chạy 1 attempt. `reading` đã ở `Interpreting` với `attempt_id` mới (do `table::begin_interpret`).
pub async fn run(job: Job, svc: Services) {
    let Job { reading_id, attempt_id, cfg, llm, cancel } = job;
    let emit = |p: Payload| {
        svc.emitter.emit(reading_id, attempt_id, p);
    };
    let journal = svc.journal.clone();
    let save = |r: &Reading| {
        let j = journal.clone();
        let r = r.clone();
        async move { tokio::task::spawn_blocking(move || j.save(&r)).await.map_err(|e| e.to_string()).and_then(|x| x.map_err(|e| e.to_string())) }
    };

    let j2 = svc.journal.clone();
    let mut reading = match tokio::task::spawn_blocking(move || j2.load(reading_id)).await {
        Ok(Ok(r)) => r,
        other => {
            log::error!("không nạp được lượt đọc {reading_id}: {other:?}");
            emit(Payload::Error { code: "not_found".into(), message: "Không nạp được lượt đọc".into(), retryable: false });
            return;
        }
    };
    let Some(spread) = spreads::get(&reading.spread_id) else {
        emit(Payload::Error { code: "invalid".into(), message: "Kiểu trải không còn tồn tại".into(), retryable: false });
        return;
    };
    log::info!("interpret reading={reading_id} attempt={attempt_id} readers={} spread={}", reading.reader_ids.len(), reading.spread_id);
    let ledger = Arc::new(Ledger {
        budget: BudgetLedger::new(cfg.tarot.max_total_tokens),
        calls: Mutex::new(Vec::new()),
        base: reading.calls.len() as u32,
    });

    // Kết thúc attempt ở trạng thái `status`, ghi file (giữ các lá đã bốc), rồi emit `then`.
    macro_rules! finish {
        ($status:expr, $err:expr, $then:expr) => {{
            reading.status = $status;
            reading.error = $err;
            reading.calls.extend(ledger.calls.lock().unwrap_or_else(|e| e.into_inner()).drain(..));
            reading.updated_at = Utc::now();
            match save(&reading).await {
                Ok(()) => emit($then),
                Err(e) => {
                    log::error!("ghi lượt đọc {reading_id} thất bại: {e}");
                    emit(Payload::Error { code: "persist_failed".into(), message: "Không lưu được lượt đọc".into(), retryable: true });
                }
            }
            svc.emitter.forget_progress(reading_id);
            return;
        }};
    }

    // ── Pha 1 ──
    let crisis = cancel.child_token();
    let sem = Arc::new(Semaphore::new(cfg.max_parallel_calls.max(1) as usize));
    let readers: Vec<RoleDef> = reading.reader_ids.iter().filter_map(|id| cfg.role(id).cloned()).collect();
    let tasks = readers.iter().map(|reader| {
        let (sem, crisis, ledger, llm, cfg, reading) = (sem.clone(), crisis.clone(), ledger.clone(), &llm, &cfg, &reading);
        let emitter = svc.emitter.clone();
        async move {
            let _permit = tokio::select! {
                p = sem.acquire_owned() => p.ok(),
                _ = crisis.cancelled() => None,
            };
            if crisis.is_cancelled() {
                return (reader.id.clone(), Outcome::Cancelled);
            }
            emitter.emit(reading_id, attempt_id, Payload::ReaderStarted { reader_id: reader.id.clone() });
            let on_progress = |chars: u64| {
                emitter.progress(reading_id, attempt_id, &reader.id, chars);
            };
            let out = run_reader(llm, cfg, reader, reading, spread, &ledger, &crisis, &on_progress).await;
            if matches!(out, Outcome::Crisis) {
                crisis.cancel(); // huỷ các lời gọi còn lại
            }
            (reader.id.clone(), out)
        }
    });
    let results = join_all(tasks).await;

    if results.iter().any(|(_, o)| matches!(o, Outcome::Crisis)) {
        reading.outputs.clear();
        reading.synthesis = None;
        finish!(ReadingStatus::Support, None, Payload::SupportRequired);
    }
    if cancel.is_cancelled() {
        reading.outputs.clear();
        finish!(ReadingStatus::Cancelled, None, Payload::Cancelled);
    }

    reading.outputs = results
        .into_iter()
        .map(|(reader_id, o)| match o {
            Outcome::Ok(out, warnings) => ReaderResult { reader_id, output: Some(out), failed: None, warnings },
            Outcome::Failed(reason) => ReaderResult { reader_id, output: None, failed: Some(reason), warnings: vec![] },
            Outcome::Crisis | Outcome::Cancelled => ReaderResult { reader_id, output: None, failed: Some("Đã huỷ".into()), warnings: vec![] },
        })
        .collect();
    reading.calls.extend(ledger.calls.lock().unwrap_or_else(|e| e.into_inner()).drain(..));
    reading.updated_at = Utc::now();
    // Commit pha 1: ghi TRƯỚC, emit SAU.
    if let Err(e) = save(&reading).await {
        log::error!("commit pha 1 của {reading_id} thất bại: {e}");
        reading.outputs.clear();
        finish!(ReadingStatus::Failed, Some("Không lưu được kết quả".into()), Payload::Error {
            code: "persist_failed".into(),
            message: "Không lưu được kết quả luận giải".into(),
            retryable: true
        });
    }
    for o in &reading.outputs {
        emit(match &o.output {
            Some(_) => Payload::ReaderDone { reader_id: o.reader_id.clone(), output: Box::new(o.clone()) },
            None => Payload::ReaderFailed { reader_id: o.reader_id.clone(), reason: o.failed.clone().unwrap_or_default() },
        });
    }
    emit(usage_payload(&reading, &cfg));
    if reading.outputs.iter().all(|o| o.output.is_none()) {
        finish!(ReadingStatus::Failed, Some("Không thầy bói nào luận giải được".into()), Payload::Error {
            code: "all_failed".into(),
            message: "Không thầy bói nào luận giải được — xem nghĩa gốc của các lá bên dưới".into(),
            retryable: true
        });
    }

    // ── Pha 2 ──
    let Some(chief) = cfg.role(&reading.chief_reader_id).cloned() else {
        finish!(ReadingStatus::Failed, Some("Trưởng thầy không còn tồn tại".into()), Payload::Error {
            code: "synthesis_failed".into(),
            message: "Trưởng thầy không còn trong cấu hình".into(),
            retryable: true
        });
    };
    emit(Payload::SynthesisStarted);
    let res = run_synthesis(&llm, &cfg, &chief, &reading, spread, &ledger, &cancel).await;
    match res {
        Outcome::Crisis => {
            reading.outputs.clear();
            reading.synthesis = None;
            finish!(ReadingStatus::Support, None, Payload::SupportRequired);
        }
        Outcome::Cancelled => {
            finish!(ReadingStatus::Cancelled, None, Payload::Cancelled);
        }
        Outcome::Failed(reason) => {
            finish!(ReadingStatus::Failed, Some(reason.clone()), Payload::Error {
                code: "synthesis_failed".into(),
                message: format!("Trưởng thầy chưa tổng hợp được: {reason}"),
                retryable: true
            });
        }
        Outcome::Ok(s, _) => {
            reading.synthesis = Some(s.clone());
            reading.status = ReadingStatus::Done;
            reading.error = None;
            reading.calls.extend(ledger.calls.lock().unwrap_or_else(|e| e.into_inner()).drain(..));
            reading.usage = total_usage(&reading);
            reading.updated_at = Utc::now();
            if let Err(e) = save(&reading).await {
                log::error!("ghi tổng hợp của {reading_id} thất bại: {e}");
                reading.synthesis = None;
                finish!(ReadingStatus::Failed, Some("Không lưu được tổng hợp".into()), Payload::Error {
                    code: "persist_failed".into(),
                    message: "Không lưu được phần tổng hợp".into(),
                    retryable: true
                });
            }
            emit(Payload::SynthesisDone { synthesis: s });
            emit(usage_payload(&reading, &cfg));
            emit(Payload::ReadingDone);
            svc.emitter.forget_progress(reading_id);
        }
    }
}

fn total_usage(r: &Reading) -> crate::models::TokenUsage {
    let mut u = crate::models::TokenUsage::default();
    for c in &r.calls {
        u.add(c.prompt, c.cached, c.completion, c.estimated);
    }
    u
}

fn usage_payload(r: &Reading, cfg: &AppConfig) -> Payload {
    let b = cost::usage_breakdown(&r.calls, &cfg.pricing);
    Payload::Usage { tokens: b.totals.prompt + b.totals.completion, cost_usd: b.totals.cost_usd }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::with_default_provider;
    use crate::events::tests::Collect;
    use crate::roster::{CHIEF_READER, CLASSIC_READER, JUNG_READER};
    use crate::storage::tests::tmp_storage;
    use crate::table::{self, Created};
    use crate::test_support::{completion, counted, spawn_mock, MockResp};
    use serde_json::{json, Value};

    struct World {
        journal: Arc<Journal>,
        sink: Arc<Collect>,
        emitter: Arc<ReadingEmitter>,
        cfg: AppConfig,
        reading: Reading,
    }

    /// Lượt đọc 3 lá đã bốc xong, `readers` thầy bói, provider = mock server.
    async fn world(readers: &[&str], handler: crate::test_support::Handler) -> World {
        let base = spawn_mock(handler).await;
        let mut cfg = with_default_provider("sk-test-12345678", "mock-model");
        cfg.providers[0].base_url = base;
        cfg.max_retries = 0;
        let ids: Vec<String> = readers.iter().map(|s| s.to_string()).collect();
        let Created::Reading(r) = table::create(&cfg, "Công việc của tôi sắp tới?", "three", &ids, [5; 32]).unwrap() else { panic!() };
        let mut reading = *r;
        table::auto_draw(&mut reading).unwrap();
        let journal = Arc::new(Journal::open(tmp_storage()));
        journal.save(&reading).unwrap();
        let sink = Arc::new(Collect::default());
        World { journal, emitter: Arc::new(ReadingEmitter::new(sink.clone())), sink, cfg, reading }
    }

    async fn interpret(w: &mut World) -> Reading {
        let attempt = table::begin_interpret(&mut w.reading).unwrap();
        w.journal.save(&w.reading).unwrap();
        let llm = LlmClient::new(reqwest::Client::new(), &w.cfg);
        run(
            Job { reading_id: w.reading.id, attempt_id: attempt, cfg: w.cfg.clone(), llm, cancel: CancellationToken::new() },
            Services { journal: w.journal.clone(), emitter: w.emitter.clone() },
        )
        .await;
        w.journal.load(w.reading.id).unwrap()
    }

    /// Trả lời đúng schema dựa vào DRAWN CARDS trong prompt.
    fn good_reader(body: &Value) -> MockResp {
        let user = body["messages"][1]["content"].as_str().unwrap();
        let system = body["messages"][0]["content"].as_str().unwrap();
        if system.contains("chair of a council") {
            return completion(&json!({"summary":"Tổng hợp ổn","agreements":["a"],"divergences":["d"],"advice":["Nghỉ ngơi"],"disclaimer":"Vui thôi.","crisis":false}).to_string());
        }
        let start = user.find('[').unwrap();
        let end = user.find("\n\nQUESTION").unwrap();
        let cards: Vec<Value> = serde_json::from_str(&user[start..end]).unwrap();
        let positions: Vec<Value> = cards
            .iter()
            .map(|c| json!({"position": c["position"], "card_id": c["card_id"], "interpretation": "Lá này gợi ý kiên nhẫn."}))
            .collect();
        completion(&json!({"positions": positions, "overall": "Mọi thứ sẽ ổn.", "tone": "hopeful", "crisis": false}).to_string())
    }

    #[tokio::test]
    async fn happy_path_commits_before_emitting_and_completes() {
        let mut w = world(&[CLASSIC_READER, JUNG_READER], counted(|_, b| good_reader(b))).await;
        let r = interpret(&mut w).await;
        assert_eq!(r.status, ReadingStatus::Done, "{:?}", r.error);
        assert_eq!(r.outputs.len(), 2);
        assert!(r.outputs.iter().all(|o| o.output.is_some()));
        let s = r.synthesis.unwrap();
        assert!(s.disclaimer.starts_with("Vui thôi.") && s.disclaimer.contains("giải trí"));
        assert_eq!(r.calls.len(), 3);
        assert_eq!(r.usage.calls, 3);
        let t = w.sink.types();
        let first_done = t.iter().position(|x| x == "reader_done").unwrap();
        assert!(t[..first_done].iter().all(|x| x == "reader_started" || x == "reader_progress"), "{t:?}");
        assert_eq!(t.last().unwrap(), "reading_done");
        assert!(t.contains(&"synthesis_done".to_string()) && t.contains(&"usage".to_string()));
        let seqs: Vec<u64> = w.sink.0.lock().unwrap().iter().map(|e| e.seq).collect();
        assert!(seqs.windows(2).all(|p| p[1] == p[0] + 1), "seq liên tục: {seqs:?}");
    }

    #[tokio::test]
    async fn crisis_from_any_reader_publishes_nothing() {
        let handler = counted(|_, b| {
            let system = b["messages"][0]["content"].as_str().unwrap();
            if system.contains("Nhà Tâm Lý Jung") {
                completion(&json!({"positions": [], "overall": "", "tone": "neutral", "crisis": true}).to_string())
            } else {
                good_reader(b)
            }
        });
        let mut w = world(&[CLASSIC_READER, JUNG_READER], handler).await;
        let r = interpret(&mut w).await;
        assert_eq!(r.status, ReadingStatus::Support);
        assert!(r.outputs.is_empty() && r.synthesis.is_none());
        let t = w.sink.types();
        assert!(!t.iter().any(|x| x == "reader_done" || x == "synthesis_done"), "{t:?}");
        assert_eq!(t.last().unwrap(), "support_required");
        assert_eq!(r.draws.len(), 3, "giữ các lá đã bốc");
    }

    #[tokio::test]
    async fn invalid_output_is_retried_once_then_falls_back() {
        // Thầy bói Cổ Điển luôn trả sai card_id → trượt 2 lần → ReaderFailed; Jung hợp lệ.
        let handler = counted(|_, b| {
            let system = b["messages"][0]["content"].as_str().unwrap();
            if system.contains("Bà Đồng Cổ Điển") {
                completion(&json!({"positions": [{"position":0,"card_id":"major-00","interpretation":"x"}], "overall":"y"}).to_string())
            } else {
                good_reader(b)
            }
        });
        let mut w = world(&[CLASSIC_READER, JUNG_READER], handler).await;
        let r = interpret(&mut w).await;
        assert_eq!(r.status, ReadingStatus::Done);
        let classic = r.outputs.iter().find(|o| o.reader_id == CLASSIC_READER).unwrap();
        assert!(classic.output.is_none() && classic.failed.as_deref().unwrap().contains("hậu kiểm"));
        let kinds: Vec<_> = r.calls.iter().filter(|c| c.agent == CLASSIC_READER).map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["reader", "reader_retry"]);
        assert!(w.sink.types().contains(&"reader_failed".to_string()));
    }

    #[tokio::test]
    async fn all_readers_failing_marks_failed_and_allows_retry() {
        let mut w = world(&[CLASSIC_READER], counted(|_, _| MockResp::http(401, "application/json", r#"{"error":"bad key"}"#))).await;
        let r = interpret(&mut w).await;
        assert_eq!(r.status, ReadingStatus::Failed);
        assert_eq!(w.sink.types().last().unwrap(), "error");
        w.reading = r;
        assert!(table::begin_interpret(&mut w.reading).is_ok(), "luận giải lại được");
    }

    #[tokio::test]
    async fn cancel_keeps_draws_and_emits_cancelled() {
        let mut w = world(&[CLASSIC_READER], counted(|_, b| good_reader(b).delayed(std::time::Duration::from_secs(5)))).await;
        let attempt = table::begin_interpret(&mut w.reading).unwrap();
        w.journal.save(&w.reading).unwrap();
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            c2.cancel();
        });
        let llm = LlmClient::new(reqwest::Client::new(), &w.cfg);
        run(
            Job { reading_id: w.reading.id, attempt_id: attempt, cfg: w.cfg.clone(), llm, cancel },
            Services { journal: w.journal.clone(), emitter: w.emitter.clone() },
        )
        .await;
        let r = w.journal.load(w.reading.id).unwrap();
        assert_eq!((r.status, r.draws.len(), r.outputs.len()), (ReadingStatus::Cancelled, 3, 0));
        assert_eq!(w.sink.types().last().unwrap(), "cancelled");
    }

    #[tokio::test]
    async fn soft_budget_blocks_calls() {
        let mut w = world(&[CLASSIC_READER], counted(|_, b| good_reader(b))).await;
        w.cfg.tarot.max_total_tokens = Some(1000);
        let r = interpret(&mut w).await;
        assert_eq!(r.status, ReadingStatus::Failed);
        assert!(r.outputs[0].failed.as_deref().unwrap().contains("giới hạn token"));
        let _ = CHIEF_READER;
    }
}
