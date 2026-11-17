import { memo, useMemo, useState } from "react";
import type { Draw, RoleId, Spread } from "../../lib/contracts";
import type { RoleResolver } from "../../lib/agents";
import { SPRITE_W } from "../../lib/pixel";
import type { ReaderView } from "../../lib/readingSync";
import { useResolver } from "../../store/config";
import { useLanded } from "../../store/flight";
import { useReading } from "../../store/reading";
import { PixelCharacter, type AgentStatus, type Marker } from "../PixelCharacter";
import { cx } from "../ui";

// Phòng đọc bài pixel 240×150 (khung + cách xếp chỗ của MeetingRoom — PlanCouncil):
// Trưởng thầy ngồi đầu bàn, các thầy bói ngồi 2 hàng: hàng sau bị bàn che chân, hàng trước ngồi phía trước.
const W = 240;
const H = 150;
const CHIEF = { x: 66, y: 66 };
const TABLE = { x: 88, y: 72, w: 136, top: 18, skirt: 9 };
const TOP_Y = 52; // hàng sau bàn
const BOTTOM_Y = 94; // hàng trước bàn

interface Seat {
  id: RoleId;
  x: number;
  y: number;
  row: "chief" | "top" | "bottom";
}

function layout(readers: RoleId[], chief: RoleId): Seat[] {
  const top = readers.slice(0, Math.ceil(readers.length / 2));
  const bottom = readers.slice(top.length);
  const spread = (ids: RoleId[], row: "top" | "bottom", y: number, offset: number) => {
    const step = (TABLE.w - 16) / Math.max(ids.length, 1);
    return ids.map((id, i) => ({ id, row, y, x: Math.round(TABLE.x + 8 + step * (i + 0.5) + offset - SPRITE_W / 2) }));
  };
  return [
    { id: chief, x: CHIEF.x, y: CHIEF.y, row: "chief" as const },
    ...spread(top, "top", TOP_Y, 0),
    ...spread(bottom, "bottom", BOTTOM_Y, top.length > bottom.length ? 6 : 0),
  ];
}

const pct = (v: number, total: number) => `${(v / total) * 100}%`;

interface Speech {
  id: RoleId;
  text: string;
  tone: "info" | "ok" | "warn" | "chief";
}

const TONE_BORDER: Record<Speech["tone"], string> = { info: "#7c3aed", ok: "#16a34a", warn: "#d97706", chief: "#f59e0b" };
const TONE_ICON = { hopeful: "☀", cautionary: "⚠", neutral: "☾" } as const;
const clip = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

function readerSpeech(id: RoleId, v: ReaderView | undefined): Speech | null {
  if (!v) return null;
  if (v.status === "thinking") return { id, text: "Để ta xem các lá này…", tone: "info" };
  if (v.status === "failed") return { id, text: "Lần này ta chưa luận được.", tone: "warn" };
  const o = v.result?.output;
  if (v.status === "done" && o) return { id, text: `${TONE_ICON[o.tone] ?? ""} ${clip(o.overall.trim(), 110)}`, tone: "ok" };
  return null;
}

/** Màu lá mini theo chất. */
const SUIT_COLOR: Record<string, string> = {
  major: "#7c3aed",
  wands: "#ea580c",
  cups: "#2563eb",
  swords: "#94a3b8",
  pentacles: "#ca8a04",
};

