import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { motion } from "motion/react";
import { Shuffle, WandSparkles } from "lucide-react";
import type { Spread } from "../../lib/contracts";
import { useReading } from "../../store/reading";
import { Button } from "../ui";
import { CardBack } from "./TarotCard";

const DECK = 78;

/**
 * Quạt 78 lá úp (plan §2 — DeckFan). `fan_index` là chỉ số CỐ ĐỊNH: lá đã bốc biến khỏi quạt
 * nhưng chỉ số các lá còn lại không đổi. Chuột: click. Bàn phím: ←/→, PageUp/PageDown (±10),
 * Home/End, Enter/Space để bốc.
 */
export function DeckFan({ spread, animate }: { spread: Spread; animate: boolean }) {
  const reading = useReading((s) => s.reading);
  const phase = useReading((s) => s.phase);
  const busy = useReading((s) => s.busy);
  const draw = useReading((s) => s.draw);
  const autoDraw = useReading((s) => s.autoDraw);
  const reshuffle = useReading((s) => s.reshuffle);

  const picks = useMemo(() => new Set(reading?.picks ?? []), [reading?.picks]);
  const remaining = useMemo(() => Array.from({ length: DECK }, (_, i) => i).filter((i) => !picks.has(i)), [picks]);
  const [cursor, setCursor] = useState(0);
  const [hover, setHover] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const done = (reading?.draws.length ?? 0) >= spread.positions.length;
  const shuffling = phase === "shuffling";
  const next = spread.positions[reading?.draws.length ?? 0];

  // Con trỏ luôn nằm trên 1 lá còn lại.
  useEffect(() => {
    if (!remaining.includes(cursor)) setCursor(remaining.find((i) => i > cursor) ?? remaining.at(-1) ?? 0);
  }, [remaining, cursor]);

  if (!reading) return null;

  const pick = async (i: number) => {
    if (busy || done || shuffling || picks.has(i)) return;
    await draw(i);
  };

  const onKey = (e: KeyboardEvent) => {
    const pos = remaining.indexOf(cursor);
    const move = (d: number) => {
      e.preventDefault();
      const n = remaining[Math.min(remaining.length - 1, Math.max(0, pos + d))];
      if (n !== undefined) setCursor(n);
    };
    switch (e.key) {
      case "ArrowRight":
        return move(1);
      case "ArrowLeft":
        return move(-1);
      case "PageDown":
        return move(10);
      case "PageUp":
        return move(-10);
      case "Home":
        return move(-DECK);
      case "End":
        return move(DECK);
      case "Enter":
      case " ":
        e.preventDefault();
        void pick(cursor);
    }
  };

  const n = remaining.length;
  return (
    <section aria-label="Quạt bài" className="space-y-2">
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span className="text-fg-muted" aria-live="polite">
          {shuffling ? "Đang tráo bài…" : done ? "Đã bốc đủ lá." : `Bốc lá ${reading.draws.length + 1}/${spread.positions.length} — vị trí “${next?.label_vi}”`}
        </span>
        <div className="ml-auto flex gap-2">
          <Button size="sm" variant="ghost" icon={<Shuffle className="size-3.5" />} disabled={busy || shuffling || reading.picks.length > 0} onClick={() => void reshuffle()}>
            Tráo lại
          </Button>
          <Button size="sm" icon={<WandSparkles className="size-3.5" />} disabled={busy || shuffling || done} onClick={() => void autoDraw()}>
            Bốc tự động
          </Button>
        </div>
      </div>
      <div
        ref={ref}
        role="listbox"
        tabIndex={done ? -1 : 0}
        aria-label={`Quạt bài: còn ${n} lá úp. Dùng mũi tên trái phải để chọn, PageUp PageDown nhảy 10 lá, Enter để bốc.`}
        aria-activedescendant={done ? undefined : `fan-${cursor}`}
        aria-disabled={done || shuffling}
        onKeyDown={onKey}
        className="relative h-40 w-full overflow-hidden rounded-panel bg-[#120f22] outline-offset-2 focus-visible:outline-2 focus-visible:outline-amber-300"
      >
        {remaining.map((i, k) => {
          const t = n <= 1 ? 0.5 : k / (n - 1);
          const angle = (t - 0.5) * 36;
          // Vòng cung: giữa cao, hai đầu thấp.
          const lift = (1 - 4 * (t - 0.5) ** 2) * 22;
          const active = i === cursor || i === hover;
          return (
            <motion.div
              key={i}
              id={`fan-${i}`}
              role="option"
              aria-selected={i === cursor}
              aria-label={`Lá úp số ${i + 1}`}
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover((h) => (h === i ? null : h))}
              onClick={() => {
                setCursor(i);
                void pick(i);
              }}
              className="absolute bottom-3 w-[48px] cursor-pointer"
              style={{ left: `calc(${4 + t * 92}% - 24px)`, aspectRatio: "35 / 60", transformOrigin: "50% 100%" }}
              initial={false}
              animate={
                shuffling && animate
                  ? { rotate: angle * 0.2, y: [0, -14 - (k % 5) * 3, 0], x: [0, (k % 2 ? 1 : -1) * 18, 0] }
                  : { rotate: angle, y: -lift - (active && !done ? 18 : 0), x: 0 }
              }
              transition={
                shuffling && animate ? { duration: 0.5, repeat: 1, delay: (k % 13) * 0.02 } : { type: "spring", stiffness: 380, damping: 28 }
              }
            >
              <div className={active && !done ? "drop-shadow-[0_0_6px_rgba(252,211,77,0.8)]" : undefined}>
                <CardBack />
              </div>
            </motion.div>
          );
        })}
      </div>
    </section>
  );
}
