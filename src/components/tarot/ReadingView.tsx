import { useEffect } from "react";
import { LoaderCircle } from "lucide-react";
import { useAnimationsOn } from "../../lib/useAnimations";
import { spreadOf, useCatalog } from "../../store/catalog";
import { useReading } from "../../store/reading";
import { AskPanel } from "./AskPanel";
import { CardFlights } from "./CardFlights";
import { DeckFan } from "./DeckFan";
import { PrivacyNotice } from "./PrivacyNotice";
import { ReadingChat } from "./ReadingChat";
import { ReadingControls } from "./ReadingControls";
import { ReadingResult } from "./ReadingResult";
import { ReadingRoom } from "./ReadingRoom";

/**
 * Bố cục như CouncilView của PlanCouncil: trái (điều khiển + phòng đọc bài pixel + quạt bài + hội thoại),
 * phải (bàn trải lá lớn + lời Trưởng Thầy + nghĩa gốc).
 * Luồng: idle → asking → shuffling → drawing(k/n) → interpreting → done (+ support / error / cancelled).
 */
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
  if (!spread) return <div className="p-6 text-sm text-fg-subtle">Đang nạp kiểu trải…</div>;
  const drawing = (phase === "drawing" || phase === "shuffling") && reading.draws.length < spread.positions.length;

  return (
    <div className="grid h-full grid-cols-[minmax(520px,1fr)_1fr] gap-3 p-3">
      <div className="flex min-h-0 min-w-0 flex-col gap-3 overflow-y-auto">
        <ReadingControls spread={spread} />
        {/* data-scene: cảnh pixel luôn tối, không theo theme sáng/tối. */}
        <div data-scene className="contents">
          <ReadingRoom spread={spread} />
        </div>
        {drawing && <DeckFan spread={spread} animate={animate} />}
        <ReadingChat spread={spread} />
      </div>
      <div className="min-h-0 min-w-0 overflow-y-auto" aria-label="Bàn trải và lời giải">
        <ReadingResult spread={spread} animate={animate} />
      </div>
      <PrivacyNotice />
      <CardFlights />
    </div>
  );
}