export const ReadingRoom = memo(function ReadingRoom({ spread }: { spread: Spread }) {
  const reading = useReading((s) => s.reading);
  const readers = useReading((s) => s.readers);
  const phase = useReading((s) => s.phase);
  const synthesisRunning = useReading((s) => s.synthesisRunning);
  const synthesis = useReading((s) => s.synthesis);
  const resolve = useResolver(reading?.cast);
  const [hover, setHover] = useState<RoleId | null>(null);
  const landed = useLanded(reading?.draws ?? []);

  const seats = useMemo(() => (reading ? layout(reading.reader_ids, reading.chief_reader_id) : []), [reading]);
  if (!reading) return null;
  const chief = reading.chief_reader_id;

  const statusOf = (s: Seat): AgentStatus => {
    if (s.row === "chief") return synthesisRunning ? "thinking" : synthesis ? "done" : "idle";
    const v = readers[s.id];
    return v?.status === "thinking" ? "thinking" : v?.status === "done" ? "done" : v?.status === "failed" ? "error" : "idle";
  };
  const markerOf = (s: Seat): Marker => {
    if (s.row === "chief") return synthesis ? "approve" : null;
    const v = readers[s.id];
    return v?.status === "done" ? "approve" : v?.status === "failed" ? "error" : null;
  };

  // Ai đang nói: hover > Trưởng thầy (đang/đã tổng hợp) > người vừa luận xong gần nhất.
  const speech: Speech | null = (() => {
    if (hover) {
      if (hover === chief && synthesis) return { id: hover, text: clip(synthesis.summary, 120), tone: "chief" };
      const r = readerSpeech(hover, readers[hover]);
      if (r) return r;
      const m = resolve(hover);
      return { id: hover, text: `${m.name}${m.title ? ` · ${m.title}` : ""}${m.model ? ` · ${m.model}` : ""}`, tone: "info" };
    }
    if (synthesisRunning) return { id: chief, text: "Hãy để ta gom lời cả hội đồng…", tone: "chief" };
    if (synthesis) return { id: chief, text: clip(synthesis.summary, 120), tone: "chief" };
    if (phase === "drawing" || phase === "shuffling") {
      const next = spread.positions[reading.draws.length];
      return next ? { id: chief, text: phase === "shuffling" ? "Ta đang tráo bài…" : `Mời bạn bốc lá “${next.label_vi}”.`, tone: "chief" } : null;
    }
    const done = reading.reader_ids.map((id) => readerSpeech(id, readers[id])).filter((x): x is Speech => !!x);
    return done.find((s) => s.tone === "info") ?? done.at(-1) ?? null;
  })();
  const speaker = speech ? seats.find((s) => s.id === speech.id) : undefined;

  const caption =
    phase === "shuffling"
      ? "ĐANG TRÁO BÀI"
      : phase === "drawing"
        ? `BỐC LÁ ${reading.draws.length}/${spread.positions.length}`
        : phase === "interpreting"
          ? synthesisRunning
            ? "TRƯỞNG THẦY TỔNG HỢP"
            : "HỘI ĐỒNG ĐANG LUẬN"
          : phase === "done"
            ? "ĐÃ LUẬN XONG"
            : phase === "support"
              ? "TẠM DỪNG"
              : phase === "cancelled"
                ? "ĐÃ DỪNG"
                : "PHÒNG ĐỌC BÀI";

  const character = (s: Seat) => (
    <PixelCharacter key={s.id} meta={resolve(s.id)} x={s.x} y={s.y} status={statusOf(s)} speaking={speaker?.id === s.id} marker={markerOf(s)} />
  );

  return (
    <section
      aria-label={`Phòng đọc bài — ${spread.name_vi}`}
      className="relative shrink-0 overflow-hidden rounded-xl border border-neutral-800 bg-[#1d1a2c]"
      style={{ aspectRatio: `${W} / ${H}` }}
    >
      <svg className="px-scene absolute inset-0 size-full" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet" shapeRendering="crispEdges" aria-hidden>
        <Parlor />
        {/* Ghế hàng sau → người hàng sau → bàn (che chân) → ghế + người hàng trước → Trưởng thầy */}
        {seats.filter((s) => s.row === "top").map((s) => <ChairBack key={`c-${s.id}`} x={s.x} y={s.y} />)}
        {seats.filter((s) => s.row === "top").map(character)}
        <Table />
        <TableCards spread={spread} draws={landed} />
        {seats.filter((s) => s.row === "bottom").map((s) => <Stool key={`c-${s.id}`} x={s.x} y={s.y} />)}
        {seats.filter((s) => s.row === "bottom").map(character)}
        {seats.filter((s) => s.row === "chief").map((s) => <Throne key="throne" x={s.x} y={s.y} />)}
        {seats.filter((s) => s.row === "chief").map(character)}
      </svg>

      {/* Nhãn tên + vùng hover (HTML để chữ sắc nét) */}
      {seats.map((s) => (
        <SeatLabel key={s.id} seat={s} resolve={resolve} chief={s.row === "chief"} onHover={setHover} hovered={hover === s.id} />
      ))}

      {speech && speaker && (
        <SpeechBubble
          key={`${speech.id}-${speech.text}`}
          speech={speech}
          name={resolve(speech.id).name}
          x={speaker.x + SPRITE_W / 2}
          y={speaker.row === "top" ? speaker.y - 14 : speaker.y - 9}
        />
      )}

      <div className="px-font pointer-events-none absolute top-2 right-2 rounded-sm bg-black/55 px-2 py-1 text-[10px] text-neutral-200">
        {caption}
      </div>
      <div
        className="pointer-events-none absolute bottom-1.5 left-2 max-w-[62%] truncate rounded-sm bg-black/55 px-2 py-0.5 text-[10px] text-neutral-300"
        title={reading.question}
      >
        {spread.name_vi} · “{reading.question}”
      </div>
    </section>
  );
});

