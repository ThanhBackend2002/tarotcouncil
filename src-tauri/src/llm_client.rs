//! OpenAI-compatible Chat Completions client (`POST {base_url}/chat/completions`).
//!
//! - Retry exponential backoff + jitter cho 408/425/429/5xx/529/timeout/network, tôn trọng `Retry-After`.
//! - Mọi await đều `select!` với `CancellationToken` → bấm Stop là dừng ngay, không chờ hết request.
//! - `chat_json`: parse khoan dung (bỏ code fence, `<think>`, text thừa) + 1 lần "repair prompt".
//! - `chat_stream`: SSE parser tự viết (không thêm dependency), idle-timeout từng chunk,
//!   tự fallback nếu provider phớt lờ `stream: true` và trả JSON thường.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, CONTENT_TYPE, RETRY_AFTER};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::config::{api_root, AppConfig, JsonMode, ResolvedEndpoint};
use crate::models::truncate_chars;

const MAX_ERROR_BODY_CHARS: usize = 500;
const MAX_SSE_LINE_BYTES: usize = 1 << 20; // 1 MiB: chống server lỗi đẩy 1 dòng vô hạn
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Giả định khi endpoint không đặt `max_tokens` (dùng để tính lần retry khi output bị cắt).
pub const DEFAULT_OUTPUT_TOKENS: u32 = 4096;
/// Trần `max_tokens` khi tự nới cho lần retry — không bao giờ xin quá mức này.
pub const OUTPUT_TOKENS_CAP: u32 = 32768;

// ───────────────────────────── Types ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(c: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: c.into(),
        }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: c.into(),
        }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: c.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    Text,
    Json,
}

pub enum StreamSignal<'a> {
    /// Bắt đầu (lại) 1 attempt → consumer phải xoá buffer cũ.
    Reset,
    Delta(&'a str),
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("đã huỷ")]
    Cancelled,
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("timeout khi gọi LLM")]
    Timeout,
    #[error("lỗi mạng: {0}")]
    Network(String),
    #[error("phản hồi không hợp lệ: {0}")]
    InvalidResponse(String),
    #[error("không parse được JSON từ model: {0}")]
    JsonParse(String),
    /// Output bị cắt: `"length"` (retry với max_tokens lớn hơn vẫn bị cắt) hoặc
    /// `"max_tokens_cap"` (max_tokens đã chạm `OUTPUT_TOKENS_CAP`, không nới thêm được).
    #[error("output của model bị cắt ({reason})")]
    Truncated { reason: &'static str },
}

impl LlmError {
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::Http { status, .. } => {
                matches!(status, 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504 | 529)
            }
            LlmError::Timeout | LlmError::Network(_) => true,
            _ => false,
        }
    }

    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            LlmError::Timeout
        } else if e.is_decode() {
            LlmError::InvalidResponse(e.to_string())
        } else {
            LlmError::Network(e.to_string())
        }
    }
}

/// Token tiêu thụ của 1 lần gọi (theo `usage` provider trả về).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Phần `prompt_tokens` đọc từ cache của provider (`prompt_tokens_details.cached_tokens`); thiếu → 0.
    pub cached_tokens: u64,
}

/// Dạng thô provider trả về; `cached_tokens` nằm trong `prompt_tokens_details` (OpenAI) —
/// vài proxy đặt thẳng ở cấp ngoài, nên nhận cả hai.
#[derive(Deserialize)]
struct RawUsage {
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    completion_tokens: Option<u64>,
    #[serde(default)]
    prompt_tokens_details: Option<RawPromptDetails>,
    #[serde(default)]
    cached_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct RawPromptDetails {
    #[serde(default)]
    cached_tokens: Option<u64>,
}

impl<'de> Deserialize<'de> for Usage {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawUsage::deserialize(d)?;
        let prompt_tokens = raw.prompt_tokens.unwrap_or(0);
        let cached = raw
            .prompt_tokens_details
            .and_then(|p| p.cached_tokens)
            .or(raw.cached_tokens)
            .unwrap_or(0);
        Ok(Usage {
            prompt_tokens,
            completion_tokens: raw.completion_tokens.unwrap_or(0),
            // Cache không thể lớn hơn tổng prompt (provider lỗi số liệu → kẹp lại).
            cached_tokens: cached.min(prompt_tokens),
        })
    }
}

impl Usage {
    pub fn add(self, o: Usage) -> Usage {
        Usage {
            prompt_tokens: self.prompt_tokens + o.prompt_tokens,
            completion_tokens: self.completion_tokens + o.completion_tokens,
            cached_tokens: self.cached_tokens + o.cached_tokens,
        }
    }
}

/// Cộng 2 usage có thể thiếu (provider không trả usage → None).
pub fn sum_usage(a: Option<Usage>, b: Option<Usage>) -> Option<Usage> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.add(y)),
        (x, None) => x,
        (None, y) => y,
    }
}

/// Lý do kết thúc của 1 lần gọi chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Finish {
    /// Model tự dừng (hoặc provider không báo `finish_reason`).
    Stop,
    /// Bị cắt do `max_tokens` (`finish_reason=length`).
    Length,
    /// Stream đóng kết nối giữa chừng: không có `[DONE]` lẫn `finish_reason` → text có thể thiếu.
    Error,
}

