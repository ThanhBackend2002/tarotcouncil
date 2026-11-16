import { memo, useMemo } from "react";
import { makeSprite, SPRITE_W, type Rect } from "../lib/pixel";
import type { RoleMeta } from "../lib/agents";
/** Trạng thái hoạt hình của nhân vật: đứng yên / đang suy ngẫm / xong / lỗi. */
export type AgentStatus = "idle" | "thinking" | "done" | "error";

function Rects({ rects }: { rects: Rect[] }) {
  return (
    <>
      {rects.map((r, i) => (
        <rect key={i} x={r.x} y={r.y} width={r.w} height={r.h} fill={r.fill} />
      ))}
    </>
  );
}

const delayOf = (id: string) => {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return `${-(h % 1200)}ms`;
};

export type Marker = "approve" | "reject" | "error" | null;

/** Nhân vật trong cảnh SVG. (x, y) = góc trên-trái sprite, đơn vị pixel logic của cảnh. */
export const PixelCharacter = memo(function PixelCharacter({
  meta,
  x,
  y,
  status,
  speaking,
  marker,
  dimmed,
}: {
  meta: RoleMeta;
  x: number;
  y: number;
  status: AgentStatus;
  speaking: boolean;
  marker: Marker;
  dimmed?: boolean;
}) {
  const sprite = useMemo(
    () => makeSprite({ id: meta.id, color: meta.color, level: meta.level, icon: meta.iconKey, appearance: meta.appearance }),
    [meta.id, meta.color, meta.level, meta.iconKey, meta.appearance],
  );
  const thinking = status === "thinking";
  const delay = delayOf(meta.id);

  return (
    <g transform={`translate(${x} ${y})`} opacity={dimmed ? 0.55 : 1}>
      {/* bóng dưới chân */}
      <rect x={3} y={23} width={10} height={1} fill="#000" opacity={0.25} />
      <g className={thinking ? "px-bob-fast" : "px-bob"} style={{ animationDelay: delay }}>
        <Rects rects={sprite.body} />
        <g className="px-blink" style={{ animationDelay: delay }}>
          <Rects rects={sprite.blink} />
        </g>
        {speaking && (
          <g className="px-talk">
            <Rects rects={sprite.mouthOpen} />
          </g>
        )}
        {thinking && <ThinkingCloud />}
        {!thinking && marker && <MarkerIcon marker={marker} />}
      </g>
    </g>
  );
});

/** Đám mây "..." phía trên đầu khi đang suy nghĩ. */
function ThinkingCloud() {
  return (
    <g transform={`translate(${SPRITE_W / 2 - 6} -9)`}>
      <rect x={1} y={0} width={10} height={6} fill="#f8fafc" />
      <rect x={0} y={1} width={12} height={4} fill="#f8fafc" />
      <rect x={4} y={6} width={2} height={1} fill="#f8fafc" />
      <rect x={3} y={8} width={1} height={1} fill="#f8fafc" />
      {[0, 1, 2].map((i) => (
        <rect key={i} className="px-dot" style={{ animationDelay: `${i * 180}ms` }} x={2 + i * 3} y={2} width={2} height={2} fill="#6d28d9" />
      ))}
    </g>
  );
}

const CHECK = ["....G", "...GG", "G.GG.", "GGG..", ".G..."];
const CROSS = ["R...R", ".R.R.", "..R..", ".R.R.", "R...R"];
const BANG = ["..A..", "..A..", "..A..", ".....", "..A.."];

function MarkerIcon({ marker }: { marker: Exclude<Marker, null> }) {
  const [tpl, color] =
    marker === "approve" ? [CHECK, "#22c55e"] : marker === "reject" ? [CROSS, "#ef4444"] : [BANG, "#f59e0b"];
  return (
    <g transform={`translate(${SPRITE_W / 2 - 3.5} -8)`}>
      <rect x={-1} y={-1} width={7} height={7} fill="#0b0b12" opacity={0.75} />
      {tpl.flatMap((row, yy) =>
        [...row].map((ch, xx) => (ch === "." ? null : <rect key={`${xx}-${yy}`} x={xx} y={yy} width={1} height={1} fill={color} />)),
      )}
    </g>
  );
}

/** Chân dung pixel (đầu + vai) dùng trong khung hội thoại. */
export const PixelPortrait = memo(function PixelPortrait({
  meta,
  size = 28,
  full,
}: {
  meta: RoleMeta;
  size?: number;
  /** Toàn thân (16×24) thay vì chỉ đầu + vai. `size` = chiều rộng. */
  full?: boolean;
}) {
  const sprite = useMemo(
    () => makeSprite({ id: meta.id, color: meta.color, level: meta.level, icon: meta.iconKey, appearance: meta.appearance }),
    [meta.id, meta.color, meta.level, meta.iconKey, meta.appearance],
  );
  return (
    <svg
      width={size}
      height={full ? size * 1.5 : size}
      viewBox={full ? "0 0 16 24" : "0 0 16 16"}
      shapeRendering="crispEdges"
      className="shrink-0 rounded-sm"
      style={{ background: full ? undefined : `${meta.color}26`, imageRendering: "pixelated" }}
      aria-hidden
    >
      <Rects rects={full ? sprite.body : sprite.body.filter((r) => r.y < 16)} />
    </svg>
  );
});
