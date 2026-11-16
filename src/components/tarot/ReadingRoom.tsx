import { memo, useMemo } from "react";
import type { Spread } from "../../lib/contracts";
import { SPRITE_W } from "../../lib/pixel";
import type { ReaderView } from "../../lib/readingSync";
import { useResolver } from "../../store/config";
import { useReading } from "../../store/reading";
import { PixelCharacter, type AgentStatus, type Marker } from "../PixelCharacter";
import { TarotCard } from "./TarotCard";

// Cảnh 240×150 pixel logic (khung từ MeetingRoom của PlanCouncil), scale theo khung chứa.
const W = 240;
const H = 150;
const SEAT_Y = 30;
/** Mặt bàn (vùng đặt lá, toạ độ chuẩn hoá 0–1 của kiểu trải map vào đây). */
const TABLE = { x: 14, y: 60, w: 212, h: 86 };

const pct = (v: number, total: number) => `${(v / total) * 100}%`;

const TONE_ICON = { hopeful: "☀", cautionary: "⚠", neutral: "☾" } as const;

function bubbleOf(v: ReaderView | undefined, phase: string): { text: string; tone: string } | null {
  if (!v) return null;
  if (v.status === "thinking") return { text: "đang suy ngẫm…", tone: "#7c3aed" };
  if (v.status === "failed") return { text: "Ta chưa luận được lần này.", tone: "#d97706" };
  if (v.status === "done" && v.result?.output) {
    const o = v.result.output;
    const s = o.overall.trim();
    return { text: `${TONE_ICON[o.tone] ?? ""} ${s.length > 70 ? `${s.slice(0, 68)}…` : s}`, tone: "#16a34a" };
  }
  return phase === "interpreting" ? { text: "…", tone: "#64748b" } : null;
}

/**
 * Bề rộng lá (% chiều rộng cảnh) sao cho lá cao nhất vẫn lọt giữa 2 vị trí gần nhau nhất theo
 * chiều dọc / ngang (lá cao = rộng × 60/35; đơn vị cảnh: W=240, mặt bàn cao TABLE.h).
 */
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
  const byHeight = ((minDy * TABLE.h * 0.92) / (60 / 35) / W) * 100;
  const byWidth = ((minDx * TABLE.w * 0.8) / W) * 100;
  return Math.min(13, byHeight, byWidth, ((TABLE.h * 0.8) / (60 / 35) / W) * 100);
}

