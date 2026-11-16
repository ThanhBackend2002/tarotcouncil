//! Bộ bài 78 lá Rider–Waite–Smith, nạp từ `assets/deck_vi.json` lúc biên dịch (`include_str!`).
//!
//! Chỉ số "canonical" của lá = vị trí trong file (0–21 Ẩn Chính, rồi Gậy, Cốc, Kiếm, Tiền — mỗi
//! chất Át→Vua). `draw.rs` hoán vị các chỉ số này; đổi thứ tự file = đổi `deck_version`.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const DECK_JSON: &str = include_str!("../assets/deck_vi.json");
pub const DECK_SIZE: usize = 78;
/// Plan §4 — Nghiệm thu bộ bài: nghĩa mỗi chiều dài 60–100 từ.
pub const MEANING_WORDS: std::ops::RangeInclusive<usize> = 60..=100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arcana {
    Major,
    Minor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub arcana: Arcana,
    /// "wands" | "cups" | "swords" | "pentacles"; None với Ẩn Chính.
    pub suit: Option<String>,
    /// 0–21 với Ẩn Chính; 1–14 với Ẩn Phụ (Page=11 … King=14).
    pub rank: u8,
    pub name_vi: String,
    pub name_en: String,
    pub keywords_upright: Vec<String>,
    pub keywords_reversed: Vec<String>,
    pub meaning_upright: String,
    pub meaning_reversed: String,
    /// "fire" | "water" | "air" | "earth"
    pub element: String,
    /// Khoá ảnh (vd "m00", "c13") — UI map sang file trong `src/assets/cards/`.
    pub art_key: String,
}

impl Card {
    pub fn meaning(&self, reversed: bool) -> &str {
        if reversed { &self.meaning_reversed } else { &self.meaning_upright }
    }
    pub fn keywords(&self, reversed: bool) -> &[String] {
        if reversed { &self.keywords_reversed } else { &self.keywords_upright }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deck {
    pub deck_version: u32,
    pub cards: Vec<Card>,
}

impl Deck {
    pub fn card(&self, canonical: u8) -> &Card {
        &self.cards[canonical as usize]
    }
    pub fn by_id(&self, id: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.id == id)
    }
}

pub fn parse(json: &str) -> Result<Deck, String> {
    serde_json::from_str(json).map_err(|e| format!("deck_vi.json không hợp lệ: {e}"))
}

/// Kiểm tra cấu trúc + nội dung (plan §4 — Nghiệm thu bộ bài). Trả mọi lỗi cùng lúc.
pub fn validate(d: &Deck) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    if d.cards.len() != DECK_SIZE {
        errs.push(format!("cần {DECK_SIZE} lá, có {}", d.cards.len()));
    }
    let mut ids = HashSet::new();
    let mut majors = HashSet::new();
    let mut minors: HashSet<(String, u8)> = HashSet::new();
    for c in &d.cards {
        if !ids.insert(c.id.as_str()) {
            errs.push(format!("id trùng: {}", c.id));
        }
        for (field, v) in [
            ("name_vi", &c.name_vi),
            ("name_en", &c.name_en),
            ("element", &c.element),
            ("art_key", &c.art_key),
            ("meaning_upright", &c.meaning_upright),
            ("meaning_reversed", &c.meaning_reversed),
        ] {
            if v.trim().is_empty() {
                errs.push(format!("{}: {field} rỗng", c.id));
            }
        }
        if c.keywords_upright.is_empty() || c.keywords_reversed.is_empty() || c.keywords_upright.iter().chain(&c.keywords_reversed).any(|k| k.trim().is_empty()) {
            errs.push(format!("{}: thiếu từ khoá", c.id));
        }
        for (dir, m) in [("xuôi", &c.meaning_upright), ("ngược", &c.meaning_reversed)] {
            let n = m.split_whitespace().count();
            if !MEANING_WORDS.contains(&n) {
                errs.push(format!("{}: nghĩa {dir} dài {n} từ (cần 60–100)", c.id));
            }
        }
        match (c.arcana, c.suit.as_deref()) {
            (Arcana::Major, None) if c.rank <= 21 => {
                majors.insert(c.rank);
            }
            (Arcana::Minor, Some(s @ ("wands" | "cups" | "swords" | "pentacles"))) if (1..=14).contains(&c.rank) => {
                minors.insert((s.to_owned(), c.rank));
            }
            _ => errs.push(format!("{}: arcana/suit/rank không hợp lệ", c.id)),
        }
    }
    if majors.len() != 22 {
        errs.push(format!("Ẩn Chính cần đủ rank 0–21, có {}", majors.len()));
    }
    if minors.len() != 56 {
        errs.push(format!("Ẩn Phụ cần 4 chất × 14 rank, có {}", minors.len()));
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

/// Bộ bài đã nạp + validate (panic nếu file nhúng hỏng — được chặn bởi test trong CI).
pub fn deck() -> &'static Deck {
    static DECK: OnceLock<Deck> = OnceLock::new();
    DECK.get_or_init(|| {
        let d = parse(DECK_JSON).expect("deck_vi.json nhúng phải hợp lệ");
        if let Err(e) = validate(&d) {
            panic!("deck_vi.json không đạt nghiệm thu: {}", e.join("; "));
        }
        d
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan §4 — test tự động nghiệm thu bộ bài (78 lá, id duy nhất, không rỗng, 60–100 từ).
    #[test]
    fn embedded_deck_passes_acceptance() {
        let d = parse(DECK_JSON).unwrap();
        validate(&d).unwrap_or_else(|e| panic!("{}", e.join("\n")));
        assert_eq!(deck().cards.len(), DECK_SIZE);
        assert_eq!(d.card(0).name_vi, "Chàng Khờ");
        assert_eq!(d.card(21).name_en, "The World");
        assert_eq!(d.by_id("cups-queen").unwrap().rank, 13);
    }

    #[test]
    fn validation_reports_every_problem() {
        let mut d = parse(DECK_JSON).unwrap();
        d.cards[1].id = d.cards[0].id.clone();
        d.cards[2].meaning_upright = "quá ngắn".into();
        d.cards[3].keywords_reversed.clear();
        d.cards.pop();
        let e = validate(&d).unwrap_err().join("\n");
        for needle in ["cần 78 lá", "id trùng", "dài 2 từ", "thiếu từ khoá", "Ẩn Phụ cần"] {
            assert!(e.contains(needle), "thiếu '{needle}' trong:\n{e}");
        }
    }
}
