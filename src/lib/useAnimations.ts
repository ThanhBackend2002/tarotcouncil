import { useConfig } from "../store/config";
import { useUi } from "../store/ui";

/** Hiệu ứng tráo/lật: tuỳ chọn "Tắt hiệu ứng" của tarot hoặc `prefers-reduced-motion` (plan §3). */
export function useAnimationsOn(): boolean {
  const pref = useConfig((s) => s.config?.tarot.animations ?? "system");
  const reduced = useUi((s) => s.motionReduced);
  return pref === "on" || (pref === "system" && !reduced);
}
