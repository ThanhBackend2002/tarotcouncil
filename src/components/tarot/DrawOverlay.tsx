import { useEffect } from "react";
import { createPortal } from "react-dom";
import { motion } from "motion/react";
import { Minimize2, Sparkles } from "lucide-react";
import type { Spread } from "../../lib/contracts";
import { useFlight } from "../../store/flight";
import { useReading } from "../../store/reading";
import { Button } from "../ui";
import { DeckFan } from "./DeckFan";
import { SpreadBoard } from "./ReadingResult";

/**
 * Popup trong suốt toàn màn hình khi tráo & bốc bài, chia 2 phần trên/dưới:
 * - Trên: tiêu đề lượt đọc + bàn trải bài lớn (lá bay đáp vào đây).
 * - Dưới: quạt 78 lá úp để chọn lá.
 */
export function DrawOverlay({
  spread,
  animate,
  onMinimize,
}: {
  spread: Spread;
  animate: boolean;
  onMinimize: () => void;
}) {
  const reading = useReading((s) => s.reading);
  const phase = useReading((s) => s.phase);
  const flights = useFlight((s) => s.flights);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onMinimize();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onMinimize]);

  if (!reading) return null;

  const full = reading.draws.length >= spread.positions.length && flights.length === 0;
  const caption =
    phase === "shuffling"
      ? "Đang tráo bài…"
      : full
        ? "Đã bốc đủ lá!"
        : `Bốc ${reading.draws.length}/${spread.positions.length} lá`;

  return createPortal(
    <motion.div
      role="dialog"
      aria-modal="true"
      aria-label={`Bốc bài — ${spread.name_vi}`}
      initial={animate ? { opacity: 0 } : false}
      animate={{ opacity: 1 }}
      exit={animate ? { opacity: 0 } : undefined}
      transition={{ duration: 0.2 }}
      className="fixed inset-0 z-40 flex flex-col justify-between gap-4 bg-[#090714]/70 p-4 backdrop-blur-md sm:p-6"
    >
      {/* Phần trên: thông tin lượt đọc + bàn trải bài */}
      <div className="mx-auto flex min-h-0 w-full max-w-5xl flex-1 flex-col gap-3">
        <header className="flex items-center gap-3 rounded-panel border border-white/10 bg-black/35 px-4 py-2.5 text-neutral-100 backdrop-blur-sm">
          <Sparkles className="size-4 shrink-0 text-amber-300" aria-hidden />
          <div className="min-w-0 flex-1">
            <p className="text-[11px] tracking-wide text-amber-200/80 uppercase">
              {spread.name_vi} · {caption}
            </p>
            <p className="truncate text-sm font-medium text-white" title={reading.question}>
              “{reading.question}”
            </p>
          </div>
          <Button size="sm" variant="ghost" icon={<Minimize2 className="size-3.5" />} onClick={onMinimize} className="text-neutral-200 hover:bg-white/10 hover:text-white">
            Thu gọn
          </Button>
        </header>

        <div className="flex min-h-0 flex-1 items-center justify-center">
          <SpreadBoard
            spread={spread}
            animate={animate}
            slotAttr="data-popup-slot"
            className="w-full max-w-[min(820px,calc(40vh*1.6))] border-amber-400/25 bg-[radial-gradient(ellipse_at_center,rgba(76,29,149,0.72)_0%,rgba(46,16,101,0.68)_60%,rgba(30,16,54,0.75)_100%)] shadow-2xl shadow-black/50"
          />
        </div>
      </div>

      {/* Phần dưới: quạt 78 lá */}
      <div className="mx-auto w-full max-w-5xl shrink-0 rounded-panel border border-white/10 bg-black/35 p-3 backdrop-blur-sm">
        <DeckFan spread={spread} animate={animate} />
      </div>
    </motion.div>,
    document.body,
  );
}