function SeatLabel({
  seat: s,
  resolve,
  chief,
  hovered,
  onHover,
}: {
  seat: Seat;
  resolve: RoleResolver;
  chief: boolean;
  hovered: boolean;
  onHover: (id: RoleId | null) => void;
}) {
  const m = resolve(s.id);
  const labelY = s.row === "top" ? s.y - 10 : s.y + 25;
  return (
    <div>
      <div
        className="absolute cursor-default"
        style={{ left: pct(s.x, W), top: pct(s.y, H), width: pct(SPRITE_W, W), height: pct(24, H) }}
        onMouseEnter={() => onHover(s.id)}
        onMouseLeave={() => onHover(null)}
      />
      <div
        className={cx(
          "px-font pointer-events-none absolute max-w-[84px] -translate-x-1/2 truncate rounded-[1px] px-1 text-[9px] leading-[13px] font-semibold text-neutral-950",
          s.row === "top" && "-translate-y-full",
          hovered && "ring-1 ring-white",
        )}
        style={{ left: pct(s.x + SPRITE_W / 2, W), top: pct(labelY, H), background: m.color }}
        title={`${m.name}${m.title ? ` — ${m.title}` : ""}`}
      >
        {chief ? `♛ ${m.name}` : m.name}
      </div>
    </div>
  );
}

const SpeechBubble = memo(function SpeechBubble({ speech, name, x, y }: { speech: Speech; name: string; x: number; y: number }) {
  // Giữ bong bóng trong khung: neo theo % và kẹp 20%–80%.
  const left = Math.min(80, Math.max(20, (x / W) * 100));
  const border = TONE_BORDER[speech.tone];
  return (
    <div className="px-bubble pointer-events-none absolute z-10 w-max max-w-[230px] -translate-x-1/2 -translate-y-full" style={{ left: `${left}%`, top: pct(y, H) }}>
      <div className="px-box px-font rounded-[1px] bg-[#fdfdf8] px-2 py-1 text-[11px] leading-snug text-neutral-900" style={{ ["--px-border" as string]: border }}>
        <span className="mr-1 font-bold" style={{ color: border }}>
          {name}:
        </span>
        <span className="line-clamp-3">{speech.text}</span>
      </div>
      <div className="mx-auto mt-[2px] h-[6px] w-[6px] bg-[#fdfdf8]" style={{ boxShadow: `0 2px 0 0 ${border}` }} />
    </div>
  );
});

/** Lá mini trên mặt bàn theo vị trí của kiểu trải (lá lớn ở panel bên phải). */
const TableCards = memo(function TableCards({ spread, draws }: { spread: Spread; draws: Draw[] }) {
  const big = spread.positions.length <= 5;
  const cw = big ? 6 : 4;
  const ch = big ? 9 : 6;
  const x0 = TABLE.x + 10;
  const x1 = TABLE.x + TABLE.w - 10;
  const y0 = TABLE.y + 2;
  const y1 = TABLE.y + TABLE.top - 2;
  return (
    <g>
      {spread.positions.map((p) => {
        const d = draws.find((x) => x.position === p.index);
        const cx0 = Math.round(x0 + p.x * (x1 - x0) - cw / 2);
        const cy0 = Math.round(y0 + p.y * (y1 - y0 - ch));
        const rot = p.rotation ? `rotate(${p.rotation} ${cx0 + cw / 2} ${cy0 + ch / 2})` : undefined;
        if (!d) {
          return <rect key={p.index} x={cx0} y={cy0} width={cw} height={ch} fill="none" stroke="#d9b26a" strokeOpacity={0.35} strokeWidth={0.5} transform={rot} />;
        }
        const suit = d.card_id.split("-")[0] ?? "major";
        const fill = SUIT_COLOR[suit] ?? SUIT_COLOR.major;
        return (
          <g key={p.index} transform={rot}>
            <rect x={cx0} y={cy0} width={cw} height={ch} fill="#f4ecd8" />
            <rect x={cx0 + 1} y={cy0 + 1} width={cw - 2} height={ch - 2} fill={fill} />
            {/* chấm chỉ chiều: trên = xuôi, dưới = ngược */}
            <rect x={cx0 + cw / 2 - 0.5} y={d.reversed ? cy0 + ch - 2 : cy0 + 1} width={1} height={1} fill="#fef3c7" />
          </g>
        );
      })}
    </g>
  );
});

