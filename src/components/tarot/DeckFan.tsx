import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { motion } from "motion/react";
import { Shuffle, WandSparkles } from "lucide-react";
import type { Spread } from "../../lib/contracts";
import { boxOf, unconfirmed, useFlight } from "../../store/flight";
import { SHUFFLE_MS, useReading } from "../../store/reading";
import { Button } from "../ui";
import { CardBack } from "./TarotCard";

const DECK = 78;
const CARD_W = 50;
const PAD = 14;
/** Nhịp chia bài khi "Bốc tự động". */
const DEAL_GAP_MS = 380;

const sleep = (ms: number) => new Promise((ok) => setTimeout(ok, ms));

/** Hình học CỐ ĐỊNH của ô `i` trong quạt (không dồn chỗ khi rút lá → quạt không bị giật). */
function slot(i: number, width: number) {
  const t = i / (DECK - 1);
  const x = PAD + t * Math.max(0, width - 2 * PAD - CARD_W);
  const lift = (1 - 4 * (t - 0.5) ** 2) * 22; // vòng cung: giữa cao, hai đầu thấp
  const rot = (t - 0.5) * 34;
  return { x, lift, rot };
}

/**
 * Quạt 78 lá úp (plan §2 — DeckFan). `fan_index` là chỉ số CỐ ĐỊNH: lá đã bốc để lại khe trống,
 * các lá khác đứng yên. Chuột: click. Bàn phím: ←/→, PageUp/PageDown (±10), Home/End, Enter/Space.
 * Lá được bốc bay sang ô trên bàn trải (CardFlights) rồi lật ra.
 */
