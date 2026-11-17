import { useEffect, useRef, useState } from "react";
import type { Card, Draw, Spread } from "../../lib/contracts";
import { cardImage } from "../../lib/cardArt";
import { useCatalog } from "../../store/catalog";
import { unconfirmed, useFlight, useLanded } from "../../store/flight";
import { useReading } from "../../store/reading";
import { Badge, Panel, cx } from "../ui";
import { TarotCard } from "./TarotCard";

/** Bàn trải: chiều cao chuẩn hoá của mặt bàn so với chiều rộng (khung 16:10). */
const BOARD_ASPECT = 10 / 16;
/** Vùng đặt lá chiếm 88% bề rộng, 84% chiều cao bàn (phần còn lại là lề). */
const SPAN_X = 0.88;
const SPAN_Y = 0.84;

/** Bề rộng lá (% bề rộng bàn) để lá cao nhất lọt giữa 2 vị trí gần nhau nhất. */
function cardWidth(spread: Spread): number {
  const ps = spread.positions;
  let minDy = 1;
  let minDx = 1;
  for (const a of ps) {
    for (const b of ps) {
      if (a === b || a.rotation || b.rotation) continue;
      if (Math.abs(a.x - b.x) < 0.05) minDy = Math.min(minDy, Math.abs(a.y - b.y));
      if (Math.abs(a.y - b.y) < 0.05) minDx = Math.min(minDx, Math.abs(a.x - b.x));
    }
  }
  // Chừa khoảng giữa 2 lá; trải ≤ 5 lá có nhãn vị trí dưới mỗi lá → chừa thêm.
  const gap = ps.length <= 5 ? 0.68 : 0.85;
  const byHeight = ((minDy * SPAN_Y * BOARD_ASPECT * gap) / (60 / 35)) * 100;
  const byWidth = minDx * SPAN_X * 0.8 * 100;
  return Math.min(18, byHeight, byWidth, ((BOARD_ASPECT * 0.8) / (60 / 35)) * 100);
}

function Placeholder({ rotation, glow }: { rotation: number; glow?: boolean }) {
  return (
    <div
      className={cx(
        "w-full rounded-[6%] border border-dashed",
        glow ? "animate-pulse border-amber-300 bg-amber-300/15 shadow-[0_0_16px_rgba(252,211,77,0.45)]" : "border-amber-200/40 bg-black/10",
      )}
      style={{ aspectRatio: "35 / 60", rotate: `${rotation}deg` }}
    />
  );
}

/**
 * Lá trên bàn trải. Lá vừa bốc (đang bay từ quạt) → giữ chỗ trống tới khi đáp → hiện mặt lưng → lật ngửa.
 * Lá có sẵn (mở lượt cũ) → hiện ngửa ngay, không hiệu ứng.
 */
function BoardCard({ draw, rotation, animate }: { draw: Draw; rotation: number; animate: boolean }) {
  const inFlight = useFlight((s) => s.flights.some((f) => f.fanIndex === draw.fan_index));
  const arriving = useRef(inFlight);
  const [revealed, setRevealed] = useState(!inFlight);
  useEffect(() => {
    if (!arriving.current || inFlight) return;
    arriving.current = false;
    const t = setTimeout(() => setRevealed(true), animate ? 90 : 0);
    return () => clearTimeout(t);
  }, [inFlight, animate]);
  if (inFlight) return <Placeholder rotation={rotation} glow />;
  return <TarotCard draw={draw} revealed={revealed} width="100%" animate={animate} rotation={rotation} />;
}

/** Lá RWS cỡ lớn đặt theo `x, y, rotation` của từng vị trí. */
export function SpreadBoard({
  spread,
  animate,
  slotAttr = "data-slot",
  className,
}: {
  spread: Spread;
  animate: boolean;
  slotAttr?: "data-slot" | "data-popup-slot";
  className?: string;
}) {
  const reading = useReading((s) => s.reading)!;
  const phase = useReading((s) => s.phase);
  const inAir = useFlight((s) => unconfirmed(s.flights, reading.picks));
  const cw = cardWidth(spread);
  const nextPos = phase === "drawing" ? reading.draws.length + inAir : -1;
  return (
    <section
      aria-label={`Bàn trải ${spread.name_vi}`}
      className={cx(
        "relative w-full overflow-hidden rounded-panel border border-amber-500/20 bg-[radial-gradient(ellipse_at_center,#4c1d95_0%,#2e1065_60%,#1e1036_100%)]",
        className,
      )}
      style={{ aspectRatio: `${1 / BOARD_ASPECT}` }}
    >
      <div className="pointer-events-none absolute inset-3 rounded-md border border-amber-300/20" />
      {spread.positions.map((p) => {
        const d = reading.draws.find((x) => x.position === p.index) ?? null;
        return (
          <div
            key={p.index}
            className="absolute flex -translate-x-1/2 -translate-y-1/2 flex-col items-center"
            style={{ left: `${((1 - SPAN_X) / 2 + p.x * SPAN_X) * 100}%`, top: `${((1 - SPAN_Y) / 2 + p.y * SPAN_Y) * 100}%`, width: `${cw}%`, zIndex: p.rotation ? 3 : 2 }}
          >
            {/* data-slot / data-popup-slot: đích của lá bay (CardFlights). */}
            <div {...{ [slotAttr]: p.index }} data-rot={p.rotation} className="w-full">
              {d ? <BoardCard draw={d} rotation={p.rotation} animate={animate} /> : <Placeholder rotation={p.rotation} glow={p.index === nextPos} />}
            </div>
            {!p.rotation && spread.positions.length <= 5 && (
              <span className="mt-1 max-w-[140%] truncate rounded-sm bg-black/50 px-1 text-[10px] text-amber-100">
                {p.index + 1}. {p.label_vi}
              </span>
            )}
          </div>
        );
      })}
    </section>
  );
}