/** Ghế tựa sau lưng người hàng sau (lộ ra quanh vai). */
function ChairBack({ x, y }: { x: number; y: number }) {
  return (
    <g>
      <rect x={x} y={y + 4} width={16} height={18} fill="#4a2a17" />
      <rect x={x + 1} y={y + 5} width={14} height={2} fill="#6b3d22" />
      <rect x={x - 1} y={y + 3} width={18} height={2} fill="#5b3a1f" />
    </g>
  );
}

/** Ghế đẩu dưới người hàng trước. */
function Stool({ x, y }: { x: number; y: number }) {
  return (
    <g>
      <rect x={x + 1} y={y + 18} width={14} height={3} fill="#6b3d22" />
      <rect x={x + 2} y={y + 21} width={2} height={4} fill="#4a2a17" />
      <rect x={x + 12} y={y + 21} width={2} height={4} fill="#4a2a17" />
    </g>
  );
}

/** Ghế bành cao của Trưởng thầy ở đầu bàn. */
function Throne({ x, y }: { x: number; y: number }) {
  return (
    <g>
      <rect x={x - 2} y={y - 4} width={20} height={26} fill="#581c87" />
      <rect x={x - 1} y={y - 3} width={18} height={2} fill="#d9b26a" />
      <rect x={x + 6} y={y - 7} width={4} height={3} fill="#d9b26a" />
      <rect x={x - 3} y={y + 14} width={22} height={4} fill="#3b0764" />
      <rect x={x - 2} y={y + 18} width={2} height={6} fill="#2e1065" />
      <rect x={x + 16} y={y + 18} width={2} height={6} fill="#2e1065" />
    </g>
  );
}