export function DeckFan({ spread, animate }: { spread: Spread; animate: boolean }) {
  const reading = useReading((s) => s.reading);
  const phase = useReading((s) => s.phase);
  const busy = useReading((s) => s.busy);
  const draw = useReading((s) => s.draw);
  const autoDraw = useReading((s) => s.autoDraw);
  const reshuffle = useReading((s) => s.reshuffle);
  const flights = useFlight((s) => s.flights);
  const launch = useFlight((s) => s.launch);
  const land = useFlight((s) => s.land);

  const box = useRef<HTMLDivElement>(null);
  const cards = useRef(new Map<number, HTMLDivElement>());
  const [width, setWidth] = useState(0);
  const [cursor, setCursor] = useState(0);
  const [hover, setHover] = useState<number | null>(null);
  const [pending, setPending] = useState<number | null>(null);
  const [dealing, setDealing] = useState(false);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const flying = useMemo(() => new Set(flights.map((f) => f.fanIndex)), [flights]);
  const picks = useMemo(() => new Set(reading?.picks ?? []), [reading?.picks]);
  const remaining = useMemo(() => Array.from({ length: DECK }, (_, i) => i).filter((i) => !picks.has(i) && !flying.has(i)), [picks, flying]);
  const drawn = (reading?.draws.length ?? 0) + unconfirmed(flights, reading?.picks ?? []);
  const done = drawn >= spread.positions.length;
  const shuffling = phase === "shuffling";
  const locked = busy || dealing || shuffling || done || pending !== null;
  const next = spread.positions[Math.min(drawn, spread.positions.length - 1)];

  // Con trỏ luôn nằm trên 1 lá còn lại.
  useEffect(() => {
    if (remaining.length && !remaining.includes(cursor)) setCursor(remaining.find((i) => i > cursor) ?? remaining.at(-1)!);
  }, [remaining, cursor]);

  if (!reading) return null;

  /** Bốc 1 lá: cất cánh ngay (cảm giác tức thì), ghi ở Rust, lỗi → huỷ chuyến bay. */
  const pick = async (i: number): Promise<boolean> => {
    if (picks.has(i) || flying.has(i)) return false;
    const cur = useReading.getState().reading;
    const position = (cur?.draws.length ?? 0) + unconfirmed(useFlight.getState().flights, cur?.picks ?? []);
    if (position >= spread.positions.length) return false;
    const s = slot(i, width);
    const from = boxOf(cards.current.get(i) ?? null, s.rot);
    const id = animate && from ? launch({ fanIndex: i, position, from }) : null;
    setPending(i);
    const ok = await draw(i);
    setPending(null);
    if (!ok && id !== null) land(id);
    return ok;
  };

  const onPick = (i: number) => {
    if (locked) return;
    setCursor(i);
    void pick(i);
  };

  const deal = async () => {
    if (locked) return;
    if (!animate) return void autoDraw();
    setDealing(true);
    try {
      const cur = useReading.getState().reading;
      let n = (cur?.draws.length ?? 0) + unconfirmed(useFlight.getState().flights, cur?.picks ?? []);
      for (let i = 0; i < DECK && n < spread.positions.length; i++) {
        const r = useReading.getState().reading;
        if (!r || r.picks.includes(i)) continue;
        if (!(await pick(i))) break;
        n += 1;
        await sleep(DEAL_GAP_MS);
      }
    } finally {
      setDealing(false);
    }
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
        onPick(cursor);
    }
  };

  const center = width / 2 - CARD_W / 2;
  return (
    <section aria-label="Quạt bài" className="space-y-2">
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span className="text-fg-muted" aria-live="polite">
          {shuffling
            ? "Đang tráo bài…"
            : done
              ? "Đã bốc đủ lá."
              : dealing
                ? "Đang chia bài…"
                : `Bốc lá ${drawn + 1}/${spread.positions.length} — vị trí “${next?.label_vi}”`}
        </span>
        <div className="ml-auto flex gap-2">
          <Button size="sm" variant="ghost" icon={<Shuffle className="size-3.5" />} disabled={locked || reading.picks.length > 0} onClick={() => void reshuffle()}>
            Tráo lại
          </Button>
          <Button size="sm" icon={<WandSparkles className="size-3.5" />} disabled={locked} onClick={() => void deal()}>
            Bốc tự động
          </Button>
        </div>
      </div>
      <div
        ref={box}
        role="listbox"
        tabIndex={done ? -1 : 0}
        aria-label={`Quạt bài: còn ${remaining.length} lá úp. Dùng mũi tên trái phải để chọn, PageUp PageDown nhảy 10 lá, Enter để bốc.`}
        aria-activedescendant={done ? undefined : `fan-${cursor}`}
        aria-disabled={locked}
        onKeyDown={onKey}
        className="relative h-44 w-full overflow-hidden rounded-panel bg-[radial-gradient(ellipse_at_bottom,#2e1065_0%,#120f22_70%)] outline-offset-2 focus-visible:outline-2 focus-visible:outline-amber-300"
      >
        {width > 0 &&
          remaining.map((i, k) => {
            const s = slot(i, width);
            const active = !locked && (i === cursor || i === hover);
            const isPending = i === pending;
            const jitter = ((i * 37) % 23) - 11;
            return (
              <motion.div
                key={i}
                ref={(el) => {
                  if (el) cards.current.set(i, el);
                  else cards.current.delete(i);
                }}
                id={`fan-${i}`}
                role="option"
                aria-selected={i === cursor}
                aria-label={`Lá úp số ${i + 1}`}
                onMouseEnter={() => setHover(i)}
                onMouseLeave={() => setHover((h) => (h === i ? null : h))}
                onClick={() => onPick(i)}
                className={locked ? "absolute bottom-4 cursor-default" : "absolute bottom-4 cursor-pointer"}
                style={{ width: CARD_W, aspectRatio: "35 / 60", transformOrigin: "50% 100%", left: 0, zIndex: isPending ? 200 : active ? 100 : i }}
                initial={false}
                animate={
                  shuffling && animate
                    ? {
                        // Gom vào giữa → trộn → xoè lại về ô cố định.
                        x: [s.x, center + jitter, center - jitter * 0.6, center + jitter * 0.3, s.x],
                        y: [-s.lift, -10, -16 - (k % 4) * 2, -10, -s.lift],
                        rotate: [s.rot, jitter * 0.5, -jitter * 0.4, jitter * 0.2, s.rot],
                      }
                    : { x: s.x, y: -s.lift - (isPending ? 34 : active ? 20 : 0), rotate: s.rot, scale: isPending ? 1.08 : 1 }
                }
                transition={
                  shuffling && animate
                    ? { duration: SHUFFLE_MS / 1000, times: [0, 0.3, 0.55, 0.75, 1], ease: "easeInOut", delay: (k % 9) * 0.008 }
                    : { type: "spring", stiffness: 420, damping: 30, mass: 0.6 }
                }
              >
                <div
                  className={
                    isPending
                      ? "drop-shadow-[0_0_10px_rgba(252,211,77,0.95)]"
                      : active
                        ? "drop-shadow-[0_0_6px_rgba(252,211,77,0.8)]"
                        : "drop-shadow-[0_2px_2px_rgba(0,0,0,0.5)]"
                  }
                >
                  <CardBack />
                </div>
              </motion.div>
            );
          })}
      </div>
    </section>
  );
}
