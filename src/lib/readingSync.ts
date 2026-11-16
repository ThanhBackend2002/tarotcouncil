// Đồng bộ event `reading-event` ở client (plan §3) — hàm thuần, không phụ thuộc React/Tauri.
//
// 1. Bỏ event khác `reading_id` đang mở.
// 2. Bỏ event có `attempt_id` nhỏ hơn attempt hiện tại (attempt LỚN hơn = attempt mới → nhận và reset).
// 3. Bỏ event có `seq <= last_seq`.
// 4. Hở seq (seq > last_seq + 1) → caller chuyển sang chế độ đệm, gọi `get_reading` lấy snapshot
//    `{reading, attempt_id, last_seq}`, thay toàn bộ state bằng snapshot rồi áp các event đệm có `seq > last_seq`.

import type { ReaderResult, Reading, ReadingEnvelope, ReadingSnapshot, RoleId, Synthesis } from "./contracts";

export type Phase = "idle" | "asking" | "shuffling" | "drawing" | "interpreting" | "done" | "support" | "error" | "cancelled";

export interface ReaderView {
  status: "waiting" | "thinking" | "done" | "failed";
  chars: number;
  result: ReaderResult | null;
  reason: string | null;
}

export interface LiveError { code: string; message: string; retryable: boolean }

export interface LiveState {
  phase: Phase;
  reading: Reading | null;
  attemptId: number;
  lastSeq: number;
  readers: Record<RoleId, ReaderView>;
  synthesis: Synthesis | null;
  synthesisRunning: boolean;
  usage: { tokens: number; cost_usd: number | null } | null;
  error: LiveError | null;
}

export const EMPTY: LiveState = {
  phase: "idle",
  reading: null,
  attemptId: 0,
  lastSeq: 0,
  readers: {},
  synthesis: null,
  synthesisRunning: false,
  usage: null,
  error: null,
};

export function phaseOf(r: Reading): Phase {
  switch (r.status) {
    case "drawing":
    case "drawn":
      return "drawing";
    case "interpreting":
      return "interpreting";
    case "done":
      return "done";
    case "failed":
      return "error";
    case "cancelled":
      return "cancelled";
    case "support":
      return "support";
  }
}

function readersOf(r: Reading): Record<RoleId, ReaderView> {
  const out: Record<RoleId, ReaderView> = {};
  for (const id of r.reader_ids) {
    const res = r.outputs.find((o) => o.reader_id === id) ?? null;
    out[id] = res
      ? { status: res.output ? "done" : "failed", chars: 0, result: res, reason: res.failed }
      : { status: r.status === "interpreting" ? "thinking" : "waiting", chars: 0, result: null, reason: null };
  }
  return out;
}

/** State dựng từ bản ghi (mở lượt cũ / sau khi tạo / sau khi bốc lá). */
export function fromReading(r: Reading, lastSeq = 0): LiveState {
  return {
    ...EMPTY,
    phase: phaseOf(r),
    reading: r,
    attemptId: r.attempt_id,
    lastSeq,
    readers: readersOf(r),
    synthesis: r.synthesis,
    synthesisRunning: r.status === "interpreting" && r.outputs.length > 0 && !r.synthesis,
    error: r.status === "failed" && r.error ? { code: "failed", message: r.error, retryable: true } : null,
  };
}

export function fromSnapshot(s: ReadingSnapshot): LiveState {
  return fromReading({ ...s.reading, attempt_id: s.attempt_id }, s.last_seq);
}

export type Verdict = "apply" | "drop" | "gap";

export function classify(s: LiveState, env: ReadingEnvelope): Verdict {
  if (!s.reading || env.reading_id !== s.reading.id) return "drop";
  if (env.attempt_id < s.attemptId) return "drop";
  if (env.seq <= s.lastSeq) return "drop";
  if (env.seq > s.lastSeq + 1) return "gap";
  return "apply";
}

/** Áp 1 event đã qua `classify` = "apply" (hoặc event đệm sau snapshot). */
export function applyEvent(s: LiveState, env: ReadingEnvelope): LiveState {
  let next: LiveState = { ...s, lastSeq: Math.max(s.lastSeq, env.seq) };
  if (env.attempt_id > s.attemptId) {
    // Attempt mới: reset phần luận giải, giữ các lá đã bốc.
    next = {
      ...next,
      attemptId: env.attempt_id,
      phase: "interpreting",
      synthesis: null,
      synthesisRunning: false,
      error: null,
      readers: Object.fromEntries((s.reading?.reader_ids ?? []).map((id) => [id, { status: "waiting", chars: 0, result: null, reason: null } as ReaderView])),
    };
  }
  const p = env.payload;
  const setReader = (id: RoleId, patch: Partial<ReaderView>) => ({
    ...next.readers,
    [id]: { ...(next.readers[id] ?? { status: "waiting", chars: 0, result: null, reason: null }), ...patch },
  });
  switch (p.type) {
    case "reader_started":
      return { ...next, phase: "interpreting", readers: setReader(p.reader_id, { status: "thinking", chars: 0 }) };
    case "reader_progress":
      return { ...next, readers: setReader(p.reader_id, { chars: p.chars }) };
    case "reader_done":
      return { ...next, readers: setReader(p.reader_id, { status: "done", result: p.output, reason: null }) };
    case "reader_failed":
      return { ...next, readers: setReader(p.reader_id, { status: "failed", reason: p.reason }) };
    case "synthesis_started":
      return { ...next, synthesisRunning: true };
    case "synthesis_done":
      return { ...next, synthesisRunning: false, synthesis: p.synthesis };
    case "support_required":
      return { ...next, phase: "support", synthesisRunning: false, readers: {}, synthesis: null };
    case "usage":
      return { ...next, usage: { tokens: p.tokens, cost_usd: p.cost_usd } };
    case "reading_done":
      return { ...next, phase: "done", synthesisRunning: false };
    case "cancelled":
      return { ...next, phase: "cancelled", synthesisRunning: false };
    case "error":
      return { ...next, phase: "error", synthesisRunning: false, error: { code: p.code, message: p.message, retryable: p.retryable } };
  }
}

/** Snapshot + các event đệm có `seq > last_seq` (theo thứ tự seq). */
export function replay(snapshot: ReadingSnapshot, buffered: ReadingEnvelope[]): LiveState {
  let s = fromSnapshot(snapshot);
  const pending = buffered.filter((e) => e.reading_id === snapshot.reading.id && e.seq > snapshot.last_seq).sort((a, b) => a.seq - b.seq);
  for (const e of pending) {
    if (e.attempt_id < s.attemptId || e.seq <= s.lastSeq) continue;
    s = applyEvent(s, e);
  }
  return s;
}
