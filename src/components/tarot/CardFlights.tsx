import { useMemo } from "react";
import { createPortal } from "react-dom";
import { motion } from "motion/react";
import { slotBox, useFlight, type Flight } from "../../store/flight";
import { CardBack } from "./TarotCard";

const FLIGHT_S = 0.6;

function FlyingCard({ f }: { f: Flight }) {
  const land = useFlight((s) => s.land);
  // Đích đo 1 lần lúc cất cánh (ô trống đã có sẵn trên bàn trải).
  const to = useMemo(() => slotBox(f.position) ?? f.from, [f]);
  const lift = Math.min(f.from.cy, to.cy) - 80; // bay theo vòng cung lên trên
  return (
    <motion.div
      className="pointer-events-none fixed top-0 left-0 z-50 drop-shadow-[0_10px_18px_rgba(0,0,0,0.55)]"
      style={{ aspectRatio: "35 / 60" }}
      initial={{ x: f.from.cx - f.from.w / 2, y: f.from.cy - (f.from.w * 60) / 35 / 2, width: f.from.w, rotate: f.from.rot }}
      animate={{
        x: [f.from.cx - f.from.w / 2, (f.from.cx + to.cx) / 2 - to.w / 2, to.cx - to.w / 2],
        y: [f.from.cy - (f.from.w * 60) / 35 / 2, lift, to.cy - (to.w * 60) / 35 / 2],
        width: [f.from.w, (f.from.w + to.w) / 2 * 1.15, to.w],
        rotate: [f.from.rot, (f.from.rot + to.rot) / 2 - 8, to.rot],
      }}
      transition={{ duration: FLIGHT_S, ease: [0.3, 0.7, 0.3, 1], times: [0, 0.45, 1] }}
      onAnimationComplete={() => land(f.id)}
    >
      <div className="size-full overflow-hidden rounded-[6%]">
        <CardBack />
      </div>
    </motion.div>
  );
}

/** Lớp phủ cố định cho các lá đang bay — không bị cắt bởi vùng cuộn của cột trái / phải. */
export function CardFlights() {
  const flights = useFlight((s) => s.flights);
  if (!flights.length) return null;
  return createPortal(
    <>
      {flights.map((f) => (
        <FlyingCard key={f.id} f={f} />
      ))}
    </>,
    document.body,
  );
}
