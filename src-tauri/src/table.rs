//! Bàn bốc bài — logic thuần (không I/O) cho `create_reading`, `draw_card`, `auto_draw`,
//! `reshuffle_reading`, `verify_reading` và điều kiện bắt đầu `interpret_reading` (plan §3).

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::config::{AppConfig, MAX_READERS};
use crate::deck::{deck, DECK_SIZE};
use crate::draw::{self, SHUFFLE_ALGO_VERSION};
use crate::reading::{CastMember, Draw, Reading, ReadingStatus};
use crate::safety;
use crate::spreads;

pub const QUESTION_CHARS: std::ops::RangeInclusive<usize> = 3..=500;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("{0}")]
    Invalid(String),
    /// Trạng thái không cho phép thao tác (đang luận giải, đã đủ lá…).
    #[error("{0}")]
    Busy(String),
}

#[derive(Debug)]
pub enum Created {
    /// Câu hỏi trúng nhóm tự hại → mở SupportScreen, KHÔNG tạo lượt đọc.
    Blocked,
    Reading(Box<Reading>),
}

pub fn create(cfg: &AppConfig, question: &str, spread_id: &str, reader_ids: &[String], seed: [u8; 32]) -> Result<Created, TableError> {
    let question = question.trim();
    let n = question.chars().count();
    if !QUESTION_CHARS.contains(&n) {
        return Err(TableError::Invalid(format!("Câu hỏi cần {}–{} ký tự", QUESTION_CHARS.start(), QUESTION_CHARS.end())));
    }
    let spread = spreads::get(spread_id).ok_or_else(|| TableError::Invalid(format!("Không có kiểu trải '{spread_id}'")))?;
    let mut ids: Vec<String> = Vec::new();
    for id in reader_ids {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    if ids.is_empty() || ids.len() > MAX_READERS {
        return Err(TableError::Invalid(format!("Chọn 1–{MAX_READERS} thầy bói")));
    }
    if let Some(bad) = ids.iter().find(|id| cfg.role(id).is_none()) {
        return Err(TableError::Invalid(format!("Thầy bói '{bad}' không tồn tại")));
    }
    let scan = safety::scan(question);
    if scan.self_harm() {
        return Ok(Created::Blocked);
    }
    let shuffled = draw::shuffle(&seed, SHUFFLE_ALGO_VERSION, cfg.tarot.reversed_enabled).map_err(TableError::Invalid)?;
    let chief = cfg.tarot.chief_reader_id.clone();
    let cast = ids.iter().chain([&chief]).filter_map(|id| cfg.role(id)).map(CastMember::of).collect();
    let now = Utc::now();
    let _ = spread;
    Ok(Created::Reading(Box::new(Reading {
        id: Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        deck_version: deck().deck_version,
        question: question.to_owned(),
        spread_id: spread_id.to_owned(),
        reader_ids: ids,
        chief_reader_id: chief,
        cast,
        safety_flags: scan.sensitive(),
        seed_hex: draw::to_hex(&seed),
        shuffle_algo_version: SHUFFLE_ALGO_VERSION,
        reversed_enabled: cfg.tarot.reversed_enabled,
        deck_order: shuffled.deck_order,
        reversed_flags: shuffled.reversed_flags,
        status: ReadingStatus::Drawing,
        ..Reading::default()
    })))
}

fn spread_len(r: &Reading) -> Result<usize, TableError> {
    spreads::get(&r.spread_id).map(|s| s.len()).ok_or_else(|| TableError::Invalid("Kiểu trải không còn tồn tại".into()))
}

/// Bốc lá ở `fan_index` (chỉ số CỐ ĐỊNH trong `deck_order`). Thêm snapshot đầy đủ vào `draws`.
pub fn draw_card(r: &mut Reading, fan_index: u8) -> Result<Draw, TableError> {
    if r.status != ReadingStatus::Drawing {
        return Err(TableError::Busy("Lượt đọc không còn ở bước bốc bài".into()));
    }
    let need = spread_len(r)?;
    if r.draws.len() >= need {
        return Err(TableError::Busy("Đã bốc đủ lá cho kiểu trải này".into()));
    }
    if fan_index as usize >= DECK_SIZE || r.deck_order.len() != DECK_SIZE {
        return Err(TableError::Invalid(format!("fan_index {fan_index} không hợp lệ")));
    }
    if r.picks.contains(&fan_index) {
        return Err(TableError::Busy("Lá này đã được bốc".into()));
    }
    let spread = spreads::get(&r.spread_id).expect("đã kiểm tra");
    let pos = &spread.positions[r.draws.len()];
    let card = deck().card(r.deck_order[fan_index as usize]);
    let d = Draw {
        position: pos.index,
        position_label_vi: pos.label_vi.to_owned(),
        fan_index,
        card_id: card.id.clone(),
        name_vi: card.name_vi.clone(),
        name_en: card.name_en.clone(),
        reversed: r.reversed_flags[fan_index as usize],
        art_key: card.art_key.clone(),
    };
    r.picks.push(fan_index);
    r.draws.push(d.clone());
    if r.draws.len() == need {
        r.status = ReadingStatus::Drawn;
    }
    r.updated_at = Utc::now();
    Ok(d)
}

/// "Bốc tự động": rút các lá trên cùng còn lại của quạt (fan_index nhỏ nhất chưa bốc).
pub fn auto_draw(r: &mut Reading) -> Result<Vec<Draw>, TableError> {
    let need = spread_len(r)?;
    let mut out = Vec::new();
    for i in 0..DECK_SIZE as u8 {
        if r.draws.len() >= need {
            break;
        }
        if !r.picks.contains(&i) {
            out.push(draw_card(r, i)?);
        }
    }
    Ok(out)
}

/// Tráo lại (chỉ khi chưa bốc lá nào): seed mới → thứ tự mới.
pub fn reshuffle(r: &mut Reading, seed: [u8; 32]) -> Result<(), TableError> {
    if r.status != ReadingStatus::Drawing || !r.picks.is_empty() {
        return Err(TableError::Busy("Chỉ tráo lại được khi chưa bốc lá nào".into()));
    }
    let s = draw::shuffle(&seed, SHUFFLE_ALGO_VERSION, r.reversed_enabled).map_err(TableError::Invalid)?;
    r.seed_hex = draw::to_hex(&seed);
    r.shuffle_algo_version = SHUFFLE_ALGO_VERSION;
    r.deck_order = s.deck_order;
    r.reversed_flags = s.reversed_flags;
    r.updated_at = Utc::now();
    Ok(())
}

/// Bắt đầu 1 attempt luận giải. Chỉ khi `status ∈ {drawn, failed, cancelled}`; tăng `attempt_id`.
pub fn begin_interpret(r: &mut Reading) -> Result<u32, TableError> {
    match r.status {
        ReadingStatus::Drawn | ReadingStatus::Failed | ReadingStatus::Cancelled => {}
        ReadingStatus::Drawing => return Err(TableError::Busy("Hãy bốc đủ lá trước".into())),
        _ => return Err(TableError::Busy("Lượt đọc đang bận hoặc đã hoàn tất".into())),
    }
    r.attempt_id += 1;
    r.status = ReadingStatus::Interpreting;
    r.outputs.clear();
    r.synthesis = None;
    r.error = None;
    r.updated_at = Utc::now();
    Ok(r.attempt_id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verification {
    pub verified: bool,
    /// Nhãn hiển thị; "không kiểm chứng được" khi lệch.
    pub label: String,
    pub problems: Vec<String>,
}

/// Tái tạo `deck_order` + `reversed_flags` từ seed theo `shuffle_algo_version`, kiểm `draws` khớp
/// `picks`. KHÔNG sửa dữ liệu (plan §3 — Khôi phục sau crash).
pub fn verify(r: &Reading) -> Verification {
    let mut problems = Vec::new();
    match draw::seed_from_hex(&r.seed_hex).and_then(|s| draw::shuffle(&s, r.shuffle_algo_version, r.reversed_enabled)) {
        Err(e) => problems.push(e),
        Ok(s) => {
            if s.deck_order != r.deck_order {
                problems.push("deck_order không khớp seed".into());
            }
            if s.reversed_flags != r.reversed_flags {
                problems.push("reversed_flags không khớp seed".into());
            }
            if r.picks.len() != r.draws.len() {
                problems.push("số picks khác số lá đã bốc".into());
            }
            for (i, (pick, d)) in r.picks.iter().zip(&r.draws).enumerate() {
                let Some(&canon) = s.deck_order.get(*pick as usize) else {
                    problems.push(format!("lá {i}: fan_index ngoài phạm vi"));
                    continue;
                };
                let card = deck().card(canon);
                if d.fan_index != *pick || d.card_id != card.id || d.reversed != s.reversed_flags[*pick as usize] || d.position as usize != i {
                    problems.push(format!("lá {i} ({}) không khớp lượt tráo", d.name_vi));
                }
            }
        }
    }
    let verified = problems.is_empty();
    Verification {
        verified,
        label: if verified { "Đã kiểm chứng: các lá khớp seed".into() } else { "Không kiểm chứng được".into() },
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roster::{CLASSIC_READER, JUNG_READER};

    fn cfg() -> AppConfig {
        AppConfig::default()
    }

    fn new(spread: &str) -> Reading {
        match create(&cfg(), "Công việc sắp tới của tôi?", spread, &[CLASSIC_READER.into()], [7; 32]).unwrap() {
            Created::Reading(r) => *r,
            Created::Blocked => panic!("không được chặn"),
        }
    }

    #[test]
    fn create_validates_and_blocks_self_harm() {
        let c = cfg();
        let e = |q: &str, s: &str, r: &[String]| create(&c, q, s, r, [0; 32]).unwrap_err();
        let one = vec![CLASSIC_READER.to_string()];
        assert!(matches!(e("hi", "one", &one), TableError::Invalid(m) if m.contains("3–500")));
        assert!(matches!(e(&"x".repeat(501), "one", &one), TableError::Invalid(_)));
        assert!(matches!(e("câu hỏi", "nope", &one), TableError::Invalid(_)));
        assert!(matches!(e("câu hỏi", "one", &[]), TableError::Invalid(_)));
        assert!(matches!(e("câu hỏi", "one", &vec!["a".into(); 5]), TableError::Invalid(_)));
        assert!(matches!(e("câu hỏi", "one", &["ghost".into()]), TableError::Invalid(_)));
        assert!(matches!(create(&c, "tôi muốn tự tử", "one", &one, [0; 32]).unwrap(), Created::Blocked));

        let r = match create(&c, "  Có nên đầu tư cổ phiếu?  ", "three", &[CLASSIC_READER.into(), JUNG_READER.into(), CLASSIC_READER.into()], [1; 32]).unwrap() {
            Created::Reading(r) => r,
            _ => unreachable!(),
        };
        assert_eq!(r.question, "Có nên đầu tư cổ phiếu?");
        assert_eq!(r.reader_ids.len(), 2, "bỏ trùng");
        assert_eq!(r.cast.len(), 3, "2 thầy bói + trưởng thầy");
        assert_eq!(r.safety_flags, vec![safety::Category::Financial]);
        assert_eq!((r.deck_order.len(), r.status), (78, ReadingStatus::Drawing));
    }

    #[test]
    fn draw_uses_fixed_fan_index_and_rejects_duplicates_and_overflow() {
        let mut r = new("three");
        let d = draw_card(&mut r, 40).unwrap();
        assert_eq!((d.position, d.fan_index), (0, 40));
        assert_eq!(d.card_id, deck().card(r.deck_order[40]).id);
        assert_eq!(draw_card(&mut r, 40).unwrap_err(), TableError::Busy("Lá này đã được bốc".into()));
        assert!(matches!(draw_card(&mut r, 78), Err(TableError::Invalid(_))));
        // Chỉ số các lá còn lại không đổi.
        let d2 = draw_card(&mut r, 41).unwrap();
        assert_eq!(d2.card_id, deck().card(r.deck_order[41]).id);
        draw_card(&mut r, 0).unwrap();
        assert_eq!(r.status, ReadingStatus::Drawn);
        assert!(matches!(draw_card(&mut r, 1), Err(TableError::Busy(_))));
        assert!(verify(&r).verified);
    }

    #[test]
    fn auto_draw_takes_top_cards_and_reshuffle_only_before_first_pick() {
        let mut r = new("love5");
        let before = r.deck_order.clone();
        reshuffle(&mut r, [9; 32]).unwrap();
        assert_ne!(before, r.deck_order);
        draw_card(&mut r, 1).unwrap();
        assert!(reshuffle(&mut r, [3; 32]).is_err());
        let rest = auto_draw(&mut r).unwrap();
        assert_eq!(rest.iter().map(|d| d.fan_index).collect::<Vec<_>>(), vec![0, 2, 3, 4]);
        assert_eq!(r.status, ReadingStatus::Drawn);
        assert!(auto_draw(&mut r).unwrap().is_empty());
        assert!(verify(&r).verified);
    }

    #[test]
    fn interpret_only_from_allowed_states() {
        let mut r = new("one");
        assert!(matches!(begin_interpret(&mut r), Err(TableError::Busy(_))));
        auto_draw(&mut r).unwrap();
        assert_eq!(begin_interpret(&mut r).unwrap(), 1);
        assert!(matches!(begin_interpret(&mut r), Err(TableError::Busy(_))), "đang interpreting → busy");
        r.status = ReadingStatus::Failed;
        assert_eq!(begin_interpret(&mut r).unwrap(), 2);
        r.status = ReadingStatus::Done;
        assert!(begin_interpret(&mut r).is_err());
    }

    #[test]
    fn verify_flags_tampering_without_changing_data() {
        let mut r = new("three");
        auto_draw(&mut r).unwrap();
        let mut bad = r.clone();
        bad.draws[1].card_id = "major-00".into();
        bad.draws[2].reversed = !bad.draws[2].reversed;
        let v = verify(&bad);
        assert!(!v.verified && v.label == "Không kiểm chứng được" && v.problems.len() == 2, "{v:?}");
        let mut bad2 = r.clone();
        bad2.seed_hex = "00".repeat(32);
        assert!(!verify(&bad2).verified);
        assert_eq!(bad.draws[1].card_id, "major-00", "không sửa dữ liệu");
    }
}
