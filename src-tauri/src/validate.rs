//! Hậu kiểm output của mô hình (plan §3 — Hậu kiểm và chống bịa lá).
//!
//! **Kiểm tra bảo đảm** (quyết định đạt/trượt): schema, số vị trí = số lá, từng cặp
//! `position`/`card_id` khớp `draws`, độ dài hợp lý.
//! **Tín hiệu bổ trợ** (KHÔNG phải bảo đảm): dò tên lá ngoài lượt bốc — chỉ mẫu "lá {name_vi}"
//! và tên tiếng Anh nguyên cụm, để tránh bắt nhầm cụm thường như "mặt trời", "công lý".

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::deck::Deck;
use crate::reading::Draw;
use crate::safety;

pub const MAX_INTERPRETATION_CHARS: usize = 1500;
pub const MAX_OVERALL_CHARS: usize = 2000;
pub const MAX_LIST_ITEMS: usize = 6;
pub const MAX_ITEM_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Hopeful,
    Cautionary,
    #[default]
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionReading {
    pub position: u8,
    pub card_id: String,
    pub interpretation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReaderOutput {
    pub positions: Vec<PositionReading>,
    pub overall: String,
    #[serde(default)]
    pub tone: Tone,
    #[serde(default)]
    pub crisis: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Synthesis {
    pub summary: String,
    #[serde(default)]
    pub agreements: Vec<String>,
    #[serde(default)]
    pub divergences: Vec<String>,
    #[serde(default)]
    pub advice: Vec<String>,
    #[serde(default)]
    pub disclaimer: String,
    #[serde(default)]
    pub crisis: bool,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// Kiểm tra bảo đảm cho output của 1 thầy bói. `Err` = danh sách lỗi để gửi lại trong lượt sửa.
pub fn check_reader(out: &ReaderOutput, draws: &[Draw]) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    if out.positions.len() != draws.len() {
        errs.push(format!("cần đúng {} mục trong positions, nhận {}", draws.len(), out.positions.len()));
    }
    let mut seen = HashSet::new();
    for p in &out.positions {
        if !seen.insert(p.position) {
            errs.push(format!("position {} bị lặp", p.position));
        }
        match draws.iter().find(|d| d.position == p.position) {
            None => errs.push(format!("position {} không có trong lượt bốc", p.position)),
            Some(d) if d.card_id != p.card_id => {
                errs.push(format!("position {}: card_id phải là \"{}\" (nhận \"{}\")", p.position, d.card_id, p.card_id))
            }
            Some(_) => {}
        }
        let n = chars(p.interpretation.trim());
        if n == 0 || n > MAX_INTERPRETATION_CHARS {
            errs.push(format!("position {}: interpretation phải có 1–{MAX_INTERPRETATION_CHARS} ký tự", p.position));
        }
    }
    let n = chars(out.overall.trim());
    if n == 0 || n > MAX_OVERALL_CHARS {
        errs.push(format!("overall phải có 1–{MAX_OVERALL_CHARS} ký tự"));
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

pub fn check_synthesis(s: &Synthesis) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    let n = chars(s.summary.trim());
    if n == 0 || n > MAX_OVERALL_CHARS {
        errs.push(format!("summary phải có 1–{MAX_OVERALL_CHARS} ký tự"));
    }
    if s.advice.is_empty() {
        errs.push("advice cần ít nhất 1 mục".into());
    }
    for (name, list) in [("agreements", &s.agreements), ("divergences", &s.divergences), ("advice", &s.advice)] {
        if list.len() > MAX_LIST_ITEMS {
            errs.push(format!("{name} tối đa {MAX_LIST_ITEMS} mục"));
        }
        if list.iter().any(|x| x.trim().is_empty() || chars(x) > MAX_ITEM_CHARS) {
            errs.push(format!("{name}: mỗi mục 1–{MAX_ITEM_CHARS} ký tự"));
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

/// Toàn bộ văn bản của output (để chạy cổng an toàn + dò tên lá).
pub fn reader_text(out: &ReaderOutput) -> String {
    let mut t: Vec<&str> = out.positions.iter().map(|p| p.interpretation.as_str()).collect();
    t.push(&out.overall);
    t.join("\n")
}

pub fn synthesis_text(s: &Synthesis) -> String {
    let mut t = vec![s.summary.as_str()];
    t.extend(s.agreements.iter().map(String::as_str));
    t.extend(s.divergences.iter().map(String::as_str));
    t.extend(s.advice.iter().map(String::as_str));
    t.join("\n")
}

/// Cổng an toàn cho output: cờ `crisis` của mô hình HOẶC lọc từ khoá tự hại trên văn bản.
pub fn is_crisis(flag: bool, text: &str) -> bool {
    flag || safety::scan(text).self_harm()
}

/// Tín hiệu bổ trợ: id các lá NGOÀI lượt bốc được nhắc tới theo mẫu "lá {name_vi}" hoặc tên tiếng Anh.
pub fn foreign_cards(text: &str, draws: &[Draw], deck: &Deck) -> Vec<String> {
    let padded = format!(" {} ", safety::normalize(text));
    let drawn: HashSet<&str> = draws.iter().map(|d| d.card_id.as_str()).collect();
    let mut out = Vec::new();
    for c in deck.cards.iter().filter(|c| !drawn.contains(c.id.as_str())) {
        let vi = format!(" la {} ", safety::normalize(&c.name_vi));
        let en = format!(" {} ", safety::normalize(&c.name_en));
        if padded.contains(&vi) || padded.contains(&en) {
            out.push(c.id.clone());
        }
    }
    // "lá Hai Gậy" cũng chứa "lá Hai Gậy…" của lá khác? Không: tên đầy đủ có ranh giới từ hai đầu.
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::deck;

    pub(crate) fn draws(ids: &[&str]) -> Vec<Draw> {
        ids.iter()
            .enumerate()
            .map(|(i, id)| {
                let c = deck().by_id(id).unwrap();
                Draw {
                    position: i as u8,
                    position_label_vi: format!("Vị trí {i}"),
                    fan_index: i as u8,
                    card_id: c.id.clone(),
                    name_vi: c.name_vi.clone(),
                    name_en: c.name_en.clone(),
                    reversed: false,
                    art_key: c.art_key.clone(),
                }
            })
            .collect()
    }

    fn out(ps: &[(u8, &str)]) -> ReaderOutput {
        ReaderOutput {
            positions: ps.iter().map(|(p, id)| PositionReading { position: *p, card_id: (*id).into(), interpretation: "ý nghĩa".into() }).collect(),
            overall: "tổng quan".into(),
            tone: Tone::Neutral,
            crisis: false,
        }
    }

    #[test]
    fn guaranteed_checks() {
        let d = draws(&["major-19", "cups-ace", "swords-10"]);
        check_reader(&out(&[(0, "major-19"), (1, "cups-ace"), (2, "swords-10")]), &d).unwrap();
        let e = check_reader(&out(&[(0, "major-19"), (1, "major-13"), (1, "cups-ace")]), &d).unwrap_err().join("; ");
        assert!(e.contains("card_id phải là \"cups-ace\"") && e.contains("bị lặp") && !e.contains("cần đúng"), "{e}");
        let e = check_reader(&out(&[(0, "major-19"), (5, "cups-ace")]), &d).unwrap_err().join("; ");
        assert!(e.contains("cần đúng 3") && e.contains("position 5 không có"), "{e}");
        let mut empty = out(&[(0, "major-19"), (1, "cups-ace"), (2, "swords-10")]);
        empty.overall = " ".into();
        assert!(check_reader(&empty, &d).is_err());
    }

    #[test]
    fn synthesis_checks() {
        let ok = Synthesis { summary: "s".into(), agreements: vec![], divergences: vec![], advice: vec!["a".into()], disclaimer: String::new(), crisis: false };
        check_synthesis(&ok).unwrap();
        let bad = Synthesis { advice: vec![], summary: "".into(), ..ok };
        assert_eq!(check_synthesis(&bad).unwrap_err().len(), 2);
    }

    #[test]
    fn crisis_gate_uses_flag_or_keywords() {
        assert!(is_crisis(true, "bình thường"));
        assert!(is_crisis(false, "nếu bạn đang nghĩ tới chuyện tự tử, hãy gọi người thân"));
        assert!(!is_crisis(false, "Lá Cái Chết nói về sự chuyển hoá"));
    }

    #[derive(serde::Deserialize)]
    struct Case {
        text: String,
        drawn: Vec<String>,
        foreign: Vec<String>,
    }

    /// Corpus đo dương tính giả / âm tính giả của tín hiệu bổ trợ (plan §3 — `tests/validate_fp.jsonl`).
    #[test]
    fn foreign_card_corpus() {
        let (mut fp, mut fneg, mut total) = (Vec::new(), Vec::new(), 0usize);
        for line in include_str!("../tests/validate_fp.jsonl").lines().filter(|l| !l.trim().is_empty() && !l.starts_with("//")) {
            let c: Case = serde_json::from_str(line).unwrap();
            let ids: Vec<&str> = c.drawn.iter().map(String::as_str).collect();
            let got = foreign_cards(&c.text, &draws(&ids), deck());
            total += 1;
            fp.extend(got.iter().filter(|g| !c.foreign.contains(g)).map(|g| format!("{g} ← {:?}", c.text)));
            fneg.extend(c.foreign.iter().filter(|w| !got.contains(w)).map(|w| format!("{w} ← {:?}", c.text)));
        }
        assert!(total >= 10);
        assert!(fp.is_empty(), "dương tính giả:\n{}", fp.join("\n"));
        assert!(fneg.is_empty(), "âm tính giả:\n{}", fneg.join("\n"));
    }
}
