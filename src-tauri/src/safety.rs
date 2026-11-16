//! Lớp an toàn 1 (plan §5): lọc câu hỏi (và output của mô hình) theo cụm từ trên văn bản đã
//! chuẩn hoá — viết thường, bỏ dấu tiếng Việt (`đ` → `d`), mọi ký tự không phải chữ/số → khoảng trắng,
//! gộp khoảng trắng. So khớp theo ranh giới từ.
//!
//! GIỚI HẠN: bộ lọc từ khoá CÓ THỂ BỎ SÓT. Vì vậy UI luôn có lối vào "Cần hỗ trợ?" và mô hình còn
//! có cờ `crisis` (lớp 2). Bắt nhầm chỉ khiến người dùng phải diễn đạt lại câu hỏi.

use serde::{Deserialize, Serialize};

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

/// Cụm từ ĐÃ chuẩn hoá (không dấu, viết thường). Thêm mẫu mới → thêm vào corpus test.
const SELF_HARM: &[&str] = &[
    // vi
    "tu tu", "tu sat", "tu van minh", "muon chet", "muon di chet", "chet di cho xong", "chet cho xong",
    "khong muon song", "khong muon song nua", "chan song", "het muon song", "song de lam gi", "song lam gi nua",
    "ket thuc cuoc doi", "ket thuc cuoc song", "ket lieu ban than", "ket lieu doi minh", "tu ket lieu",
    "giai thoat khoi cuoc doi", "roi khoi the gioi nay", "bien mat vinh vien", "khong con ton tai nua",
    "nhay lau", "nhay cau", "uong thuoc ngu", "uong thuoc tu", "rach tay", "cat tay", "cat co tay", "treo co",
    "tu hai", "tu lam dau ban than", "lam hai ban than", "hai ban than", "tu gay thuong tich",
    "muon bien mat", "khong ai can minh", "ca the gioi se tot hon neu khong co minh",
    // tiếng lóng / viết tắt phổ biến
    "unalive", "kms", "tu tu cho roi",
    // en
    "suicide", "suicidal", "kill myself", "killing myself", "end my life", "ending my life", "want to die",
    "wanna die", "self harm", "selfharm", "hurt myself", "cut myself", "cutting myself", "take my own life",
    "no reason to live", "better off dead", "overdose",
];

const MEDICAL: &[&str] = &[
    "benh", "ung thu", "khoi u", "chan doan", "xet nghiem", "phau thuat", "mo tim", "dieu tri", "hoa tri",
    "uong thuoc", "thuoc men", "ke don thuoc", "bac si", "benh vien", "mang thai", "co thai", "sinh no", "say thai", "tram cam", "roi loan",
    "tieu duong", "huyet ap", "covid", "suc khoe",
    "cancer", "tumor", "diagnosis", "surgery", "pregnant", "pregnancy", "doctor", "hospital", "disease",
    "medication", "depression",
];

const FINANCIAL: &[&str] = &[
    "co phieu", "chung khoan", "dau tu", "crypto", "bitcoin", "tien ao", "coin", "forex", "mua vang", "bat dong san",
    "vay tien", "vay no", "no nan", "lai suat", "lo de", "xo so", "ve so", "danh bac", "ca do", "ca cuoc",
    "stock", "stocks", "invest", "investment", "trading", "lottery", "gamble", "gambling",
];

const LEGAL: &[&str] = &[
    "kien tung", "khoi kien", "bi kien", "kien ra toa", "toa an", "phien toa", "ban an", "luat su", "phap ly",
    "hop dong", "ly hon", "tranh chap", "quyen nuoi con", "di chuc", "thua ke", "bi bat", "cong an", "hinh su",
    "lawsuit", "sue", "court", "lawyer", "attorney", "divorce", "custody", "contract", "arrested",
];

/// Chuẩn hoá: fold (bỏ dấu + lowercase) → ký tự không phải chữ/số thành khoảng trắng → gộp khoảng trắng.
pub fn normalize(text: &str) -> String {
    let (folded, _) = fold(text);
    let spaced: String = folded.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contains_phrase(padded: &str, phrase: &str) -> bool {
    padded.contains(&format!(" {phrase} "))
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
    let padded = format!(" {} ", normalize(text));
    let mut categories = Vec::new();
    for (cat, list) in [
        (Category::SelfHarm, SELF_HARM),
        (Category::Medical, MEDICAL),
        (Category::Financial, FINANCIAL),
        (Category::Legal, LEGAL),
    ] {
        if list.iter().any(|p| contains_phrase(&padded, p)) {
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
