import { create } from "zustand";
import type { Reading, ReadingEnvelope, RoleId } from "../lib/contracts";
import { isCommandError, readingIpc } from "../lib/ipc";
import { EMPTY, applyEvent, classify, fromReading, replay, type LiveState } from "../lib/readingSync";
import { toast, toastError } from "./toast";
import { useUi } from "./ui";

/** Thời lượng hiệu ứng tráo trước khi mở quạt bài. */
export const SHUFFLE_MS = 1100;

interface ReadingStore extends LiveState {
  /** Đang đồng bộ lại bằng snapshot → event mới được đệm. */
  buffering: boolean;
  buffer: ReadingEnvelope[];
  busy: boolean;
  /** Hỏi đồng ý quyền riêng tư trước lần luận giải đầu tiên. */
  privacyPrompt: boolean;

  ask: () => void;
  reset: () => void;
  start: (question: string, spreadId: string, readerIds: RoleId[], animate: boolean) => Promise<void>;
  draw: (fanIndex: number) => Promise<boolean>;
  autoDraw: () => Promise<void>;
  reshuffle: () => Promise<void>;
  interpret: () => Promise<void>;
  setPrivacyPrompt: (v: boolean) => void;
  cancel: () => Promise<void>;
  open: (id: string) => Promise<void>;
  ingest: (env: ReadingEnvelope) => void;
  resync: () => Promise<void>;
}

const base = { ...EMPTY, buffering: false, buffer: [] as ReadingEnvelope[], busy: false, privacyPrompt: false };

export const useReading = create<ReadingStore>((set, get) => {
  /** Cập nhật bản ghi sau thao tác bốc bài (không đụng tới seq). */
  const adopt = (r: Reading) => set((s) => ({ ...fromReading(r, s.lastSeq), buffering: false, buffer: [] }));

  const guard = async (title: string, f: () => Promise<void>) => {
    if (get().busy) return;
    set({ busy: true });
    try {
      await f();
    } catch (e) {
      toastError(e, title);
    } finally {
      set({ busy: false });
    }
  };

  return {
    ...base,

    ask: () => set({ ...base, phase: "asking" }),
    reset: () => set({ ...base }),

    start: (question, spreadId, readerIds, animate) =>
      guard("Không tạo được lượt đọc", async () => {
        const res = await readingIpc.create(question, spreadId, readerIds);
        if (res.blocked || !res.reading) {
          useUi.getState().openSupport("blocked");
          return;
        }
        const r = res.reading;
        set({ ...fromReading(r, 0), phase: "shuffling", buffering: false, buffer: [] });
        await new Promise((ok) => setTimeout(ok, animate ? SHUFFLE_MS : 0));
        if (get().reading?.id === r.id) set({ phase: "drawing" });
      }),

    draw: async (fanIndex) => {
      const r = get().reading;
      if (!r || get().busy) return false;
      set({ busy: true });
      try {
        const res = await readingIpc.draw(r.id, fanIndex);
        adopt(res.reading);
        return true;
      } catch (e) {
        // `persist_failed` → không lật lá (plan §3).
        toastError(e, "Không bốc được lá");
        return false;
      } finally {
        set({ busy: false });
      }
    },

    autoDraw: () =>
      guard("Không bốc tự động được", async () => {
        const r = get().reading;
        if (r) adopt(await readingIpc.autoDraw(r.id));
      }),

    reshuffle: () =>
      guard("Không tráo lại được", async () => {
        const r = get().reading;
        if (!r) return;
        const next = await readingIpc.reshuffle(r.id);
        set({ ...fromReading(next, get().lastSeq), phase: "shuffling" });
        await new Promise((ok) => setTimeout(ok, SHUFFLE_MS));
        if (get().reading?.id === next.id) set({ phase: "drawing" });
      }),

    interpret: () =>
      guard("Không bắt đầu luận giải được", async () => {
        const r = get().reading;
        if (!r) return;
        try {
          const attempt = await readingIpc.interpret(r.id);
          set((s) =>
            s.attemptId >= attempt && s.phase === "interpreting"
              ? {}
              : {
                  phase: "interpreting",
                  attemptId: Math.max(s.attemptId, attempt),
                  error: null,
                  synthesis: null,
                  synthesisRunning: false,
                  readers: Object.fromEntries(r.reader_ids.map((id) => [id, { status: "waiting", chars: 0, result: null, reason: null }])),
                },
          );
        } catch (e) {
          if (isCommandError(e) && e.kind === "privacy_required") {
            set({ privacyPrompt: true });
            return;
          }
          throw e;
        }
      }),

    setPrivacyPrompt: (privacyPrompt) => set({ privacyPrompt }),

    cancel: async () => {
      const r = get().reading;
      if (!r) return;
      try {
        if (!(await readingIpc.cancel(r.id))) toast("info", "Lượt đọc không còn chạy");
      } catch (e) {
        toastError(e, "Không dừng được");
      }
    },

    open: async (id) => {
      set({ ...base, phase: "interpreting", buffering: true });
      try {
        const snap = await readingIpc.get(id);
        set((s) => ({ ...replay(snap, s.buffer), buffering: false, buffer: [] }));
      } catch (e) {
        set({ ...base });
        toastError(e, "Không mở được lượt đọc");
      }
    },

    ingest: (env) => {
      const s = get();
      if (s.buffering) {
        if (!s.reading || env.reading_id === s.reading.id) set({ buffer: [...s.buffer, env] });
        return;
      }
      const v = classify(s, env);
      if (v === "drop") return;
      if (v === "gap") {
        set({ buffering: true, buffer: [env] });
        void get().resync();
        return;
      }
      const next = applyEvent(s, env);
      set(next);
      if (env.payload.type === "support_required") useUi.getState().openSupport("crisis");
    },

    resync: async () => {
      const r = get().reading;
      if (!r) return;
      try {
        const snap = await readingIpc.get(r.id);
        set((s) => {
          const next = replay(snap, s.buffer);
          return { ...next, buffering: false, buffer: [] };
        });
      } catch (e) {
        set({ buffering: false, buffer: [] });
        toastError(e, "Không đồng bộ lại được lượt đọc");
      }
    },
  };
});