impl Finish {
    /// `"length"` → `Length`; thiếu hoặc giá trị khác (`stop`, `content_filter`…) → `Stop`.
    fn from_reason(reason: Option<&str>) -> Self {
        match reason {
            Some("length") => Finish::Length,
            _ => Finish::Stop,
        }
    }
}

/// Kết quả 1 lần gọi chat: nội dung + token (nếu provider báo) + lý do kết thúc.
#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub usage: Option<Usage>,
    pub finish: Finish,
}

/// 1 lượt trong vòng gọi công cụ.
#[derive(Debug, Clone)]
pub struct ToolTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// Message assistant nguyên bản (giữ `tool_calls`) để nối lại vào hội thoại.
    pub assistant_message: Value,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    message: Option<ResponseMessage>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: Option<StreamDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

// ───────────────────────────── Client ─────────────────────────────

/// Rẻ để clone (`reqwest::Client` bên trong là `Arc`) → share connection pool giữa 3 reviewer.
#[derive(Clone)]
pub struct LlmClient {
    http: reqwest::Client,
    max_retries: u32,
    request_timeout: Duration,
    stream_idle_timeout: Duration,
    /// Provider từ chối `stream_options` → tắt cho các lần stream sau (dùng chung giữa các clone).
    stream_usage_off: Arc<AtomicBool>,
}

pub fn build_http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .use_rustls_tls()
        .connect_timeout(Duration::from_secs(15))
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .user_agent(concat!("TarotCouncil/", env!("CARGO_PKG_VERSION")))
        .build()
}

