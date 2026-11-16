//! Kiểu dữ liệu dùng chung không thuộc riêng domain nào: token usage + helper cắt chuỗi.
//! Domain tarot (Reading, Draw, ReaderOutput…) nằm ở các module riêng.

use serde::{Deserialize, Serialize};

/// Token tiêu thụ cộng dồn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Phần `prompt_tokens` provider đọc từ cache.
    pub cached_tokens: u64,
    pub calls: u32,
    /// Số lần gọi mà provider không trả `usage` → token được ước lượng theo độ dài văn bản.
    pub estimated_calls: u32,
}

impl TokenUsage {
    pub fn add(&mut self, prompt: u64, cached: u64, completion: u64, estimated: bool) {
        self.prompt_tokens += prompt;
        self.cached_tokens += cached;
        self.completion_tokens += completion;
        self.calls += 1;
        if estimated {
            self.estimated_calls += 1;
        }
    }
}

/// Token của 1 lần gọi LLM (để tính chi phí lúc đọc theo bảng giá hiện tại).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallUsage {
    /// Số thứ tự lần gọi trong lượt đọc (0, 1, 2…).
    pub call_id: u32,
    /// Thầy bói đã gọi (id persona).
    pub agent: String,
    /// "reader" | "reader_retry" | "synthesis" | "synthesis_retry"
    pub kind: String,
    pub model: String,
    pub prompt: u64,
    #[serde(default)]
    pub cached: u64,
    pub completion: u64,
    /// true = provider không trả `usage`, token được ước lượng theo độ dài.
    pub estimated: bool,
    /// "stop" | "length" | "error"
    pub finish: String,
    pub latency_ms: u64,
}

/// Cắt theo char (không theo byte) để an toàn với tiếng Việt có dấu / emoji.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_owned(),
        Some((idx, _)) => format!("{}…", &s[..idx]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate_chars("Ngôi Sao", 4), "Ngôi…");
        assert_eq!(truncate_chars("abc", 5), "abc");
    }

    #[test]
    fn token_usage_accumulates() {
        let mut u = TokenUsage::default();
        u.add(10, 2, 5, false);
        u.add(1, 0, 1, true);
        assert_eq!((u.prompt_tokens + u.completion_tokens, u.calls, u.estimated_calls, u.cached_tokens), (17, 2, 1, 2));
    }
}