export const ReadingRoom = memo(function ReadingRoom({ spread, animate }: { spread: Spread; animate: boolean }) {
  const reading = useReading((s) => s.reading);
  const readers = useReading((s) => s.readers);
  const phase = useReading((s) => s.phase);
  const synthesisRunning = useReading((s) => s.synthesisRunning);
  const synthesis = useReading((s) => s.synthesis);
  const resolve = useResolver(reading?.cast);

  const seats = useMemo(() => {
    if (!reading) return [];
    const ids = [...reading.reader_ids, reading.chief_reader_id];
    const step = (TABLE.w - 20) / ids.length;
    return ids.map((id, i) => ({ id, chief: id === reading.chief_reader_id, x: Math.round(TABLE.x + 10 + step * (i + 0.5) - SPRITE_W / 2) }));
  }, [reading]);

  if (!reading) return null;
  const cw = cardWidth(spread);

  const statusOf = (id: string, chief: boolean): AgentStatus => {
    if (chief) return synthesisRunning ? "thinking" : synthesis ? "done" : "idle";
    const v = readers[id];
    return v?.status === "thinking" ? "thinking" : v?.status === "done" ? "done" : v?.status === "failed" ? "error" : "idle";
  };
  const markerOf = (id: string, chief: boolean): Marker => {
    if (chief) return synthesis ? "approve" : null;
    const v = readers[id];
    return v?.status === "done" ? "approve" : v?.status === "failed" ? "error" : null;
  };

  return (
    <section
      aria-label={`Bàn đọc bài — ${spread.name_vi}`}
      className="relative w-full overflow-hidden rounded-panel border border-neutral-800 bg-[#1b1530]"
      style={{ aspectRatio: `${W} / ${H}` }}
    >
      <svg className="px-scene absolute inset-0 size-full" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet" shapeRendering="crispEdges" aria-hidden>
        <Room />
        {seats.map((s) => (
          <PixelCharacter
            key={s.id}
            meta={resolve(s.id)}
            x={s.x}
            y={SEAT_Y}
            status={statusOf(s.id, s.chief)}
            speaking={statusOf(s.id, s.chief) === "thinking"}
            marker={markerOf(s.id, s.chief)}
          />
        ))}
        <Table animate={animate} />
      </svg>

      {/* Nhãn tên + bong bóng thoại (HTML để chữ sắc nét) */}
      {seats.map((s) => {
        const m = resolve(s.id);
        const b = s.chief
          ? synthesisRunning
            ? { text: "đang tổng hợp lời hội đồng…", tone: "#7c3aed" }
            : synthesis
              ? { text: `✦ ${synthesis.summary.slice(0, 68)}${synthesis.summary.length > 68 ? "…" : ""}`, tone: "#f59e0b" }
              : null
          : bubbleOf(readers[s.id], phase);
        return (
          <div key={s.id}>
            <div
              className="px-font pointer-events-none absolute max-w-[22%] -translate-x-1/2 truncate rounded-[1px] px-1 text-[9px] leading-[13px] font-semibold text-neutral-950"
              style={{ left: pct(s.x + SPRITE_W / 2, W), top: pct(SEAT_Y + 25, H), background: m.color }}
              title={`${m.name}${m.title ? ` — ${m.title}` : ""}`}
            >
              {s.chief ? `♛ ${m.name}` : m.name}
            </div>
            {b && (
              <div
                className="pointer-events-none absolute z-10 w-max max-w-[20%] -translate-x-1/2 -translate-y-full"
                style={{ left: pct(Math.min(W - 28, Math.max(28, s.x + SPRITE_W / 2)), W), top: pct(SEAT_Y - 2, H) }}
              >
                <div
                  className="px-box px-font rounded-[1px] bg-[#fdfdf8] px-1.5 py-0.5 text-[10px] leading-snug text-neutral-900"
                  style={{ ["--px-border" as string]: b.tone }}
                >
                  <span className="line-clamp-2">{b.text}</span>
                </div>
              </div>
            )}
          </div>
        );
      })}

      {/* Lá bài đặt theo vị trí của kiểu trải */}
      {spread.positions.map((p) => {
        const d = reading.draws.find((x) => x.position === p.index) ?? null;
        const left = ((TABLE.x + p.x * TABLE.w) / W) * 100;
        const top = ((TABLE.y + p.y * TABLE.h) / H) * 100;
        return (
          <div
            key={p.index}
            className="absolute -translate-x-1/2 -translate-y-1/2"
            style={{ left: `${left}%`, top: `${top}%`, width: `${cw}%`, zIndex: p.rotation ? 3 : 2 }}
          >
            {d ? (
              <TarotCard draw={d} revealed width="100%" animate={animate} rotation={p.rotation} />
            ) : (
              <div
                className="rounded-[6%] border border-dashed border-amber-200/40 bg-black/10"
                style={{ aspectRatio: "35 / 60", rotate: `${p.rotation}deg` }}
                title={p.label_vi}
              />
            )}
          </div>
        );
      })}

      <div className="px-font pointer-events-none absolute top-2 right-2 rounded-sm bg-black/55 px-2 py-1 text-[10px] text-neutral-200">
        {spread.name_vi.toUpperCase()} · {reading.draws.length}/{spread.positions.length}
      </div>
    </section>
  );
});