impl LlmClient {
    pub fn new(http: reqwest::Client, cfg: &AppConfig) -> Self {
        Self {
            http,
            max_retries: cfg.max_retries,
            request_timeout: Duration::from_secs(cfg.request_timeout_secs),
            stream_idle_timeout: Duration::from_secs(cfg.stream_idle_timeout_secs),
            stream_usage_off: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_limits(mut self, max_retries: u32, request_timeout: Duration) -> Self {
        self.max_retries = max_retries;
        self.request_timeout = request_timeout;
        self
    }

    fn build_body(
        &self,
        ep: &ResolvedEndpoint,
        messages: &[ChatMessage],
        stream: bool,
        json_rf: bool,
    ) -> Value {
        let mut body = json!({
            "model": ep.model,
            "messages": messages,
            "temperature": ep.temperature,
            "stream": stream,
        });
        if let Some(mt) = ep.max_tokens {
            body["max_tokens"] = json!(mt);
        }
        if json_rf {
            body["response_format"] = json!({ "type": "json_object" });
        }
        if stream && !self.stream_usage_off.load(Ordering::Relaxed) {
            // Để chunk cuối có `usage` (OpenAI và đa số proxy hỗ trợ).
            body["stream_options"] = json!({ "include_usage": true });
        }
        body
    }

    /// Non-streaming completion, trả về `choices[0].message.content` + usage.
    pub async fn chat(
        &self,
        ep: &ResolvedEndpoint,
        messages: &[ChatMessage],
        expect: Expect,
        cancel: &CancellationToken,
    ) -> Result<Completion, LlmError> {
        let mut use_rf = expect == Expect::Json && ep.json_mode == JsonMode::JsonObject;
        loop {
            let body = self.build_body(ep, messages, false, use_rf);
            match self
                .send_with_retry(ep, &body, cancel, Some(self.request_timeout))
                .await
            {
                Ok(resp) => return read_completion(resp, cancel).await,
                // Nhiều proxy/local model không hiểu `response_format` → hạ cấp 1 lần.
                Err(LlmError::Http {
                    status: 400 | 422,
                    ref body,
                }) if use_rf => {
                    log::warn!(
                        "model {} từ chối response_format=json_object ({}); thử lại không có response_format",
                        ep.model,
                        truncate_chars(body, 120)
                    );
                    use_rf = false;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Gọi model và parse JSON thành `T`. Trả về cả raw text (để đưa lại vào hội thoại khi cần sửa).
    /// - Output bị cắt (`Finish::Length`) → retry 1 lần với max_tokens ×1.5 (tối đa `OUTPUT_TOKENS_CAP`);
    ///   vẫn bị cắt hoặc đã chạm trần → `LlmError::Truncated`, không cố parse JSON dở dang.
    /// - Parse fail → gửi lại 1 "repair prompt" kèm lỗi cụ thể.
    ///
    /// Usage trả về là tổng của mọi attempt.
    pub async fn chat_json<T: DeserializeOwned>(
        &self,
        ep: &ResolvedEndpoint,
        messages: &[ChatMessage],
        cancel: &CancellationToken,
    ) -> Result<(T, String, Option<Usage>), LlmError> {
        let mut first = self.chat(ep, messages, Expect::Json, cancel).await?;
        let mut usage = first.usage;
        // Endpoint thực dùng cho các lần gọi sau (đã nới max_tokens nếu lần đầu bị cắt).
        let mut bumped: Option<ResolvedEndpoint> = None;
        if first.finish == Finish::Length {
            let current = ep.max_tokens.unwrap_or(DEFAULT_OUTPUT_TOKENS);
            if current >= OUTPUT_TOKENS_CAP {
                log::warn!("model {} bị cắt JSON ở max_tokens={current} (đã chạm trần)", ep.model);
                return Err(LlmError::Truncated { reason: "max_tokens_cap" });
            }
            let next = bumped_max_tokens(current);
            log::warn!(
                "model {} bị cắt JSON (finish_reason=length, max_tokens={current}); retry với max_tokens={next}",
                ep.model
            );
            let mut bigger = ep.clone();
            bigger.max_tokens = Some(next);
            let retry = self.chat(&bigger, messages, Expect::Json, cancel).await?;
            usage = sum_usage(usage, retry.usage);
            if retry.finish == Finish::Length {
                return Err(LlmError::Truncated { reason: "length" });
            }
            first = retry;
            bumped = Some(bigger);
        }
        let ep = bumped.as_ref().unwrap_or(ep);

        let text = first.text;
        let first_err = match parse_json_lenient::<T>(&text) {
            Ok(v) => return Ok((v, text, usage)),
            Err(e) => e,
        };
        log::warn!(
            "model {} trả JSON không hợp lệ ({first_err}); gửi repair prompt",
            ep.model
        );

        let mut repair = messages.to_vec();
        repair.push(ChatMessage::assistant(truncate_chars(&text, 8_000)));
        repair.push(ChatMessage::user(format!(
            "Your previous reply was NOT valid JSON for the required schema. Parser error: {first_err}. \
             Reply again with ONLY one valid JSON object matching the schema. No markdown fences, no prose."
        )));
        let second = self.chat(ep, &repair, Expect::Json, cancel).await?;
        let usage = sum_usage(usage, second.usage);
        if second.finish == Finish::Length {
            return Err(LlmError::Truncated { reason: "length" });
        }
        parse_json_lenient::<T>(&second.text)
            .map(|v| (v, second.text, usage))
            .map_err(LlmError::JsonParse)
    }

    /// 1 lượt gọi có công cụ (OpenAI function calling, non-stream).
    /// `messages` là JSON thô vì cần chứa `tool_calls` / `role: "tool"`.
    pub async fn chat_tools(
        &self,
        ep: &ResolvedEndpoint,
        messages: &[Value],
        tools: &[Value],
        allow_tools: bool,
        cancel: &CancellationToken,
    ) -> Result<ToolTurn, LlmError> {
        let mut body = json!({
            "model": ep.model,
            "messages": messages,
            "temperature": ep.temperature,
            "stream": false,
        });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
            body["tool_choice"] = json!(if allow_tools { "auto" } else { "none" });
        }
        if let Some(mt) = ep.max_tokens {
            body["max_tokens"] = json!(mt);
        }
        let resp = self
            .send_with_retry(ep, &body, cancel, Some(self.request_timeout))
            .await?;
        let text = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(LlmError::Cancelled),
            t = resp.text() => t.map_err(LlmError::from_reqwest)?,
        };
        parse_tool_turn(&text)
    }

    /// Streaming completion. `on` nhận `Reset` trước mỗi attempt và `Delta` cho từng mẩu text.
    pub async fn chat_stream<F>(
        &self,
        ep: &ResolvedEndpoint,
        messages: &[ChatMessage],
        cancel: &CancellationToken,
        mut on: F,
    ) -> Result<Completion, LlmError>
    where
        F: FnMut(StreamSignal<'_>) + Send,
    {
        let mut body = self.build_body(ep, messages, true, false);
        let mut attempt = 0u32;
        // Usage của các attempt đã đứt giữa chừng (hiếm khi có, vì usage thường nằm ở chunk cuối).
        let mut failed_usage: Option<Usage> = None;
        loop {
            on(StreamSignal::Reset);
            // Không đặt total-timeout cho stream: plan dài có thể chạy vài phút. Dùng idle-timeout.
            let resp = match self.send_with_retry(ep, &body, cancel, None).await {
                Ok(r) => r,
                // Provider không hiểu `stream_options` → bỏ đi, gửi lại (1 lần cho cả phiên).
                Err(LlmError::Http { status: 400 | 422, body: ref err })
                    if body.get("stream_options").is_some() =>
                {
                    log::warn!(
                        "model {} từ chối stream_options ({}); stream không kèm usage",
                        ep.model,
                        truncate_chars(err, 120)
                    );
                    self.stream_usage_off.store(true, Ordering::Relaxed);
                    if let Some(o) = body.as_object_mut() {
                        o.remove("stream_options");
                    }
                    continue;
                }
                Err(e) => return Err(e),
            };
            let mut acc = StreamAcc::default();
            match self.consume_stream(resp, cancel, &mut on, &mut acc).await {
                Ok(mut c) => {
                    c.usage = sum_usage(failed_usage, c.usage);
                    return Ok(c);
                }
                Err(LlmError::Cancelled) => return Err(LlmError::Cancelled),
                Err(e) if e.is_retryable() && attempt < self.max_retries => {
                    failed_usage = sum_usage(failed_usage, acc.usage);
                    attempt += 1;
                    let delay = backoff(attempt);
                    log::warn!(
                        "stream {} đứt giữa chừng ({e}); retry #{attempt} sau {delay:?}",
                        ep.model
                    );
                    sleep_cancellable(delay, cancel).await?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn send_with_retry(
        &self,
        ep: &ResolvedEndpoint,
        body: &Value,
        cancel: &CancellationToken,
        timeout: Option<Duration>,
    ) -> Result<reqwest::Response, LlmError> {
        let url = ep.chat_url();
        let mut attempt = 0u32;
        loop {
            if cancel.is_cancelled() {
                return Err(LlmError::Cancelled);
            }
            let mut req = self.http.post(&url).json(body);
            if let Some(key) = ep.api_key.as_deref().filter(|k| !k.is_empty()) {
                req = req.bearer_auth(key);
            }
            if let Some(t) = timeout {
                req = req.timeout(t);
            }

            let outcome = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                r = req.send() => r,
            };

            let (err, retry_after) = match outcome {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let retry_after = parse_retry_after(resp.headers());
                    let text = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                        t = resp.text() => t.unwrap_or_default(),
                    };
                    (
                        LlmError::Http {
                            status,
                            body: truncate_chars(text.trim(), MAX_ERROR_BODY_CHARS),
                        },
                        retry_after,
                    )
                }
                Err(e) => (LlmError::from_reqwest(e), None),
            };

            attempt += 1;
            if !err.is_retryable() || attempt > self.max_retries {
                return Err(err);
            }
            let delay = retry_after
                .unwrap_or_else(|| backoff(attempt))
                .min(MAX_BACKOFF);
            log::warn!(
                "LLM {} lỗi ({err}); retry {attempt}/{} sau {delay:?}",
                ep.model,
                self.max_retries
            );
            sleep_cancellable(delay, cancel).await?;
        }
    }

    /// Đọc 1 response SSE vào `acc` (do caller tạo để vẫn lấy được usage khi attempt lỗi).
    async fn consume_stream<F>(
        &self,
        resp: reqwest::Response,
        cancel: &CancellationToken,
        on: &mut F,
        acc: &mut StreamAcc,
    ) -> Result<Completion, LlmError>
    where
        F: FnMut(StreamSignal<'_>) + Send,
    {
        let is_json = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("application/json"));
        if is_json {
            // Provider bỏ qua stream=true → đọc như response thường.
            let c = read_completion(resp, cancel).await?;
            on(StreamSignal::Delta(&c.text));
            return Ok(c);
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);

        'outer: loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                n = tokio::time::timeout(self.stream_idle_timeout, stream.next()) => n,
            };
            let chunk = match next {
                Err(_) => return Err(LlmError::Timeout),
                Ok(None) => break,
                Ok(Some(Err(e))) => return Err(LlmError::from_reqwest(e)),
                Ok(Some(Ok(b))) => b,
            };
            buf.extend_from_slice(&chunk);

            // '\n' không bao giờ nằm giữa 1 ký tự UTF-8 multi-byte → split theo byte an toàn.
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                if acc.push_line(&line, on)? {
                    break 'outer;
                }
            }
            if buf.len() > MAX_SSE_LINE_BYTES {
                return Err(LlmError::InvalidResponse("dòng SSE vượt quá 1 MiB".into()));
            }
        }
        if !buf.is_empty() && !acc.done {
            acc.push_line(&buf, on)?;
        }
        acc.take_completion()
    }
}

/// Trạng thái gom của 1 attempt stream — tách riêng để test thuần không cần HTTP.
#[derive(Debug, Default)]
struct StreamAcc {
    out: String,
    usage: Option<Usage>,
    /// `finish_reason` cuối cùng thấy được (chunk cuối thường có delta rỗng).
    finish: Option<Finish>,
    /// Đã gặp `data: [DONE]`.
    done: bool,
}

impl StreamAcc {
    /// Xử lý 1 dòng SSE. Trả về `true` khi gặp `[DONE]`.
    fn push_line<G>(&mut self, line: &[u8], on: &mut G) -> Result<bool, LlmError>
    where
        G: FnMut(StreamSignal<'_>),
    {
        match parse_sse_line(line)? {
            SseLine::Data(d) => {
                if !d.delta.is_empty() {
                    self.out.push_str(&d.delta);
                    on(StreamSignal::Delta(&d.delta));
                }
                if d.usage.is_some() {
                    self.usage = d.usage;
                }
                if d.finish.is_some() {
                    self.finish = d.finish;
                }
                Ok(false)
            }
            SseLine::Done => {
                self.done = true;
                Ok(true)
            }
            SseLine::Skip => Ok(false),
        }
    }

    /// Kết thúc attempt:
    /// - không có text → lỗi `InvalidResponse` (như trước);
    /// - có `[DONE]` → `finish_reason` đã thấy, không thấy thì `Stop` (nhiều proxy không gửi);
    /// - đóng kết nối không có `[DONE]` → `finish_reason` đã thấy nếu có, không thì `Finish::Error`
    ///   (trước đây trả text như bình thường, giờ đánh dấu rõ là có thể thiếu).
    fn take_completion(&mut self) -> Result<Completion, LlmError> {
        if self.out.trim().is_empty() {
            return Err(LlmError::InvalidResponse(
                "stream kết thúc mà không có nội dung".into(),
            ));
        }
        let finish = match (self.done, self.finish) {
            (_, Some(f)) => f,
            (true, None) => Finish::Stop,
            (false, None) => {
                log::warn!("stream đóng kết nối khi chưa có [DONE] / finish_reason; output có thể thiếu");
                Finish::Error
            }
        };
        if finish == Finish::Length {
            log::warn!("output stream bị cắt do max_tokens (finish_reason=length)");
        }
        Ok(Completion {
            text: std::mem::take(&mut self.out),
            usage: self.usage,
            finish,
        })
    }
}

/// max_tokens cho lần retry khi bị cắt: ceil(current × 1.5), tối đa `OUTPUT_TOKENS_CAP`.
fn bumped_max_tokens(current: u32) -> u32 {
    let next = (u64::from(current) * 3).div_ceil(2);
    next.min(u64::from(OUTPUT_TOKENS_CAP)) as u32
}

/// Parse 1 response non-stream có thể chứa `tool_calls`.
fn parse_tool_turn(text: &str) -> Result<ToolTurn, LlmError> {
    let v: Value = serde_json::from_str(text).map_err(|e| {
        LlmError::InvalidResponse(format!("{e}; body: {}", truncate_chars(text, 200)))
    })?;
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        return Err(LlmError::InvalidResponse(format!(
            "provider error: {}",
            truncate_chars(&err.to_string(), MAX_ERROR_BODY_CHARS)
        )));
    }
    let usage = v.get("usage").and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok());
    let msg = v
        .pointer("/choices/0/message")
        .cloned()
        .ok_or_else(|| LlmError::InvalidResponse("không có choices".into()))?;
    let content = msg
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|s| !s.trim().is_empty());
    let tool_calls: Vec<ToolCall> = msg
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|c| {
                    let f = c.get("function")?;
                    Some(ToolCall {
                        id: c.get("id").and_then(Value::as_str).unwrap_or("call").to_owned(),
                        name: f.get("name")?.as_str()?.to_owned(),
                        arguments: match f.get("arguments") {
                            Some(Value::String(s)) => s.clone(),
                            Some(other) => other.to_string(),
                            None => "{}".into(),
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Giữ message gốc nhưng bỏ field null/thừa để provider khác nhận lại được.
    let mut assistant = json!({ "role": "assistant", "content": content.clone().unwrap_or_default() });
    if let Some(tc) = msg.get("tool_calls").filter(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
        assistant["tool_calls"] = tc.clone();
    }
    Ok(ToolTurn {
        content,
        tool_calls,
        assistant_message: assistant,
        usage,
    })
}

async fn read_completion(
    resp: reqwest::Response,
    cancel: &CancellationToken,
) -> Result<Completion, LlmError> {
    let text = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(LlmError::Cancelled),
        t = resp.text() => t.map_err(LlmError::from_reqwest)?,
    };
    parse_completion(&text)
}

/// Parse body non-stream: `choices[0].message.content` + usage + `finish_reason`.
fn parse_completion(text: &str) -> Result<Completion, LlmError> {
    let parsed: ChatResponse = serde_json::from_str(text).map_err(|e| {
        LlmError::InvalidResponse(format!("{e}; body: {}", truncate_chars(text, 200)))
    })?;
    if let Some(err) = parsed.error {
        return Err(LlmError::InvalidResponse(format!(
            "provider error: {}",
            truncate_chars(&err.to_string(), MAX_ERROR_BODY_CHARS)
        )));
    }
    let usage = parsed.usage;
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::InvalidResponse("không có choices".into()))?;
    let finish = Finish::from_reason(choice.finish_reason.as_deref());
    if finish == Finish::Length {
        log::warn!("output bị cắt do max_tokens (finish_reason=length)");
    }
    let content = choice.message.and_then(|m| m.content).unwrap_or_default();
    if content.trim().is_empty() {
        return Err(LlmError::InvalidResponse("content rỗng".into()));
    }
    Ok(Completion {
        text: content,
        usage,
        finish,
    })
}

// ───────────────────────────── SSE ─────────────────────────────

/// 1 chunk `data: {...}` có ít nhất 1 trong: text, usage, finish_reason.
#[derive(Debug, Default, PartialEq)]
struct SseData {
    delta: String,
    /// Chunk cuối (khi gửi `stream_options.include_usage`).
    usage: Option<Usage>,
    /// `Some` khi chunk có `finish_reason` (thường là chunk có delta rỗng).
    finish: Option<Finish>,
}

#[derive(Debug, PartialEq)]
enum SseLine {
    Data(SseData),
    Done,
    Skip,
}

fn parse_sse_line(line: &[u8]) -> Result<SseLine, LlmError> {
    let s = std::str::from_utf8(line)
        .map_err(|e| LlmError::InvalidResponse(format!("SSE không phải UTF-8: {e}")))?
        .trim_end_matches(['\r', '\n']);
    // Bỏ qua comment (`: OPENROUTER PROCESSING`), `event:`, `id:`, dòng trống.
    let Some(data) = s.strip_prefix("data:") else {
        return Ok(SseLine::Skip);
    };
    let data = data.trim_start();
    if data.is_empty() {
        return Ok(SseLine::Skip);
    }
    if data == "[DONE]" {
        return Ok(SseLine::Done);
    }
    let chunk: StreamChunk = serde_json::from_str(data).map_err(|e| {
        LlmError::InvalidResponse(format!(
            "SSE chunk lỗi: {e}; data: {}",
            truncate_chars(data, 200)
        ))
    })?;
    if let Some(err) = chunk.error {
        return Err(LlmError::InvalidResponse(format!(
            "provider error giữa stream: {}",
            truncate_chars(&err.to_string(), MAX_ERROR_BODY_CHARS)
        )));
    }
    let usage = chunk.usage;
    let (delta, finish) = match chunk.choices.into_iter().next() {
        Some(c) => (
            c.delta.and_then(|d| d.content).unwrap_or_default(),
            c.finish_reason
                .as_deref()
                .map(|r| Finish::from_reason(Some(r))),
        ),
        None => (String::new(), None),
    };
    if delta.is_empty() && usage.is_none() && finish.is_none() {
        return Ok(SseLine::Skip);
    }
    Ok(SseLine::Data(SseData {
        delta,
        usage,
        finish,
    }))
}

// ───────────────────────────── JSON extraction ─────────────────────────────

/// Thứ tự thử: nguyên văn → bỏ `<think>` → bỏ code fence → object `{...}` cân bằng đầu tiên.
pub fn parse_json_lenient<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let original = text.trim();
    let mut last_err = match serde_json::from_str::<T>(original) {
        Ok(v) => return Ok(v),
        Err(e) => e.to_string(),
    };
    let no_think = strip_think_blocks(original);
    let candidates = [
        Some(no_think.trim()),
        Some(strip_code_fence(no_think.trim())),
        extract_first_json_object(&no_think),
    ];
    for c in candidates.into_iter().flatten() {
        match serde_json::from_str::<T>(c) {
            Ok(v) => return Ok(v),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(last_err)
}

fn strip_think_blocks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return out, // think không đóng → bỏ phần còn lại
        }
    }
    out.push_str(rest);
    out
}

fn strip_code_fence(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("```") else {
        return s;
    };
    let rest = rest.split_once('\n').map_or("", |(_, body)| body); // bỏ "json"/"markdown" tag
    rest.trim_end().strip_suffix("```").unwrap_or(rest).trim()
}

/// Quét byte, theo dõi string/escape để không nhầm `{` `}` nằm trong chuỗi.
fn extract_first_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let (mut depth, mut in_str, mut escaped) = (0usize, false, false);
    for (i, &b) in s.as_bytes().iter().enumerate().skip(start) {
        if in_str {
            match (escaped, b) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Bỏ code fence bao ngoài toàn bộ document (model hay wrap plan trong ```markdown).
pub fn unwrap_markdown_fence(s: &str) -> String {
    let t = s.trim();
    let lowered_head: String = t.chars().take(16).collect::<String>().to_ascii_lowercase();
    if (lowered_head.starts_with("```markdown") || lowered_head.starts_with("```md"))
        && t.ends_with("```")
    {
        strip_code_fence(t).to_owned()
    } else {
        t.to_owned()
    }
}

// ───────────────────────────── Model list ─────────────────────────────

/// `GET {base_url}/models` (OpenAI-compatible). Không retry — dùng ở màn Settings, báo lỗi ngay.
pub async fn list_models(
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    timeout: Duration,
) -> Result<Vec<String>, LlmError> {
    let url = format!("{}/models", api_root(base_url));
    let mut req = http.get(&url).timeout(timeout);
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        req = req.bearer_auth(key);
        // Anthropic: endpoint OpenAI-compatible cho chat nhận Bearer, nhưng /v1/models cần x-api-key.
        if reqwest::Url::parse(&url).is_ok_and(|u| u.host_str() == Some("api.anthropic.com")) {
            req = req.header("x-api-key", key).header("anthropic-version", "2023-06-01");
        }
    }
    let resp = req.send().await.map_err(LlmError::from_reqwest)?;
    let status = resp.status();
    let text = resp.text().await.map_err(LlmError::from_reqwest)?;
    if !status.is_success() {
        return Err(LlmError::Http {
            status: status.as_u16(),
            body: truncate_chars(text.trim(), MAX_ERROR_BODY_CHARS),
        });
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        LlmError::InvalidResponse(format!("{e}: {}", truncate_chars(text.trim(), 120)))
    })?;
    let models = parse_model_list(&v);
    if models.is_empty() {
        return Err(LlmError::InvalidResponse(
            "endpoint /models không trả về model nào".into(),
        ));
    }
    Ok(models)
}

/// Chấp nhận `{"data":[{"id":..}]}` (OpenAI/OpenRouter/Gemini/Groq), `{"models":[{"name":..}]}`
/// (Ollama `/api/tags`) hoặc mảng thuần. Sắp xếp + bỏ trùng.
pub fn parse_model_list(v: &Value) -> Vec<String> {
    let arr = v
        .get("data")
        .or_else(|| v.get("models"))
        .unwrap_or(v)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out: Vec<String> = arr
        .iter()
        .filter_map(|it| match it {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => ["id", "name", "model"]
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str))
                .map(str::to_owned),
            _ => None,
        })
        // Gemini trả "models/gemini-2.5-pro" — chat completions dùng tên không có tiền tố.
        .map(|s| s.trim().trim_start_matches("models/").to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    out.sort_by_key(|s| s.to_lowercase());
    out.dedup();
    out
}

