import { useEffect } from "react";
import { LoaderCircle } from "lucide-react";
import { useAnimationsOn } from "../../lib/useAnimations";
import { spreadOf, useCatalog } from "../../store/catalog";
import { useReading } from "../../store/reading";
import { AskPanel } from "./AskPanel";
import { DeckFan } from "./DeckFan";
import { PrivacyNotice } from "./PrivacyNotice";
import { ReadingResult } from "./ReadingResult";
import { ReadingRoom } from "./ReadingRoom";

/** Luồng chính: idle → asking → shuffling → drawing(k/n) → interpreting → done (+ support / error / cancelled). */
export function ReadingView() {
  const load = useCatalog((s) => s.load);
  const spreads = useCatalog((s) => s.spreads);
  const phase = useReading((s) => s.phase);
  const reading = useReading((s) => s.reading);
  const buffering = useReading((s) => s.buffering);
  const animate = useAnimationsOn();

  useEffect(() => {
    void load();
  }, [load]);

  if (phase === "idle" || phase === "asking" || !reading) {
    if (buffering) {
      return (
        <div className="flex h-full items-center justify-center gap-2 text-sm text-fg-subtle">
          <LoaderCircle className="size-4 animate-spin" /> Đang mở lượt đọc…
        </div>
      );
    }
    return (
      <div className="h-full overflow-y-auto">
        <AskPanel animate={animate} />
      </div>
    );
  }

  const spread = spreadOf(spreads, reading.spread_id);
  if (!spread) {
    return <div className="p-6 text-sm text-fg-subtle">Đang nạp kiểu trải…</div>;
  }
  const drawing = (phase === "drawing" || phase === "shuffling") && reading.draws.length < spread.positions.length;

  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
        <ReadingRoom spread={spread} animate={animate} />
        {drawing && <DeckFan spread={spread} animate={animate} />}
      </div>
      <aside className="w-[420px] shrink-0 overflow-y-auto border-l border-line p-4" aria-label="Lời giải">
        <ReadingResult spread={spread} />
      </aside>
      <PrivacyNotice />
    </div>
  );
}
