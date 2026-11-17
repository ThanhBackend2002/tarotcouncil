//! Lớp an toàn 1 (plan §5): lọc câu hỏi (và output của mô hình) theo cụm từ trên văn bản đã
//! chuẩn hoá — viết thường, bỏ dấu tiếng Việt (`đ` → `d`), mọi ký tự không phải chữ/số → khoảng trắng,
//! gộp khoảng trắng. So khớp theo ranh giới từ.
//!
//! GIỚI HẠN: bộ lọc từ khoá CÓ THỂ BỎ SÓT. Vì vậy UI luôn có lối vào "Cần hỗ trợ?" và mô hình còn
//! có cờ `crisis` (lớp 2). Bắt nhầm chỉ khiến người dùng phải diễn đạt lại câu hỏi.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::normalize::fold;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Chặn: mở SupportScreen, không đọc bài.
    SelfHarm,
    /// Vẫn đọc bài, nhưng cấm chẩn đoán + disclaimer bắt buộc.
    Medical,
    Financial,
    Legal,
}

/// Cụm từ mẫu (viết thường, có dấu chuẩn). Khi cửa sổ từ trong văn bản đầu vào có dấu tiếng Việt,
/// so khớp chính xác có dấu (tránh bắt nhầm "từ từ" → "tự tử", "hai bạn thân" → "hại bản thân",
/// "mở tim" → "mổ tim", "cả cuộc đời" → "cá cược"); khi cửa sổ đầu vào gõ không dấu toàn bộ thì
/// so khớp trên bản gấp không dấu.
const SELF_HARM: &[&str] = &[
    // vi
    "tự tử", "tự sát", "tự vẫn mình", "muốn chết", "muốn đi chết", "chết đi cho xong", "chết cho xong",
    "không muốn sống", "không muốn sống nữa", "chán sống", "hết muốn sống", "sống để làm gì", "sống làm gì nữa",
    "kết thúc cuộc đời", "kết thúc cuộc sống", "kết liễu bản thân", "kết liễu đời mình", "tự kết liễu",
    "giải thoát khỏi cuộc đời", "rời khỏi thế giới này", "biến mất vĩnh viễn", "không còn tồn tại nữa",
    "nhảy lầu", "nhảy cầu", "uống thuốc ngủ", "uống thuốc tự", "rạch tay", "cắt tay", "cắt cổ tay", "treo cổ",
    "tự hại", "tự làm đau bản thân", "làm hại bản thân", "hại bản thân", "tự gây thương tích",
    "muốn biến mất", "không ai cần mình", "cả thế giới sẽ tốt hơn nếu không có mình",
    // tiếng lóng / viết tắt phổ biến
    "unalive", "kms", "tự tử cho rồi",
    // en
    "suicide", "suicidal", "kill myself", "killing myself", "end my life", "ending my life", "want to die",
    "wanna die", "self harm", "selfharm", "hurt myself", "cut myself", "cutting myself", "take my own life",
    "no reason to live", "better off dead", "overdose",
];

const MEDICAL: &[&str] = &[
    "bệnh", "ung thư", "khối u", "chẩn đoán", "xét nghiệm", "phẫu thuật", "mổ tim", "điều trị", "hoá trị", "hóa trị",
    "uống thuốc", "thuốc men", "kê đơn thuốc", "bác sĩ", "bệnh viện", "mang thai", "có thai", "sinh nở", "sảy thai", "trầm cảm", "rối loạn",
    "tiểu đường", "huyết áp", "covid", "sức khoẻ", "sức khỏe",
    "cancer", "tumor", "diagnosis", "surgery", "pregnant", "pregnancy", "doctor", "hospital", "disease",
    "medication", "depression",
];

const FINANCIAL: &[&str] = &[
    "cổ phiếu", "chứng khoán", "đầu tư", "crypto", "bitcoin", "tiền ảo", "coin", "forex", "mua vàng", "bất động sản",
    "vay tiền", "vay nợ", "nợ nần", "lãi suất", "lô đề", "xổ số", "vé số", "đánh bạc", "cá độ", "cá cược",
    "stock", "stocks", "invest", "investment", "trading", "lottery", "gamble", "gambling",
];

const LEGAL: &[&str] = &[
    "kiện tụng", "khởi kiện", "bị kiện", "kiện ra toà", "kiện ra tòa", "toà án", "tòa án", "phiên toà", "phiên tòa", "bản án", "luật sư", "pháp lý",
    "hợp đồng", "ly hôn", "tranh chấp", "quyền nuôi con", "di chúc", "thừa kế", "bị bắt", "công an", "hình sự",
    "lawsuit", "sue", "court", "lawyer", "attorney", "divorce", "custody", "contract", "arrested",
];

