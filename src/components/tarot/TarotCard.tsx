import { memo } from "react";
import { motion } from "motion/react";
import type { Draw } from "../../lib/contracts";
import { CARD_ASPECT, cardImage } from "../../lib/cardArt";
import { cx } from "../ui";

/** Mặt lưng — SVG vẽ tay, không dùng asset ngoài. */
export const CardBack = memo(function CardBack() {
  return (
    <svg viewBox="0 0 35 60" className="size-full" aria-hidden shapeRendering="crispEdges">
      <rect width="35" height="60" rx="2" fill="#1e1846" />
      <rect x="1.5" y="1.5" width="32" height="57" rx="1.5" fill="none" stroke="#d9b26a" strokeWidth="0.8" />
      <rect x="3" y="3" width="29" height="54" rx="1" fill="none" stroke="#d9b26a" strokeOpacity="0.4" strokeWidth="0.5" />
      {Array.from({ length: 6 }, (_, i) =>
        Array.from({ length: 10 }, (_, j) => (
          <rect key={`${i}-${j}`} x={5 + i * 5} y={5 + j * 5.2} width="0.8" height="0.8" fill="#d9b26a" opacity="0.35" />
        )),
      )}
      <circle cx="17.5" cy="30" r="7" fill="none" stroke="#ecd39a" strokeWidth="0.8" />
      <path d="M19 25 A5.5 5.5 0 1 0 19 35 A4.3 4.3 0 1 1 19 25 Z" fill="#ecd39a" />
    </svg>
  );
});

export function drawLabel(d: Draw): string {
  return `${d.position_label_vi}: ${d.name_vi} (${d.name_en}), ${d.reversed ? "chiều ngược" : "chiều xuôi"}`;
}

/** Lá bài lật bằng rotateY (~500 ms); lá ngược xoay 180°. `draw` null = lá úp. */
export function TarotCard({
  draw,
  revealed,
  width,
  animate,
  rotation = 0,
  selected,
  onClick,
  className,
}: {
  draw: Draw | null;
  revealed: boolean;
  width: number | string;
  animate: boolean;
  rotation?: number;
  selected?: boolean;
  onClick?: () => void;
  className?: string;
}) {
  const src = draw ? cardImage(draw.art_key) : undefined;
  const label = draw && revealed ? drawLabel(draw) : "Lá bài úp";
  const Inner = (
    <motion.div
      className="relative size-full"
      style={{ transformStyle: "preserve-3d" }}
      initial={false}
      animate={{ rotateY: revealed ? 180 : 0 }}
      transition={animate ? { duration: 0.5, ease: "easeInOut" } : { duration: 0 }}
    >
      <div className="absolute inset-0 overflow-hidden rounded-[6%] shadow-lg shadow-black/50 [backface-visibility:hidden]">
        <CardBack />
      </div>
      <div
        className="absolute inset-0 overflow-hidden rounded-[6%] bg-[#f4ecd8] shadow-lg shadow-black/50 [backface-visibility:hidden]"
        style={{ transform: "rotateY(180deg)" }}
      >
        {src && <img src={src} alt="" draggable={false} className={cx("size-full object-cover", draw?.reversed && "rotate-180")} />}
      </div>
    </motion.div>
  );
  const style = { width, aspectRatio: `${CARD_ASPECT}`, perspective: 900, rotate: `${rotation}deg` };
  const ring = selected ? "ring-2 ring-amber-300 ring-offset-2 ring-offset-transparent rounded-[6%]" : "";
  return onClick ? (
    <button type="button" onClick={onClick} aria-label={label} aria-pressed={selected} className={cx("block", ring, className)} style={style}>
      {Inner}
    </button>
  ) : (
    <div role="img" aria-label={label} className={cx(ring, className)} style={style}>
      {Inner}
    </div>
  );
}
