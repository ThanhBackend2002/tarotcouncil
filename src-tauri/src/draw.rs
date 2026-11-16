//! Tráo bài tái lập được từ seed (plan §2 — `draw.rs`, `shuffle_algo_version = 1`).
//!
//! Thuật toán v1 (ĐỪNG đổi — đổi thì tăng version và giữ nhánh cũ cho `verify_reading`):
//! 1. `ChaCha20Rng::from_seed(seed 32 byte)`.
//! 2. Fisher–Yates trên `[0, 1, …, 77]`: `for i in (1..78).rev() { j = uniform(i + 1); swap(i, j) }`,
//!    `uniform(n)` = rejection sampling trên `next_u32` (không lệch modulo).
//! 3. `reversed_flags[k]` (áp cho lá `deck_order[k]`): 78 bit tiếp theo của CÙNG RNG — lấy 3 lần
//!    `next_u32`, bit thứ k = bit `k % 32` (từ LSB) của từ thứ `k / 32`. `reversed_enabled = false`
//!    → toàn bộ false (deck_order không đổi vì cờ sinh SAU khi tráo).

use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use crate::deck::DECK_SIZE;

pub const SHUFFLE_ALGO_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shuffle {
    /// `deck_order[fan_index]` = chỉ số canonical của lá ở vị trí đó trong quạt.
    pub deck_order: Vec<u8>,
    pub reversed_flags: Vec<bool>,
}

/// Seed 32 byte từ nguồn ngẫu nhiên của hệ điều hành.
pub fn new_seed() -> Result<[u8; 32], String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("không lấy được seed ngẫu nhiên: {e}"))?;
    Ok(seed)
}

/// Số nguyên đều trong `[0, n)` (n ≥ 1) — rejection sampling, không lệch modulo.
fn uniform(rng: &mut ChaCha20Rng, n: u32) -> u32 {
    debug_assert!(n > 0);
    let zone = u32::MAX - (u32::MAX % n); // [0, zone) chia hết cho n
    loop {
        let v = rng.next_u32();
        if v < zone {
            return v % n;
        }
    }
}

pub fn shuffle(seed: &[u8; 32], algo_version: u32, reversed_enabled: bool) -> Result<Shuffle, String> {
    if algo_version != 1 {
        return Err(format!("shuffle_algo_version {algo_version} không được hỗ trợ"));
    }
    let mut rng = ChaCha20Rng::from_seed(*seed);
    let mut order: Vec<u8> = (0..DECK_SIZE as u8).collect();
    for i in (1..DECK_SIZE).rev() {
        let j = uniform(&mut rng, (i + 1) as u32) as usize;
        order.swap(i, j);
    }
    let words = [rng.next_u32(), rng.next_u32(), rng.next_u32()];
    let reversed_flags = (0..DECK_SIZE)
        .map(|k| reversed_enabled && (words[k / 32] >> (k % 32)) & 1 == 1)
        .collect();
    Ok(Shuffle { deck_order: order, reversed_flags })
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn seed_from_hex(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 || !s.is_ascii() {
        return Err("seed_hex phải dài 64 ký tự hex".into());
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        out[i] = u8::from_str_radix(hex, 16).map_err(|_| format!("seed_hex có ký tự không hợp lệ: {hex}"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(b: u8) -> [u8; 32] {
        std::array::from_fn(|i| b.wrapping_add(i as u8))
    }

    /// Golden test (plan §2): seed cố định → deck_order + reversed_flags cố định.
    /// Fail = thuật toán v1 đã bị đổi → mọi lượt đọc cũ không còn kiểm chứng được.
    #[test]
    fn golden_v1() {
        let s = shuffle(&seed(0), 1, true).unwrap();
        assert_eq!(&s.deck_order[..12], GOLDEN_ORDER_PREFIX, "order: {:?}", s.deck_order);
        let flags: String = s.reversed_flags.iter().map(|&b| if b { '1' } else { '0' }).collect();
        assert_eq!(flags, GOLDEN_FLAGS);
        assert_eq!(to_hex(&seed(0)), "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
    }

    const GOLDEN_ORDER_PREFIX: &[u8] = &[58, 18, 43, 76, 68, 72, 44, 33, 24, 73, 52, 69];
    const GOLDEN_FLAGS: &str = "101111001111001001101110001011111001111101111001000001001000100110100111000001";

    #[test]
    fn shuffle_is_a_permutation_and_deterministic() {
        for b in 0..20 {
            let a = shuffle(&seed(b), 1, true).unwrap();
            assert_eq!(a, shuffle(&seed(b), 1, true).unwrap());
            let mut sorted = a.deck_order.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..78).collect::<Vec<u8>>());
            assert_eq!(a.reversed_flags.len(), 78);
        }
        assert_ne!(shuffle(&seed(1), 1, true).unwrap().deck_order, shuffle(&seed(2), 1, true).unwrap().deck_order);
    }

    #[test]
    fn reversed_disabled_keeps_order_and_clears_flags() {
        let on = shuffle(&seed(7), 1, true).unwrap();
        let off = shuffle(&seed(7), 1, false).unwrap();
        assert_eq!(on.deck_order, off.deck_order);
        assert!(off.reversed_flags.iter().all(|f| !f));
        assert!(on.reversed_flags.iter().any(|f| *f));
    }

    #[test]
    fn roughly_uniform_first_card_and_half_reversed() {
        let mut first = [0u32; 78];
        let mut rev = 0u32;
        let n = 7800u32;
        for i in 0..n {
            let mut sd = [0u8; 32];
            sd[..4].copy_from_slice(&i.to_le_bytes());
            let s = shuffle(&sd, 1, true).unwrap();
            first[s.deck_order[0] as usize] += 1;
            rev += s.reversed_flags.iter().filter(|f| **f).count() as u32;
        }
        // kỳ vọng 100 / lá; chặn lệch thô (không phải kiểm định thống kê chặt).
        assert!(first.iter().all(|&c| (40..=180).contains(&c)), "{first:?}");
        let ratio = rev as f64 / (n as f64 * 78.0);
        assert!((0.48..0.52).contains(&ratio), "{ratio}");
    }

    #[test]
    fn hex_roundtrip_and_errors() {
        let sd = seed(200);
        assert_eq!(seed_from_hex(&to_hex(&sd)).unwrap(), sd);
        assert!(seed_from_hex("abc").is_err());
        assert!(seed_from_hex(&"zz".repeat(32)).is_err());
        assert!(shuffle(&sd, 2, true).is_err());
        assert_ne!(new_seed().unwrap(), new_seed().unwrap());
    }

    #[test]
    #[ignore = "in giá trị golden khi cố ý tạo version mới"]
    fn print_golden() {
        let s = shuffle(&seed(0), 1, true).unwrap();
        println!("{:?}", &s.deck_order[..12]);
        println!("{}", s.reversed_flags.iter().map(|&b| if b { '1' } else { '0' }).collect::<String>());
    }
}
