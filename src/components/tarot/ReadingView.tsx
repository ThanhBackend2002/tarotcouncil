import { useEffect, useState } from "react";
import { AnimatePresence } from "motion/react";
import { Layers, LoaderCircle } from "lucide-react";
import { useAnimationsOn } from "../../lib/useAnimations";
import { spreadOf, useCatalog } from "../../store/catalog";
import { useFlight } from "../../store/flight";
import { useReading } from "../../store/reading";
import { Button } from "../ui";
import { AskPanel } from "./AskPanel";
import { CardFlights } from "./CardFlights";
import { DrawOverlay } from "./DrawOverlay";
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
  const flights = useFlight((s) => s.flights);
  const animate = useAnimationsOn();
  const [minimized, setMinimized] = useState(false);
  const [lingering, setLingering] = useState(false);

  useEffect(() => {
    void load();
  }, [load]);

  const spread = reading ? spreadOf(spreads, reading.spread_id) : undefined;
  const needsDraw = !!reading && !!spread && (phase === "drawing" || phase === "shuffling") && (reading.draws.length < spread.positions.length || flights.length > 0);

  // Mở lại popup tự động khi bắt đầu lượt mới hoặc tráo lại bài.
  useEffect(() => {
    if (phase === "shuffling") setMinimized(false);
  }, [phase, reading?.id]);

  // Giữ popup thêm một nhịp ngắn sau khi lá cuối đáp để người dùng kịp thấy lá lật ngửa.
  useEffect(() => {
    if (needsDraw) {
      setLingering(true);
      return;
    }
    if (!lingering) return;
    const t = setTimeout(() => setLingering(false), animate ? 750 : 0);
    return () => clearTimeout(t);
  }, [needsDraw, lingering, animate]);

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

  if (!spread) return <div className="p-6 text-sm text-fg-subtle">Đang nạp kiểu trải…</div>;
  const showOverlay = !minimized && (needsDraw || lingering) && (phase === "drawing" || phase === "shuffling");

  return (
    <div className="grid h-full grid-cols-[minmax(520px,1fr)_1fr] gap-3 p-3">
      <div className="flex min-h-0 min-w-0 flex-col gap-3 overflow-y-auto">
        <ReadingControls spread={spread} />
        {minimized && needsDraw && (
          <Button variant="primary" icon={<Layers className="size-4" />} onClick={() => setMinimized(false)}>
            Mở bàn bốc bài ({reading.draws.length}/{spread.positions.length})
          </Button>
        )}
        {/* data-scene: cảnh pixel luôn tối, không theo theme sáng/tối. */}
        <div data-scene className="contents">
          <ReadingRoom spread={spread} />
        </div>
        <ReadingChat spread={spread} />
      </div>
      <div className="min-h-0 min-w-0 overflow-y-auto" aria-label="Bàn trải và lời giải">
        <ReadingResult spread={spread} animate={animate} />
      </div>
      <AnimatePresence>{showOverlay && <DrawOverlay spread={spread} animate={animate} onMinimize={() => setMinimized(true)} />}</AnimatePresence>
      <PrivacyNotice />
      <CardFlights />
    </div>
  );
}