/** Tường, cửa sổ trăng, kệ sách, rèm — rect thuần. */
const Room = memo(function Room() {
  const stars = [
    [182, 9],
    [205, 14],
    [196, 22],
    [222, 8],
  ];
  return (
    <g>
      <rect x={0} y={0} width={W} height={H} fill="#231b3d" />
      {[16, 48, 80, 112, 144, 176, 208].map((x) => (
        <rect key={x} x={x} y={0} width={1} height={58} fill="#2c2350" />
      ))}
      {/* cửa sổ đêm + trăng */}
      <rect x={172} y={4} width={58} height={30} fill="#3b2f63" />
      <rect x={174} y={6} width={54} height={26} fill="#0f1230" />
      {stars.map(([x, y]) => (
        <rect key={`${x}`} x={x} y={y} width={1} height={1} fill="#f8fafc" className="px-dot" />
      ))}
      <rect x={210} y={10} width={8} height={8} fill="#fef3c7" />
      <rect x={213} y={10} width={5} height={6} fill="#0f1230" />
      <rect x={200} y={6} width={2} height={26} fill="#3b2f63" />
      {/* rèm */}
      <rect x={168} y={2} width={6} height={36} fill="#7e22ce" />
      <rect x={228} y={2} width={6} height={36} fill="#7e22ce" />
      {/* kệ sách + quả cầu */}
      <rect x={8} y={10} width={44} height={3} fill="#5b3a1f" />
      {[10, 14, 18, 22, 27, 31].map((x, i) => (
        <rect key={x} x={x} y={2 + (i % 2)} width={3} height={8 - (i % 2)} fill={["#a855f7", "#f59e0b", "#22c55e", "#ef4444", "#38bdf8", "#eab308"][i]} />
      ))}
      <rect x={40} y={4} width={8} height={6} fill="#67e8f9" opacity={0.7} />
      <rect x={42} y={5} width={2} height={2} fill="#ecfeff" />
      <rect x={8} y={22} width={44} height={3} fill="#5b3a1f" />
      <rect x={12} y={15} width={6} height={7} fill="#fde68a" />
      <rect x={24} y={17} width={10} height={5} fill="#c084fc" />
      {/* sàn */}
      <rect x={0} y={58} width={W} height={H - 58} fill="#1a1430" />
    </g>
  );
});

/** Bàn khăn tím + nến (ngọn lửa nhấp nháy, tắt khi giảm chuyển động). */
const Table = memo(function Table({ animate }: { animate: boolean }) {
  const { x, y, w, h } = TABLE;
  const candle = (cx: number) => (
    <g key={cx}>
      <rect x={cx} y={y - 9} width={3} height={8} fill="#fef3c7" />
      <rect x={cx + 1} y={y - 11} width={1} height={2} fill="#1f2937" />
      <rect x={cx} y={y - 15} width={3} height={4} fill="#fbbf24" className={animate ? "px-blink" : undefined} opacity={0.95} />
      <rect x={cx + 1} y={y - 14} width={1} height={2} fill="#fff7ed" />
      <rect x={cx - 3} y={y - 1} width={9} height={2} fill="#78350f" />
    </g>
  );
  return (
    <g>
      <rect x={x - 4} y={y - 2} width={w + 8} height={h + 6} fill="#000" opacity={0.25} />
      <rect x={x} y={y} width={w} height={h} fill="#4c1d95" />
      <rect x={x} y={y} width={w} height={2} fill="#6d28d9" />
      <rect x={x + 6} y={y + 6} width={w - 12} height={h - 12} fill="#5b21b6" />
      <rect x={x + 7} y={y + 7} width={w - 14} height={h - 14} fill="none" stroke="#d9b26a" strokeOpacity={0.35} strokeWidth={0.6} />
      {/* tua rua */}
      {Array.from({ length: Math.floor(w / 6) }, (_, i) => (
        <rect key={i} x={x + 2 + i * 6} y={y + h} width={2} height={3} fill="#d9b26a" opacity={0.7} />
      ))}
      {[x + 4, x + w - 7].map(candle)}
    </g>
  );
});