/** Tường, sàn, kệ sách, cửa sổ trăng, rèm, quả cầu pha lê. memo: props trống, khỏi dựng lại ô sàn. */
const Parlor = memo(function Parlor() {
  const tiles = [];
  for (let y = 48; y < H; y += 12) {
    for (let x = 0; x < W; x += 12) {
      if (((x + y) / 12) % 2 === 0) tiles.push(<rect key={`${x}-${y}`} x={x} y={y} width={12} height={12} fill="#2c2440" />);
    }
  }
  return (
    <g>
      {/* sàn + tường */}
      <rect x={0} y={48} width={W} height={H - 48} fill="#332a4b" />
      {tiles}
      <rect x={0} y={0} width={W} height={46} fill="#2a2142" />
      <rect x={0} y={44} width={W} height={4} fill="#1b1530" />
      {[20, 60, 100, 140, 180, 220].map((x) => (
        <rect key={x} x={x} y={0} width={1} height={44} fill="#251d3b" />
      ))}
      {/* thảm dưới bàn */}
      <rect x={TABLE.x - 30} y={TABLE.y + 10} width={TABLE.w + 40} height={46} fill="#5b1d3a" opacity={0.55} />
      <rect x={TABLE.x - 28} y={TABLE.y + 12} width={TABLE.w + 36} height={42} fill="none" stroke="#d9b26a" strokeOpacity={0.3} strokeWidth={1} />

      {/* kệ sách + sọ + nến nhỏ */}
      <rect x={6} y={6} width={50} height={36} fill="#3b2614" />
      <rect x={8} y={8} width={46} height={32} fill="#24160b" />
      {[9, 13, 17, 21, 26, 30, 34].map((x, i) => (
        <rect key={x} x={x} y={10 + (i % 3)} width={3} height={11 - (i % 3)} fill={["#a855f7", "#f59e0b", "#22c55e", "#ef4444", "#38bdf8", "#eab308", "#f472b6"][i]} />
      ))}
      <rect x={8} y={21} width={46} height={2} fill="#3b2614" />
      <rect x={10} y={26} width={7} height={6} fill="#e7e5e4" />
      <rect x={11} y={28} width={2} height={2} fill="#1c1917" />
      <rect x={14} y={28} width={2} height={2} fill="#1c1917" />
      <rect x={24} y={27} width={10} height={5} fill="#c084fc" />
      <rect x={40} y={29} width={3} height={6} fill="#fef3c7" />
      <rect x={40} y={26} width={3} height={3} fill="#fbbf24" className="px-flicker" />
      <rect x={8} y={36} width={46} height={2} fill="#3b2614" />

      {/* cửa sổ đêm + trăng lưỡi liềm + sao */}
      <rect x={176} y={6} width={54} height={28} fill="#3b2f63" />
      <rect x={178} y={8} width={50} height={24} fill="#0f1230" />
      {[
        [184, 12],
        [194, 22],
        [206, 11],
        [221, 25],
        [188, 27],
      ].map(([x, y]) => (
        <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} fill="#f8fafc" className="px-dot" />
      ))}
      <rect x={212} y={12} width={8} height={8} fill="#fef3c7" />
      <rect x={215} y={12} width={5} height={6} fill="#0f1230" />
      <rect x={202} y={8} width={2} height={24} fill="#3b2f63" />
      <rect x={178} y={19} width={50} height={2} fill="#3b2f63" />
      <rect x={172} y={4} width={5} height={34} fill="#7e22ce" />
      <rect x={229} y={4} width={5} height={34} fill="#7e22ce" />

      {/* chuông gió + vòng sao treo */}
      <rect x={120} y={0} width={1} height={10} fill="#a8a29e" />
      <rect x={117} y={10} width={7} height={1} fill="#d9b26a" />
      {[117, 120, 123].map((x, i) => (
        <rect key={x} x={x} y={11 + i} width={1} height={4} fill="#fde68a" />
      ))}

      {/* tủ nhỏ + quả cầu pha lê góc phải */}
      <rect x={206} y={104} width={26} height={22} fill="#4a2a17" />
      <rect x={206} y={104} width={26} height={2} fill="#6b3d22" />
      <rect x={213} y={96} width={12} height={8} fill="#67e8f9" opacity={0.75} />
      <rect x={215} y={98} width={3} height={2} fill="#ecfeff" />
      <rect x={212} y={103} width={14} height={2} fill="#78350f" />
    </g>
  );
});

/** Bàn khăn tím có tua vàng + 2 nến (ngọn lửa nhấp nháy theo `px-blink`). */
const Table = memo(function Table() {
  const { x, y, w, top, skirt } = TABLE;
  const candle = (cx: number) => (
    <g key={cx}>
      <rect x={cx} y={y - 6} width={3} height={7} fill="#fef3c7" />
      <rect x={cx} y={y - 10} width={3} height={4} fill="#fbbf24" className="px-flicker" />
      <rect x={cx + 1} y={y - 9} width={1} height={2} fill="#fff7ed" />
      <rect x={cx - 2} y={y} width={7} height={1} fill="#78350f" />
    </g>
  );
  return (
    <g>
      <rect x={x + 2} y={y + top + skirt} width={w - 4} height={2} fill="#000" opacity={0.3} />
      {/* mặt bàn */}
      <rect x={x} y={y} width={w} height={top} fill="#5b21b6" />
      <rect x={x} y={y} width={w} height={1} fill="#7c3aed" />
      <rect x={x + 3} y={y + 2} width={w - 6} height={top - 4} fill="none" stroke="#d9b26a" strokeOpacity={0.35} strokeWidth={0.6} />
      {/* khăn rủ phía trước + tua */}
      <rect x={x} y={y + top} width={w} height={skirt} fill="#4c1d95" />
      <rect x={x} y={y + top} width={w} height={1} fill="#3b0764" />
      {Array.from({ length: Math.floor(w / 5) }, (_, i) => (
        <rect key={i} x={x + 2 + i * 5} y={y + top + skirt} width={1} height={2} fill="#d9b26a" />
      ))}
      {[x + 3, x + w - 6].map(candle)}
    </g>
  );
});
