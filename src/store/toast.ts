import { create } from "zustand";
import type { CommandError } from "../lib/contracts";
import { ERROR_TITLE } from "../lib/agents";

export type ToastTone = "info" | "success" | "warning" | "error";

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  message?: string;
}

interface ToastStore {
  toasts: Toast[];
  push: (t: Omit<Toast, "id">, ttlMs?: number) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;

export const useToast = create<ToastStore>((set, get) => ({
  toasts: [],
  push: (t, ttlMs = 6000) => {
    const id = nextId++;
    // Giữ tối đa 5 toast để không che UI.
    set((s) => ({ toasts: [...s.toasts.slice(-4), { ...t, id }] }));
    if (ttlMs > 0) setTimeout(() => get().dismiss(id), ttlMs);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

export function toastError(e: CommandError | unknown, fallbackTitle?: string) {
  const err =
    e && typeof e === "object" && "kind" in e && "message" in e
      ? (e as CommandError)
      : { kind: "internal" as const, message: e instanceof Error ? e.message : String(e) };
  useToast.getState().push(
    {
      tone: err.kind === "busy" || err.kind === "invalid" ? "warning" : "error",
      title: fallbackTitle ?? ERROR_TITLE[err.kind] ?? "Lỗi",
      message: err.message,
    },
    9000,
  );
}

export const toast = (tone: ToastTone, title: string, message?: string) =>
  useToast.getState().push({ tone, title, message });
