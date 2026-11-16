import type { AppConfig, Animations, RoleId, TarotSettings } from "../../lib/contracts";
import { Panel, cx } from "../ui";
import { Field, INPUT, NumberInput, SectionTitle, Toggle, type Errors } from "./fields";

/** Kiểu trải có trong app (id khớp `spreads.rs`). Celtic Cross đến ở M3. */
export const SPREAD_OPTIONS: { id: string; label: string }[] = [
  { id: "one", label: "1 lá — Thông điệp trong ngày" },
  { id: "three", label: "3 lá — Quá khứ · Hiện tại · Tương lai" },
  { id: "love5", label: "5 lá — Tình cảm" },
  { id: "career5", label: "5 lá — Công việc" },
];

const MAX_READERS = 4;

export function TarotTab({
  draft,
  errors,
  update,
}: {
  draft: AppConfig;
  errors: Errors;
  update: (f: (c: AppConfig) => AppConfig) => void;
}) {
  const t = draft.tarot;
  const set = (patch: Partial<TarotSettings>) => update((c) => ({ ...c, tarot: { ...c.tarot, ...patch } }));
  const toggleReader = (id: RoleId) =>
    set({
      default_readers: t.default_readers.includes(id)
        ? t.default_readers.filter((r) => r !== id)
        : [...t.default_readers, id].slice(0, MAX_READERS),
    });

  return (
    <div className="space-y-4">
      <Panel className="p-4">
        <SectionTitle title="Lượt đọc mặc định" desc="Có thể đổi lại ở từng lượt đọc." />
        <div className="grid grid-cols-2 gap-4">
          <Field label="Kiểu trải mặc định">
            <select value={t.default_spread} onChange={(e) => set({ default_spread: e.target.value })} className={INPUT}>
              {SPREAD_OPTIONS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Bài ngược" hint="Tắt = mọi lá đều xuôi. Giá trị được lưu vào từng lượt đọc.">
            <Toggle checked={t.reversed_enabled} onChange={(v) => set({ reversed_enabled: v })} label="Cho phép lá ngược" />
          </Field>
          <Field label="Trưởng thầy (tổng hợp)" error={errors["tarot.chief"]}>
            <select value={t.chief_reader_id} onChange={(e) => set({ chief_reader_id: e.target.value })} className={INPUT}>
              {draft.roles.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                </option>
              ))}
            </select>
          </Field>
          <Field
            label="Giới hạn token mỗi lượt"
            error={errors["tarot.max_total_tokens"]}
            hint="Giới hạn MỀM — chi phí thật có thể vượt khi chạy song song hoặc retry. Trống = không giới hạn."
          >
            <NumberInput value={t.max_total_tokens} nullable onChange={(v) => set({ max_total_tokens: v })} />
          </Field>
        </div>
        <div className="mt-4">
          <Field label={`Thầy bói mặc định (${t.default_readers.length}/${MAX_READERS})`} error={errors["tarot.readers"]}>
            <div className="flex flex-wrap gap-2" role="group" aria-label="Thầy bói mặc định">
              {draft.roles
                .filter((r) => r.id !== t.chief_reader_id)
                .map((r) => {
                  const on = t.default_readers.includes(r.id);
                  const full = !on && t.default_readers.length >= MAX_READERS;
                  return (
                    <button
                      key={r.id}
                      type="button"
                      aria-pressed={on}
                      disabled={full}
                      onClick={() => toggleReader(r.id)}
                      className={cx(
                        "rounded-full px-3 py-1 text-xs ring-1 transition-colors disabled:opacity-40",
                        on ? "bg-accent-subtle text-accent-fg ring-accent" : "text-fg-muted ring-line-strong hover:text-fg-soft",
                      )}
                    >
                      {r.name}
                    </button>
                  );
                })}
            </div>
          </Field>
        </div>
      </Panel>

      <Panel className="p-4">
        <SectionTitle title="Hiệu ứng" />
        <Field label="Hiệu ứng tráo và lật bài" hint="“Theo hệ thống” tôn trọng tuỳ chọn giảm chuyển động của hệ điều hành.">
          <select
            value={t.animations}
            onChange={(e) => set({ animations: e.target.value as Animations })}
            className={cx(INPUT, "max-w-xs")}
          >
            <option value="system">Theo hệ thống</option>
            <option value="on">Bật</option>
            <option value="off">Tắt hiệu ứng</option>
          </select>
        </Field>
      </Panel>

      <Panel className="p-4">
        <SectionTitle title="Quyền riêng tư" />
        <p className="text-sm text-fg-muted">
          {t.privacy_ack
            ? "Bạn đã đồng ý: câu hỏi và các lá đã bốc được gửi tới provider bạn cấu hình; lịch sử lưu cục bộ, không mã hoá."
            : "Lần luận giải đầu tiên, app sẽ hỏi bạn đồng ý gửi câu hỏi và các lá đã bốc tới provider."}
        </p>
      </Panel>
    </div>
  );
}
