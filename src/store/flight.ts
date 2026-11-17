import { create } from "zustand";

/** Hình học của 1 lá trên màn hình: tâm (px, toạ độ viewport), bề rộng, góc xoay. */
export interface CardBox {
  cx: number;
  cy: number;
  w: number;
  rot: number;
}

/** 1 lá đang bay từ quạt tới ô `position` trên bàn trải. */
export interface Flight {
  id: number;
  fanIndex: number;
  position: number;
  from: CardBox;
}

interface FlightStore {
  flights: Flight[];
  launch: (f: Omit<Flight, "id">) => number;
  land: (id: number) => void;
  clear: () => void;
}

let next = 1;

export const useFlight = create<FlightStore>((set) => ({
  flights: [],
  launch: (f) => {
    const id = next++;
    set((s) => ({ flights: [...s.flights, { ...f, id }] }));
    return id;
  },
  land: (id) => set((s) => ({ flights: s.flights.filter((f) => f.id !== id) })),
  clear: () => set({ flights: [] }),
}));

/** Lá chỉ "lộ diện" (tên, nghĩa, màu chất) sau khi đáp xuống bàn — giữ cảm giác hồi hộp. */
export function useLanded<T extends { fan_index: number }>(draws: readonly T[]): T[] {
  const flights = useFlight((s) => s.flights);
  return flights.length ? draws.filter((d) => !flights.some((f) => f.fanIndex === d.fan_index)) : (draws as T[]);
}

/** Số lá đang bay mà Rust CHƯA ghi nhận (lá đã ghi nằm trong `picks` — tránh đếm 2 lần). */
export function unconfirmed(flights: Flight[], picks: readonly number[]): number {
  return flights.filter((f) => !picks.includes(f.fanIndex)).length;
}

/** Hộp của 1 phần tử (bỏ qua góc xoay CSS: dùng offsetWidth thay vì bounding box đã xoay). */
export function boxOf(el: Element | null, rot = 0): CardBox | null {
  if (!(el instanceof HTMLElement)) return null;
  const r = el.getBoundingClientRect();
  return { cx: r.left + r.width / 2, cy: r.top + r.height / 2, w: el.offsetWidth || r.width, rot };
}

/** Ô đích trên bàn trải (ưu tiên bàn trải trong popup bốc bài nếu đang mở). */
export function slotBox(position: number): CardBox | null {
  const el =
    document.querySelector<HTMLElement>(`[data-popup-slot="${position}"]`) ??
    document.querySelector<HTMLElement>(`[data-slot="${position}"]`);
  return el ? boxOf(el, Number(el.dataset.rot ?? 0)) : null;
}
