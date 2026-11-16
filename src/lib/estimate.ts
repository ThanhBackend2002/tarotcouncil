// Ước lượng TRƯỚC khi luận giải (plan §4 — Ngân sách): bảng tĩnh theo số lá × số thầy bói × đơn giá.
// Chỉ là ước lượng để người dùng cân nhắc — chi phí thật có thể vượt khi retry / chạy song song;
// UI hiển thị `Usage` thực tế sau khi chạy.

import type { AppConfig, RoleId } from "./contracts";

/** 1 thầy bói, trải 3 lá ≈ 1,5k input + 600 output (plan §4). */
export const readerTokens = (cards: number) => ({ input: 900 + 250 * cards, output: 200 + 130 * cards });
export const synthesisTokens = (cards: number, readers: number) => ({ input: 900 + 120 * cards + readers * readerTokens(cards).output, output: 500 });

export interface Estimate {
  tokens: number;
  /** null nếu có model chưa có trong bảng giá. */
  costUsd: number | null;
}

export function estimateReading(cfg: AppConfig, cards: number, readerIds: RoleId[], chiefId: RoleId): Estimate {
  const parts: { model: string; input: number; output: number }[] = [
    ...readerIds.map((id) => ({ model: cfg.roles.find((r) => r.id === id)?.model ?? "", ...readerTokens(cards) })),
    { model: cfg.roles.find((r) => r.id === chiefId)?.model ?? "", ...synthesisTokens(cards, readerIds.length) },
  ];
  let cost: number | null = 0;
  for (const p of parts) {
    const price = cfg.pricing[p.model];
    cost = price && cost != null ? cost + (p.input * price.input_per_m + p.output * price.output_per_m) / 1_000_000 : null;
  }
  return { tokens: parts.reduce((a, p) => a + p.input + p.output, 0), costUsd: cost };
}