/// Chuẩn hoá: fold (bỏ dấu + lowercase) → ký tự không phải chữ/số thành khoảng trắng → gộp khoảng trắng.
pub fn normalize(text: &str) -> String {
    let (folded, _) = fold(text);
    let spaced: String = folded.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Token {
    raw: String,
    folded: String,
}

fn tokenize(text: &str) -> Vec<Token> {
    let nfc: String = text.nfc().collect();
    let lower = nfc.to_lowercase();
    let spaced: String = lower.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    spaced
        .split_whitespace()
        .map(|w| Token { raw: w.to_owned(), folded: fold(w).0 })
        .collect()
}

fn contains_phrase(tokens: &[Token], phrase: &str) -> bool {
    let target: Vec<&str> = phrase.split_whitespace().collect();
    if target.is_empty() || tokens.len() < target.len() {
        return false;
    }
    let target_folded: Vec<String> = target.iter().map(|w| fold(w).0).collect();
    tokens.windows(target.len()).any(|win| {
        if !win.iter().zip(&target_folded).all(|(t, tf)| t.folded == *tf) {
            return false;
        }
        // Nếu cụm từ trong văn bản có dấu tiếng Việt thì đòi hỏi khớp đúng dấu;
        // nếu người dùng gõ hoàn toàn không dấu ở cụm này thì khớp qua bản gấp không dấu.
        let has_diacritics = win.iter().any(|t| t.raw != t.folded);
        !has_diacritics || win.iter().zip(&target).all(|(t, tw)| t.raw == *tw)
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Scan {
    /// Nhóm bị trúng (sắp xếp, không trùng).
    pub categories: Vec<Category>,
}

impl Scan {
    pub fn self_harm(&self) -> bool {
        self.categories.contains(&Category::SelfHarm)
    }
    /// Nhóm cần disclaimer (y tế / tài chính / pháp lý).
    pub fn sensitive(&self) -> Vec<Category> {
        self.categories.iter().copied().filter(|c| *c != Category::SelfHarm).collect()
    }
}

pub fn scan(text: &str) -> Scan {
    let tokens = tokenize(text);
    let mut categories = Vec::new();
    for (cat, list) in [
        (Category::SelfHarm, SELF_HARM),
        (Category::Medical, MEDICAL),
        (Category::Financial, FINANCIAL),
        (Category::Legal, LEGAL),
    ] {
        if list.iter().any(|p| contains_phrase(&tokens, p)) {
            categories.push(cat);
        }
    }
    Scan { categories }
}

/// Câu disclaimer bắt buộc cho từng nhóm nhạy cảm.
pub fn disclaimer(cats: &[Category]) -> String {
    let mut parts = vec!["Lượt đọc chỉ mang tính giải trí và gợi ý suy ngẫm, không phải lời tiên đoán.".to_owned()];
    for c in cats {
        parts.push(
            match c {
                Category::Medical => "Đây không phải lời khuyên y tế — hãy hỏi bác sĩ hoặc chuyên gia y tế về sức khoẻ của bạn.",
                Category::Financial => "Đây không phải lời khuyên tài chính hay đầu tư — hãy tham khảo chuyên gia trước khi quyết định về tiền bạc.",
                Category::Legal => "Đây không phải tư vấn pháp lý — hãy hỏi luật sư hoặc cơ quan có thẩm quyền.",
                Category::SelfHarm => continue,
            }
            .to_owned(),
        );
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        text: String,
        /// "self_harm" | "medical" | "financial" | "legal" | "none"
        expect: Vec<String>,
    }

    /// Corpus mẫu dương + mẫu âm (plan §5 — `tests/safety_vi.jsonl`).
    #[test]
    fn corpus() {
        let mut fails = Vec::new();
        for (i, line) in include_str!("../tests/safety_vi.jsonl").lines().enumerate() {
            if line.trim().is_empty() || line.starts_with("//") {
                continue;
            }
            let c: Case = serde_json::from_str(line).unwrap_or_else(|e| panic!("dòng {}: {e}", i + 1));
            let got: Vec<String> = scan(&c.text)
                .categories
                .iter()
                .map(|c| serde_json::to_value(c).unwrap().as_str().unwrap().to_owned())
                .collect();
            let want: Vec<String> = c.expect.into_iter().filter(|e| e != "none").collect();
            if got != want {
                fails.push(format!("dòng {}: {:?} → {got:?}, mong {want:?}", i + 1, c.text));
            }
        }
        assert!(fails.is_empty(), "{}", fails.join("\n"));
    }

    #[test]
    fn normalization_handles_diacritics_case_and_punctuation() {
        assert_eq!(normalize("  Tôi MUỐN   chết!!! "), "toi muon chet");
        assert_eq!(normalize("Đường-đời"), "duong doi");
        assert!(scan("TỰ TỬ").self_harm());
        assert!(scan("tu.tu").self_harm());
    }

    #[test]
    fn disclaimer_mentions_each_sensitive_category() {
        let d = disclaimer(&[Category::Medical, Category::Legal]);
        assert!(d.contains("y tế") && d.contains("pháp lý") && !d.contains("đầu tư"));
    }
}
