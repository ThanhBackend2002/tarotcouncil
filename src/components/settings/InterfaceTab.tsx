// Cài đặt → Giao diện: theme + giảm chuyển động. Lưu ngay vào localStorage của máy này
// (không thuộc AppConfig nên không cần bấm "Lưu").

import type { ReactNode } from "react";
import { Monitor, Moon, Sun, Sparkles, Pause, Play } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useUi, type MotionPref, type ThemePref } from "../../store/ui";
import { Panel, cx } from "../ui";
import { SectionTitle } from "./fields";

interface Choice<T extends string> {
  id: T;
  label: string;
  desc: string;
  icon: ReactNode;
}

const THEMES: Choice<ThemePref>[] = [
  { id: "system", label: "Theo hệ thống", desc: "Tự đổi sáng / tối theo cài đặt của hệ điều hành", icon: <Monitor className="size-4" /> },
  { id: "dark", label: "Tối", desc: "Nền tối, như mặc định trước đây", icon: <Moon className="size-4" /> },
  { id: "light", label: "Sáng", desc: "Nền sáng, dễ đọc dưới ánh sáng mạnh", icon: <Sun className="size-4" /> },
];

const MOTIONS: Choice<MotionPref>[] = [
  { id: "system", label: "Theo hệ thống", desc: "Giảm chuyển động nếu hệ điều hành đang bật tuỳ chọn này", icon: <Monitor className="size-4" /> },
  { id: "on", label: "Luôn giảm", desc: "Tắt hiệu ứng chuyển cảnh và hoạt ảnh nhân vật", icon: <Pause className="size-4" /> },
  { id: "off", label: "Đầy đủ", desc: "Luôn bật hiệu ứng, kể cả khi hệ điều hành yêu cầu giảm", icon: <Play className="size-4" /> },
];

function RadioCards<T extends string>({
  name,
  legend,
  choices,
  value,
  onChange,
}: {
  name: string;
  legend: string;
  choices: Choice<T>[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <fieldset>
      <legend className="sr-only">{legend}</legend>
      <div className="grid grid-cols-3 gap-2">
        {choices.map((c) => {
          const on = value === c.id;
          return (
            <label
              key={c.id}
              className={cx(
                "flex cursor-pointer items-start gap-2.5 rounded-lg border px-3 py-2 transition-colors",
                // Radio thật bị ẩn nên focus-visible hiển thị trên cả thẻ.
                "has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-accent",
                on ? "border-accent/60 bg-accent-subtle" : "border-line hover:border-line-heavy",
              )}
            >
              <input
                type="radio"
                name={name}
                value={c.id}
                checked={on}
                onChange={() => onChange(c.id)}
                className="sr-only"
              />
              <span className={cx("mt-0.5 shrink-0", on ? "text-accent-fg" : "text-fg-subtle")} aria-hidden>
                {c.icon}
              </span>
              <span className="min-w-0">
                <span className="block text-sm font-medium text-fg">{c.label}</span>
                <span className="block text-[11px] text-fg-subtle">{c.desc}</span>
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}

export function InterfaceTab() {
  const { theme, setTheme, resolvedTheme, reducedMotion, setReducedMotion, motionReduced } = useUi(
    useShallow((s) => ({
      theme: s.theme,
      setTheme: s.setTheme,
      resolvedTheme: s.resolvedTheme,
      reducedMotion: s.reducedMotion,
      setReducedMotion: s.setReducedMotion,
      motionReduced: s.motionReduced,
    })),
  );

  return (
    <div className="space-y-4">
      <Panel className="p-4">
        <SectionTitle title="Giao diện" desc="Áp dụng ngay và lưu trên máy này, không cần bấm Lưu." />
        <RadioCards name="theme" legend="Theme" choices={THEMES} value={theme} onChange={setTheme} />
        <p className="mt-2 text-[11px] text-fg-subtle">
          Đang dùng: <b className="text-fg-soft">{resolvedTheme === "dark" ? "Tối" : "Sáng"}</b>. Phòng đọc bài pixel luôn giữ nền tối riêng.
        </p>
      </Panel>

      <Panel className="p-4">
        <SectionTitle title="Chuyển động" desc="Giảm hoạt ảnh cho người nhạy cảm với chuyển động hoặc máy yếu." />
        <RadioCards name="motion" legend="Giảm chuyển động" choices={MOTIONS} value={reducedMotion} onChange={setReducedMotion} />
        <p className="mt-2 flex items-center gap-1.5 text-[11px] text-fg-subtle">
          <Sparkles className="size-3" aria-hidden />
          Hiện tại: <b className="text-fg-soft">{motionReduced ? "đang giảm chuyển động" : "hiệu ứng đầy đủ"}</b>
        </p>
      </Panel>

    </div>
  );
}
