import { create } from "zustand";
import type { Card, Deck, Spread } from "../lib/contracts";
import { readingIpc } from "../lib/ipc";
import { toastError } from "./toast";

/** Bộ bài + kiểu trải từ Rust (nguồn sự thật duy nhất), nạp 1 lần. */
interface CatalogStore {
  deck: Deck | null;
  spreads: Spread[];
  byId: Map<string, Card>;
  loading: boolean;
  load: () => Promise<void>;
}

export const useCatalog = create<CatalogStore>((set, get) => ({
  deck: null,
  spreads: [],
  byId: new Map(),
  loading: false,
  load: async () => {
    if (get().deck || get().loading) return;
    set({ loading: true });
    try {
      const [deck, spreads] = await Promise.all([readingIpc.getDeck(), readingIpc.listSpreads()]);
      set({ deck, spreads, byId: new Map(deck.cards.map((c) => [c.id, c])) });
    } catch (e) {
      toastError(e, "Không nạp được bộ bài");
    } finally {
      set({ loading: false });
    }
  },
}));

export const spreadOf = (spreads: Spread[], id: string | undefined) => spreads.find((s) => s.id === id);
