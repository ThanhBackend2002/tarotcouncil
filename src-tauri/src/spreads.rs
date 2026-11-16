//! Kiểu trải. Toạ độ `x`, `y` chuẩn hoá 0–1 trên mặt bàn (gốc trái-trên), `rotation` theo độ.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Position {
    pub index: u8,
    pub label_vi: &'static str,
    pub meaning_hint: &'static str,
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Spread {
    pub id: &'static str,
    pub name_vi: &'static str,
    pub description_vi: &'static str,
    pub positions: Vec<Position>,
}

impl Spread {
    pub fn len(&self) -> usize {
        self.positions.len()
    }
}

fn p(index: u8, label_vi: &'static str, meaning_hint: &'static str, x: f32, y: f32, rotation: f32) -> Position {
    Position { index, label_vi, meaning_hint, x, y, rotation }
}

pub fn all() -> &'static [Spread] {
    static SPREADS: std::sync::OnceLock<Vec<Spread>> = std::sync::OnceLock::new();
    SPREADS.get_or_init(|| {
        vec![
            Spread {
                id: "one",
                name_vi: "1 lá — Thông điệp",
                description_vi: "Một lá cho câu hỏi hoặc năng lượng của ngày hôm nay.",
                positions: vec![p(0, "Thông điệp", "Năng lượng chính và lời nhắn cho câu hỏi", 0.5, 0.5, 0.0)],
            },
            Spread {
                id: "three",
                name_vi: "3 lá — Quá khứ · Hiện tại · Tương lai",
                description_vi: "Nhìn lại gốc rễ, hiểu hiện trạng và xu hướng sắp tới.",
                positions: vec![
                    p(0, "Quá khứ", "Những gì đã dẫn tới tình huống hiện tại", 0.22, 0.5, 0.0),
                    p(1, "Hiện tại", "Tình hình và năng lượng lúc này", 0.5, 0.5, 0.0),
                    p(2, "Tương lai", "Xu hướng nếu mọi thứ tiếp diễn", 0.78, 0.5, 0.0),
                ],
            },
            Spread {
                id: "love5",
                name_vi: "5 lá — Tình cảm",
                description_vi: "Cảm xúc hai phía, bản chất mối quan hệ, thử thách và hướng đi.",
                positions: vec![
                    p(0, "Bạn", "Cảm xúc và mong muốn của bạn", 0.2, 0.5, 0.0),
                    p(1, "Đối phương", "Cảm xúc và góc nhìn của người ấy", 0.8, 0.5, 0.0),
                    p(2, "Mối quan hệ", "Bản chất kết nối hiện tại", 0.5, 0.5, 0.0),
                    p(3, "Thử thách", "Điều đang cản trở hai người", 0.5, 0.85, 0.0),
                    p(4, "Hướng đi", "Tiềm năng phát triển sắp tới", 0.5, 0.15, 0.0),
                ],
            },
            Spread {
                id: "career5",
                name_vi: "5 lá — Công việc",
                description_vi: "Hiện trạng, trở ngại, điểm mạnh, lời khuyên và kết quả khả dĩ.",
                positions: vec![
                    p(0, "Hiện trạng", "Công việc của bạn lúc này", 0.5, 0.5, 0.0),
                    p(1, "Trở ngại", "Thách thức cần vượt qua", 0.2, 0.5, 0.0),
                    p(2, "Điểm mạnh", "Nguồn lực bạn đang có", 0.8, 0.5, 0.0),
                    p(3, "Lời khuyên", "Hành động nên thực hiện", 0.5, 0.85, 0.0),
                    p(4, "Kết quả", "Kết quả khả dĩ nếu làm theo lời khuyên", 0.5, 0.15, 0.0),
                ],
            },
            Spread {
                id: "celtic",
                name_vi: "10 lá — Celtic Cross",
                description_vi: "Trải bài cổ điển nhìn toàn cảnh một vấn đề phức tạp.",
                positions: vec![
                    p(0, "Hiện tại", "Trọng tâm của vấn đề", 0.36, 0.5, 0.0),
                    p(1, "Thử thách", "Điều đang cắt ngang, cản trở", 0.36, 0.5, 90.0),
                    p(2, "Nền tảng", "Gốc rễ, điều nằm bên dưới", 0.36, 0.86, 0.0),
                    p(3, "Quá khứ gần", "Điều vừa đi qua", 0.12, 0.5, 0.0),
                    p(4, "Mục tiêu", "Điều bạn hướng tới, ý thức", 0.36, 0.14, 0.0),
                    p(5, "Tương lai gần", "Điều sắp đến", 0.6, 0.5, 0.0),
                    p(6, "Bản thân", "Thái độ và vị thế của bạn", 0.86, 0.88, 0.0),
                    p(7, "Môi trường", "Người và hoàn cảnh xung quanh", 0.86, 0.627, 0.0),
                    p(8, "Hy vọng & lo sợ", "Điều bạn mong và điều bạn ngại", 0.86, 0.373, 0.0),
                    p(9, "Kết quả", "Kết quả khả dĩ", 0.86, 0.12, 0.0),
                ],
            },
        ]
    })
}

pub fn get(id: &str) -> Option<&'static Spread> {
    all().iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_are_well_formed() {
        let ids: Vec<_> = all().iter().map(|s| s.id).collect();
        assert_eq!(ids, ["one", "three", "love5", "career5", "celtic"]);
        for s in all() {
            for (i, pos) in s.positions.iter().enumerate() {
                assert_eq!(pos.index as usize, i, "{}", s.id);
                assert!((0.0..=1.0).contains(&pos.x) && (0.0..=1.0).contains(&pos.y), "{} {i}", s.id);
                assert!(!pos.label_vi.is_empty() && !pos.meaning_hint.is_empty());
            }
        }
        assert_eq!(get("celtic").unwrap().len(), 10);
        assert!(get("nope").is_none());
    }
}