// ───────────────────────────── Backoff ─────────────────────────────

fn parse_retry_after(h: &HeaderMap) -> Option<Duration> {
    let v = h.get(RETRY_AFTER)?.to_str().ok()?.trim();
    v.parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64)
}

/// 0.5s, 1s, 2s, 4s, 8s (cap) + jitter 0–250ms — tránh thundering herd khi 3 reviewer cùng bị 429.
fn backoff(attempt: u32) -> Duration {
    let base_ms = 500u64.saturating_mul(1u64 << attempt.saturating_sub(1).min(4));
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()) % 250)
        .unwrap_or(0);
    Duration::from_millis(base_ms + jitter)
}

async fn sleep_cancellable(d: Duration, cancel: &CancellationToken) -> Result<(), LlmError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(LlmError::Cancelled),
        _ = tokio::time::sleep(d) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Debug)]
    struct T {
        a: i32,
    }

    #[test]
    fn model_list_shapes() {
        let openai = json!({"object":"list","data":[{"id":"gpt-4.1"},{"id":"gpt-4.1-mini"},{"id":"gpt-4.1"}]});
        assert_eq!(parse_model_list(&openai), ["gpt-4.1", "gpt-4.1-mini"]);
        let ollama = json!({"models":[{"name":"qwen3:8b"},{"name":"Llama3:8b"}]});
        assert_eq!(parse_model_list(&ollama), ["Llama3:8b", "qwen3:8b"]);
        let gemini = json!({"data":[{"id":"models/gemini-2.5-pro"}]});
        assert_eq!(parse_model_list(&gemini), ["gemini-2.5-pro"]);
        assert!(parse_model_list(&json!({"error":"x"})).is_empty());
    }

    #[test]
    fn lenient_json() {
        assert_eq!(parse_json_lenient::<T>(r#"{"a":1}"#).unwrap().a, 1);
        assert_eq!(
            parse_json_lenient::<T>("```json\n{\"a\":2}\n```")
                .unwrap()
                .a,
            2
        );
        assert_eq!(
            parse_json_lenient::<T>("Sure! Here: {\"a\":3, \"s\":\"}{\"} thanks")
                .unwrap()
                .a,
            3
        );
        assert_eq!(
            parse_json_lenient::<T>("<think>{ draft }</think>\n{\"a\":4}")
                .unwrap()
                .a,
            4
        );
        assert!(parse_json_lenient::<T>("no json here").is_err());
    }

    #[test]
    fn markdown_fence_unwrap() {
        assert_eq!(unwrap_markdown_fence("```markdown\n## Hi\n```"), "## Hi");
        // Fence mermaid bên trong document phải được giữ nguyên.
        let doc = "## A\n```mermaid\ngraph TD\n```";
        assert_eq!(unwrap_markdown_fence(doc), doc);
    }

    #[test]
    fn sse_lines() {
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[{\"delta\":{\"content\":\"Xin\"}}]}\r\n").unwrap(),
            SseLine::Data(SseData { delta: "Xin".into(), ..Default::default() })
        );
        assert_eq!(parse_sse_line(b"data: [DONE]\n").unwrap(), SseLine::Done);
        assert_eq!(
            parse_sse_line(b": OPENROUTER PROCESSING\n").unwrap(),
            SseLine::Skip
        );
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[{\"delta\":{}}]}\n").unwrap(),
            SseLine::Skip
        );
        assert!(parse_sse_line(b"data: {\"error\":{\"message\":\"overloaded\"}}\n").is_err());
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3}}\n").unwrap(),
            SseLine::Data(SseData {
                usage: Some(Usage { prompt_tokens: 12, completion_tokens: 3, cached_tokens: 0 }),
                ..Default::default()
            })
        );
    }

    #[test]
    fn sse_finish_chunk_with_empty_delta() {
        // Chunk cuối kiểu OpenAI: delta rỗng, chỉ có finish_reason.
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n").unwrap(),
            SseLine::Data(SseData { finish: Some(Finish::Stop), ..Default::default() })
        );
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n").unwrap(),
            SseLine::Data(SseData { finish: Some(Finish::Length), ..Default::default() })
        );
        // finish_reason: null (các chunk giữa) không tính là đã kết thúc.
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":null}]}\n").unwrap(),
            SseLine::Data(SseData { delta: "a".into(), ..Default::default() })
        );
        // Lý do khác (content_filter…) → Stop.
        assert_eq!(
            parse_sse_line(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"content_filter\"}]}\n").unwrap(),
            SseLine::Data(SseData { finish: Some(Finish::Stop), ..Default::default() })
        );
    }

    /// Đẩy cả body SSE qua `StreamAcc` giống `consume_stream` (không cần HTTP).
    fn run_stream(body: &str) -> (Result<Completion, LlmError>, String) {
        let mut acc = StreamAcc::default();
        let mut seen = String::new();
        let mut on = |s: StreamSignal<'_>| {
            if let StreamSignal::Delta(d) = s {
                seen.push_str(d);
            }
        };
        for line in body.split_inclusive('\n') {
            if acc.push_line(line.as_bytes(), &mut on).unwrap() {
                break;
            }
        }
        (acc.take_completion(), seen)
    }

    #[test]
    fn stream_length_maps_to_length() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"a\\\":\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":7}}\n\n",
            "data: [DONE]\n\n",
        );
        let (c, seen) = run_stream(body);
        let c = c.unwrap();
        assert_eq!(c.finish, Finish::Length);
        assert_eq!(c.text, "{\"a\":");
        assert_eq!(seen, c.text);
        assert_eq!(c.usage, Some(Usage { prompt_tokens: 5, completion_tokens: 7, cached_tokens: 0 }));
    }

    #[test]
    fn stream_done_without_reason_is_stop() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Xin \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"chào\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        let c = run_stream(body).0.unwrap();
        assert_eq!(c.finish, Finish::Stop);
        assert_eq!(c.text, "Xin chào");
        assert_eq!(c.usage, None);
    }

    #[test]
    fn stream_closed_without_done() {
        // Không [DONE], không finish_reason → Error (text có thể thiếu).
        let c = run_stream("data: {\"choices\":[{\"delta\":{\"content\":\"dở\"}}]}\n").0.unwrap();
        assert_eq!(c.finish, Finish::Error);
        assert_eq!(c.text, "dở");
        // Có finish_reason nhưng proxy không gửi [DONE] → tin finish_reason.
        let c = run_stream("data: {\"choices\":[{\"delta\":{\"content\":\"xong\"},\"finish_reason\":\"stop\"}]}\n")
            .0
            .unwrap();
        assert_eq!(c.finish, Finish::Stop);
        // Không có text → vẫn lỗi như cũ.
        assert!(run_stream("data: [DONE]\n").0.is_err());
    }

    #[test]
    fn non_stream_finish_reason() {
        let c = parse_completion(
            r#"{"choices":[{"message":{"content":"{\"a\":"},"finish_reason":"length"}],
                "usage":{"prompt_tokens":9,"completion_tokens":4096}}"#,
        )
        .unwrap();
        assert_eq!(c.finish, Finish::Length);
        assert_eq!(c.usage, Some(Usage { prompt_tokens: 9, completion_tokens: 4096, cached_tokens: 0 }));
        let c = parse_completion(r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#).unwrap();
        assert_eq!(c.finish, Finish::Stop);
        // Thiếu finish_reason → Stop.
        let c = parse_completion(r#"{"choices":[{"message":{"content":"ok"}}]}"#).unwrap();
        assert_eq!(c.finish, Finish::Stop);
    }

    #[test]
    fn cached_tokens_are_parsed_and_summed() {
        let c = parse_completion(
            r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":100,"completion_tokens":7,"prompt_tokens_details":{"cached_tokens":64}}}"#,
        )
        .unwrap();
        assert_eq!(c.usage, Some(Usage { prompt_tokens: 100, completion_tokens: 7, cached_tokens: 64 }));
        // details = null / thiếu cached_tokens → 0; cached > prompt → kẹp lại.
        let u: Usage = serde_json::from_str(r#"{"prompt_tokens":5,"completion_tokens":1,"prompt_tokens_details":null}"#).unwrap();
        assert_eq!(u.cached_tokens, 0);
        let u: Usage = serde_json::from_str(r#"{"prompt_tokens":5,"prompt_tokens_details":{"cached_tokens":9}}"#).unwrap();
        assert_eq!((u.cached_tokens, u.completion_tokens), (5, 0));
        // Chunk usage cuối của stream cũng đọc được cache.
        let s = run_stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"x\"},\"finish_reason\":\"stop\"}]}\n\n\
             data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":4}}}\n\n\
             data: [DONE]\n\n",
        )
        .0
        .unwrap();
        assert_eq!(s.usage.map(|u| u.cached_tokens), Some(4));
        let sum = sum_usage(s.usage, c.usage).unwrap();
        assert_eq!((sum.prompt_tokens, sum.cached_tokens, sum.completion_tokens), (110, 68, 9));
    }

    #[test]
    fn bumped_max_tokens_caps() {
        assert_eq!(bumped_max_tokens(DEFAULT_OUTPUT_TOKENS), 6144);
        assert_eq!(bumped_max_tokens(1001), 1502); // ceil(1501.5)
        assert_eq!(bumped_max_tokens(30_000), OUTPUT_TOKENS_CAP);
    }

    #[test]
    fn tool_turn_parsing() {
        let t = parse_tool_turn(
            r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
                {"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"README.md\"}"}}]}}],
                "usage":{"prompt_tokens":50,"completion_tokens":5}}"#,
        )
        .unwrap();
        assert_eq!(t.tool_calls.len(), 1);
        assert_eq!(t.tool_calls[0].name, "read_file");
        assert_eq!(t.tool_calls[0].arguments, r#"{"path":"README.md"}"#);
        assert_eq!(t.usage, Some(Usage { prompt_tokens: 50, completion_tokens: 5, cached_tokens: 0 }));
        assert!(t.content.is_none());
        assert_eq!(t.assistant_message["tool_calls"][0]["id"], "c1");

        let done = parse_tool_turn(r#"{"choices":[{"message":{"content":"notes"}}]}"#).unwrap();
        assert!(done.tool_calls.is_empty());
        assert_eq!(done.content.as_deref(), Some("notes"));
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert!(backoff(1) < Duration::from_millis(800));
        assert!(backoff(3) >= Duration::from_millis(2000));
        assert!(backoff(20) <= Duration::from_millis(8250));
    }

    #[test]
    fn retryability() {
        assert!(LlmError::Http {
            status: 429,
            body: String::new()
        }
        .is_retryable());
        assert!(!LlmError::Http {
            status: 401,
            body: String::new()
        }
        .is_retryable());
        assert!(!LlmError::Cancelled.is_retryable());
    }
}
