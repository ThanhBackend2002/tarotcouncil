# TarotCouncil

Ứng dụng desktop bốc bài và luận giải tarot: bạn nhập câu hỏi, chọn kiểu trải, tự bốc lá từ bộ bài đã tráo,
rồi một "hội đồng thầy bói" AI luận giải và Trưởng thầy tổng hợp. Chỉ mang tính giải trí.

Kế hoạch: [plancouncil-a1654060-v3.md](plancouncil-a1654060-v3.md) · Đồng bộ upstream: [UPSTREAM.md](UPSTREAM.md)

## Trạng thái

| Mốc | Nội dung | Trạng thái |
|---|---|---|
| M0 | Fork surgery từ PlanCouncil, cách ly dữ liệu + keyring, cấu hình provider/thầy bói | ✅ |
| M1 | `deck.rs` (78 lá, nghĩa vi 60–100 từ), `draw.rs` ChaCha20 + golden test, `safety.rs` + corpus, `SupportScreen`, hậu kiểm, `PrivacyNotice` | ✅ |
| M2 | Trải 3/5 lá, 1–4 thầy bói + Trưởng thầy, `DeckFan`, `ReadingRoom`, Nhật ký + FTS5, khôi phục sau crash, `verify_reading` | ✅ |
| M3 | Celtic Cross, hiệu ứng tráo / lật | ✅ |

**Khác plan (đã thống nhất):** dùng ảnh Rider–Waite–Smith 1909 (public domain) thay cho lá pixel-art tự sinh —
nên không có bước "22 sprite Major"; thêm 2 kiểu trải 5 lá (Tình cảm, Công việc).

## Luồng một lượt đọc

1. `create_reading` — kiểm tra câu hỏi (3–500 ký tự), lọc an toàn (trúng nhóm tự hại → `SupportScreen`,
   không đọc), seed 32 byte → Fisher–Yates trên ChaCha20 → `deck_order` + `reversed_flags`.
2. `draw_card(fan_index)` — chỉ số cố định trong quạt; ghi atomic rồi mới trả lá (lỗi ghi → không lật).
3. `interpret_reading` — pha 1 các thầy bói song song, output giữ trong Rust tới khi xong cả; cổng an toàn
   (`crisis` / từ khoá) → bỏ mọi output; hậu kiểm (schema + cặp vị trí/lá) có retry 1 lần; ghi rồi mới emit.
   Pha 2 Trưởng thầy tổng hợp, disclaimer bắt buộc theo nhóm y tế / tài chính / pháp lý.
4. Event `reading-event` `{reading_id, attempt_id, seq, ts, payload}` — client lọc attempt/seq, hở seq thì đệm
   và đồng bộ lại bằng snapshot `get_reading`.

## Kiến trúc

- **Rust (`src-tauri/src`)** — mọi lời gọi AI chạy ở đây; UI không bao giờ chạm API key.
  `deck` · `spreads` · `draw` · `table` (bàn bốc bài) · `safety` · `validate` · `prompts` · `reading_orchestrator` ·
  `events` (`ReadingEmitter`) · `journal` (JSON + FTS5) · `support` · `config` / `roster` / `secrets` / `storage` ·
  `llm_client` / `budget` / `cost` / `health` (từ PlanCouncil).
- **Dữ liệu:** `src-tauri/assets/deck_vi.json` (78 lá, nghĩa tiếng Việt tự viết), `assets/support.json`
  (đường dây hỗ trợ — chỉ hiển thị mục có `verified_at` trong 12 tháng; hiện để trống, cần người xác minh).
  Corpus test: `src-tauri/tests/safety_vi.jsonl`, `src-tauri/tests/validate_fp.jsonl`.
- **React (`src`)** — React 19, zustand, motion, Tailwind v4. `components/tarot/*` (AskPanel, DeckFan, TarotCard,
  ReadingRoom, ReadingResult, JournalView, PrivacyNotice, SupportScreen), `lib/readingSync.ts` (đồng bộ event).

Dữ liệu lưu tại `%APPDATA%\tarotcouncil\` (Windows), `~/Library/Application Support/tarotcouncil/` (macOS),
`~/.config/tarotcouncil/` (Linux). Keyring service: `com.tarotcouncil.app`. Hoàn toàn tách khỏi PlanCouncil.

## Phát triển

```sh
npm ci
npm run tauri dev      # chạy app
npm run verify         # cargo test + clippy -D warnings + vitest + tsc
npm run build:win      # hoặc build:mac / build:linux
```

> Windows: nếu `vitest` báo `Cannot read properties of undefined (reading 'config')`, terminal đang ở đường dẫn
> ổ đĩa chữ thường (`c:\…`). Mở lại terminal ở `C:\…` rồi chạy lại.

## Việc cần người làm

- Review nội dung 156 nghĩa trong `deck_vi.json` theo checklist (nhất quán, không chép sách có bản quyền) — plan §4.
- Điền `assets/support.json` bằng các đường dây hỗ trợ đã gọi kiểm tra trực tiếp (kèm `verified_at`).
- Thử nghiệm quạt 78 lá với người dùng thật (chuột + bàn phím) — plan §2 DeckFan.
