import { describe, expect, it } from "vitest";
import type { Reading, ReadingEnvelope, ReadingPayload } from "./contracts";
import { applyEvent, classify, fromReading, replay } from "./readingSync";

const reading = (patch: Partial<Reading> = {}): Reading => ({
  schema_version: 1,
  id: "r1",
  created_at: "2026-11-17T00:00:00Z",
  updated_at: "2026-11-17T00:00:00Z",
  app_version: "0.1.0",
  deck_version: 1,
  question: "Công việc?",
  spread_id: "three",
  reader_ids: ["classic", "jungian"],
  chief_reader_id: "chief",
  cast: [],
  safety_flags: [],
  shuffle_algo_version: 1,
  reversed_enabled: true,
  picks: [0, 1, 2],
  draws: [],
  attempt_id: 1,
  outputs: [],
  synthesis: null,
  usage: { prompt_tokens: 0, completion_tokens: 0, cached_tokens: 0, calls: 0, estimated_calls: 0 },
  status: "interpreting",
  error: null,
  ...patch,
});

const env = (seq: number, payload: ReadingPayload, attempt = 1, id = "r1"): ReadingEnvelope => ({
  reading_id: id,
  attempt_id: attempt,
  seq,
  ts: "",
  payload,
});

const output = { reader_id: "classic", output: { positions: [], overall: "ok", tone: "neutral" as const, crisis: false }, failed: null, warnings: [] };

describe("classify", () => {
  const s = fromReading(reading(), 3);
  it("bỏ event khác lượt đọc, attempt cũ, seq cũ", () => {
    expect(classify(s, env(4, { type: "reading_done" }, 1, "other"))).toBe("drop");
    expect(classify({ ...s, attemptId: 2 }, env(4, { type: "reading_done" }, 1))).toBe("drop");
    expect(classify(s, env(3, { type: "reading_done" }))).toBe("drop");
    expect(classify(s, env(2, { type: "reading_done" }))).toBe("drop");
  });
  it("nhận seq kế tiếp, báo hở seq", () => {
    expect(classify(s, env(4, { type: "reading_done" }))).toBe("apply");
    expect(classify(s, env(6, { type: "reading_done" }))).toBe("gap");
    expect(classify({ ...s, reading: null }, env(4, { type: "reading_done" }))).toBe("drop");
  });
});

describe("applyEvent", () => {
  it("chạy hết luồng 2 pha", () => {
    let s = fromReading(reading(), 0);
    const seq = [
      { type: "reader_started", reader_id: "classic" },
      { type: "reader_progress", reader_id: "classic", chars: 120 },
      { type: "reader_done", reader_id: "classic", output },
      { type: "reader_failed", reader_id: "jungian", reason: "timeout" },
      { type: "synthesis_started" },
      { type: "synthesis_done", synthesis: { summary: "s", agreements: [], divergences: [], advice: ["a"], disclaimer: "d", crisis: false } },
      { type: "usage", tokens: 900, cost_usd: 0.01 },
      { type: "reading_done" },
    ] satisfies ReadingPayload[];
    seq.forEach((p, i) => (s = applyEvent(s, env(i + 1, p))));
    expect(s.phase).toBe("done");
    expect(s.readers.classic!.status).toBe("done");
    expect(s.readers.jungian).toMatchObject({ status: "failed", reason: "timeout" });
    expect(s.synthesis?.summary).toBe("s");
    expect(s.usage).toEqual({ tokens: 900, cost_usd: 0.01 });
    expect(s.lastSeq).toBe(8);
  });

  it("support_required xoá mọi lời giải", () => {
    let s = applyEvent(fromReading(reading()), env(1, { type: "reader_done", reader_id: "classic", output }));
    s = applyEvent(s, env(2, { type: "support_required" }));
    expect(s.phase).toBe("support");
    expect(s.readers).toEqual({});
  });

  it("attempt mới reset phần luận giải", () => {
    let s = fromReading(reading({ status: "failed", error: "x", attempt_id: 1 }), 5);
    expect(s.phase).toBe("error");
    s = applyEvent(s, env(6, { type: "reader_started", reader_id: "classic" }, 2));
    expect(s.attemptId).toBe(2);
    expect(s.phase).toBe("interpreting");
    expect(s.error).toBeNull();
    expect(s.readers.jungian!.status).toBe("waiting");
  });
});

describe("replay", () => {
  it("snapshot + chỉ áp event đệm có seq > last_seq", () => {
    const snap = { reading: reading({ outputs: [output] }), attempt_id: 1, last_seq: 4, live: true };
    const s = replay(snap, [
      env(3, { type: "reader_failed", reader_id: "classic", reason: "cũ" }),
      env(6, { type: "reading_done" }),
      env(5, { type: "synthesis_started" }),
      env(9, { type: "reading_done" }, 1, "other"),
    ]);
    expect(s.readers.classic!.status).toBe("done"); // event seq 3 bị bỏ
    expect(s.phase).toBe("done");
    expect(s.lastSeq).toBe(6);
  });
});
