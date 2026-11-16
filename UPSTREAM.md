# Đồng bộ với PlanCouncil (upstream)

TarotCouncil là fork của PlanCouncil (`https://github.com/ThanhBackend2002/plancouncil`). Các module dưới đây
được giữ gần nguyên bản để cherry-pick bản sửa lỗi từ upstream bằng tay (plan §5 — Trade-offs: fork thay vì shared crate).

| Upstream commit đã đồng bộ gần nhất | Ngày fork |
|---|---|
| `f37d977` — feat: Enhance ContextToolsTab with unlimited tool call option and update validation logic | 2026-11-17 |

## Module theo dõi

| File | Khác biệt so với upstream |
|---|---|
| `src-tauri/src/llm_client.rs` | Chỉ đổi User-Agent (`TarotCouncil/<version>`). Không cắt API chưa dùng (tool calling, stream) để giữ diff nhỏ. |
| `src-tauri/src/budget.rs` | Nguyên bản. |
| `src-tauri/src/health.rs` | Nguyên bản (role = thầy bói). |
| `src-tauri/src/normalize.rs` | Nguyên bản. |
| `src-tauri/src/secrets.rs` | Bỏ target web search, service keyring `com.tarotcouncil.app`, `set_secret` từ chối chế độ `file`. |
| `src-tauri/src/test_support.rs` | Nguyên bản. |
| `src/lib/pixel.ts`, `src/components/PixelCharacter.tsx` | `AgentStatus` khai báo tại chỗ (store session đã bỏ). |

## Cách đồng bộ

```sh
git fetch upstream
git log --oneline f37d977..upstream/main -- src-tauri/src/llm_client.rs src-tauri/src/budget.rs src/lib/pixel.ts
git cherry-pick -n <commit>   # rồi sửa xung đột, chạy `npm run verify`, cập nhật bảng trên
```

Remote `upstream` đã bị chặn push (`git remote set-url --push upstream DISABLED`).