function CardReference({ card, draw, hint }: { card: Card | undefined; draw: Draw; hint?: string }) {
  const kw = card ? (draw.reversed ? card.keywords_reversed : card.keywords_upright) : [];
  return (
    <Panel className="p-3">
      <article aria-label={`${draw.position_label_vi}: ${draw.name_vi}`} className="flex gap-3">
        <img
          src={cardImage(draw.art_key)}
          alt=""
          className={cx("h-24 shrink-0 rounded-sm object-cover shadow", draw.reversed && "rotate-180")}
          style={{ aspectRatio: "35 / 60" }}
        />
        <div className="min-w-0 flex-1 space-y-1">
          <p className="text-[11px] tracking-wide text-fg-subtle uppercase">
            {draw.position + 1}. {draw.position_label_vi}
            {hint && <span className="normal-case"> — {hint}</span>}
          </p>
          <h3 className="font-semibold">
            {draw.name_vi} <span className="text-xs font-normal text-fg-muted">({draw.name_en})</span>{" "}
            <Badge tone={draw.reversed ? "warning" : "success"}>{draw.reversed ? "Ngược" : "Xuôi"}</Badge>
          </h3>
          {kw.length > 0 && <p className="text-xs text-fg-subtle">{kw.join(" · ")}</p>}
          {card && <p className="text-sm leading-relaxed text-fg-soft">{draw.reversed ? card.meaning_reversed : card.meaning_upright}</p>}
        </div>
      </article>
    </Panel>
  );
}

/** Cột phải (vị trí PlanViewer của PlanCouncil): bàn trải lá lớn, lời Trưởng Thầy, nghĩa gốc từng lá. */
export function ReadingResult({ spread, animate }: { spread: Spread; animate: boolean }) {
  const reading = useReading((s) => s.reading);
  const synthesis = useReading((s) => s.synthesis);
  const byId = useCatalog((c) => c.byId);
  const landed = useLanded(reading?.draws ?? []);
  if (!reading) return null;

  return (
    <div className="space-y-3">
      <SpreadBoard spread={spread} animate={animate} />

      {synthesis && (
        <Panel className="space-y-2 border-amber-500/30 p-4">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold text-amber-500">
            <span aria-hidden>♛</span> Lời Trưởng Thầy
          </h2>
          <p className="text-sm leading-relaxed">{synthesis.summary}</p>
          {(
            [
              ["Hội đồng đồng thuận", synthesis.agreements],
              ["Các cách hiểu khác nhau", synthesis.divergences],
              ["Lời khuyên", synthesis.advice],
            ] as const
          ).map(([title, items]) =>
            items.length ? (
              <div key={title}>
                <h3 className="text-xs font-medium text-fg-muted">{title}</h3>
                <ul className="mt-0.5 list-disc space-y-0.5 pl-5 text-sm text-fg-soft">
                  {items.map((x, i) => (
                    <li key={i}>{x}</li>
                  ))}
                </ul>
              </div>
            ) : null,
          )}
          <p className="border-t border-line pt-2 text-[11px] text-fg-subtle italic">{synthesis.disclaimer}</p>
        </Panel>
      )}

      {landed.length > 0 && <h2 className="px-1 text-xs font-medium tracking-wide text-fg-muted uppercase">Nghĩa gốc các lá</h2>}
      {landed.map((d) => (
        <CardReference key={d.position} card={byId.get(d.card_id)} draw={d} hint={spread.positions[d.position]?.meaning_hint} />
      ))}
      {reading.draws.length === 0 && <p className="px-1 text-sm text-fg-subtle">Bốc lá từ quạt bài để đặt lên bàn trải.</p>}
    </div>
  );
}
