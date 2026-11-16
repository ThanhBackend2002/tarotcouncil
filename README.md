# TarotCouncil

Ứng dụng desktop bốc bài và luận giải tarot: bạn nhập câu hỏi, chọn kiểu trải, tự bốc lá từ bộ bài đã tráo,
rồi một "hội đồng thầy bói" AI luận giải. Chỉ mang tính giải trí.

Kế hoạch: [plancouncil-a1654060-v3.md](plancouncil-a1654060-v3.md) · Đồng bộ upstream: [UPSTREAM.md](UPSTREAM.md)

## Trạng thái

| Mốc | Nội dung | Trạng thái |
|---|---|---|
| M0 | Fork surgery từ PlanCouncil, cách ly dữ liệu + keyring, cấu hình provider/thầy bói, `interpret_reading` stub | ✅ |
| M1 | `deck.rs`, `draw.rs` (ChaCha20 + golden test), trải 1 lá, 1 thầy bói, `safety.rs`, `SupportScreen`, hậu kiểm, quyền riêng tư | ⏳ |
| M2 | Trải 3/5 lá, 2–4 thầy bói + tổng hợp, `DeckFan`, `ReadingRoom`, Journal + FTS, khôi phục sau crash | ⏳ |
| M3 | Celtic Cross, sprite Major, hiệu ứng tráo/lật hoàn chỉnh | ⏳ |

## Kiến trúc

- **Rust (`src-tauri/src`)** — mọi lời gọi AI chạy ở đây; UI không bao giờ chạm API key.
  `config.rs` (provider, thầy bói, tuỳ chọn tarot), `roster.rs` (4 thầy bói + Trưởng thầy), `secrets.rs`
  (keyring hoặc RAM), `storage.rs` (ghi atomic), `llm_client.rs` / `budget.rs` / `cost.rs` / `health.rs` (từ PlanCouncil).
- **React (`src`)** — React 19, zustand, motion, Tailwind v4. `lib/contracts.ts` mirror kiểu của Rust, gồm event contract `reading-event`.

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
