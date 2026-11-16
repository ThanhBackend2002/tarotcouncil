import { useMemo, useState } from "react";
import { Activity, ChevronDown, ChevronRight, CircleCheck, CircleX, CopyPlus, Plus, Search, Shirt, Trash, WandSparkles } from "lucide-react";
import { toast, toastError } from "../../store/toast";
import { LEVELS, LEVEL_LABEL, ROLE_ICONS, makeResolver } from "../../lib/agents";
import type { AppConfig, RoleDef, RoleId, RoleLevel } from "../../lib/contracts";
import { HEALTH_META, isChecking, useHealth } from "../../store/health";
import { HealthBadge } from "../HealthBadge";
import { ipc } from "../../lib/ipc";
import { PixelPortrait } from "../PixelCharacter";
import { Button, Panel, cx } from "../ui";
import { AppearanceEditor, EMPTY_LOOK } from "./AppearanceEditor";
import { Field, INPUT, ModelPicker, NumberInput, SectionTitle, TextArea, TextInput, newId, type Errors } from "./fields";

const PALETTE = ["#a78bfa", "#38bdf8", "#f472b6", "#fbbf24", "#34d399", "#fb923c", "#f43f5e", "#22d3ee", "#a3e635", "#e879f9"];

/** Thầy bói đang được dùng ở đâu (chặn xoá + hiển thị badge). */
export function usageOf(c: AppConfig, id: RoleId): string[] {
  const out: string[] = [];
  if (c.tarot.chief_reader_id === id) out.push("Trưởng thầy");
  if (c.tarot.default_readers.includes(id)) out.push("Thầy bói mặc định");
  return out;
}

export function RolesTab({
  draft,
  errors,
  dirty,
  update,
}: {
  draft: AppConfig;
  errors: Errors;
  dirty: boolean;
  update: (f: (c: AppConfig) => AppConfig) => void;
}) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<Set<RoleId>>(new Set());
  const q = query.trim().toLowerCase();
  const visible = draft.roles.filter(
    (r) => !q || `${r.name} ${r.title} ${r.description} ${r.model}`.toLowerCase().includes(q),
  );

  const setRole = (id: RoleId, patch: Partial<RoleDef>) =>
    update((c) => ({ ...c, roles: c.roles.map((r) => (r.id === id ? { ...r, ...patch } : r)) }));
  const toggle = (id: RoleId) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const add = (template?: RoleDef) => {
    const p = draft.providers[0];
    const base: RoleDef = template
      ? { ...template, name: `${template.name} (copy)` }
      : {
          id: "",
          name: "Thầy bói mới",
          title: "",
          level: "engineer",
          icon: "user",
          color: PALETTE[draft.roles.length % PALETTE.length]!,
          description: "",
          instructions: "",
          provider_id: p?.id ?? "default",
          model: draft.roles.find((r) => r.id === draft.tarot.chief_reader_id)?.model ?? p?.models[0] ?? "",
          temperature: 0.7,
          appearance: EMPTY_LOOK,
        };
    const role = { ...base, id: newId("role") };
    update((c) => ({ ...c, roles: [...c.roles, role] }));
    setOpen((s) => new Set(s).add(role.id));
    setQuery("");
  };

  const [seeding, setSeeding] = useState(false);
  /** Role có ngoại hình trống + trùng id với danh mục mặc định → áp ngoại hình mẫu. */
  const applySeedLooks = async () => {
    setSeeding(true);
    try {
      const defaults = new Map((await ipc.defaultRoles()).map((r) => [r.id, r.appearance]));
      const isEmpty = (a: RoleDef["appearance"] | undefined) => !a || Object.values(a).every((v) => !v);
      const targets = new Set(draft.roles.filter((r) => defaults.has(r.id) && isEmpty(r.appearance)).map((r) => r.id));
      const n = targets.size;
      update((c) => ({
        ...c,
        roles: c.roles.map((r) => (targets.has(r.id) && isEmpty(r.appearance) ? { ...r, appearance: defaults.get(r.id)! } : r)),
      }));
      toast(n ? "success" : "info", n ? `Đã áp ngoại hình mẫu cho ${n} role` : "Không có role nào cần áp (đã có ngoại hình riêng)", n ? "Bấm Lưu để giữ thay đổi." : undefined);
    } catch (e) {
      toastError(e);
    } finally {
      setSeeding(false);
    }
  };

  const providerIds = new Set(draft.providers.map((p) => p.id));
  const unassigned = draft.roles.filter((r) => !providerIds.has(r.provider_id) || !r.model.trim()).length;

  return (
    <div className="space-y-4">
      {unassigned > 0 && (
        <p className="rounded-control border border-warn-line bg-warn-subtle px-3 py-2 text-xs text-warn">
          {unassigned} thầy bói chưa chọn provider/model. Dùng <b>Gán model hàng loạt</b> bên dưới để gán nhanh. Thầy bói chưa gán vẫn
          lưu được, nhưng không thể tham gia lượt đọc.
        </p>
      )}
      <BulkAssign draft={draft} visible={visible} update={update} />
      <Panel className="p-4">
        <SectionTitle
          title={`Thầy bói (${draft.roles.length})`}
          desc="Thêm/bớt tự do. Mỗi thầy bói có lối luận bài (persona) và model riêng."
        >
          <div className="relative w-56">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-subtle" />
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Tìm role…" className={cx(INPUT, "h-8 pl-8")} />
          </div>
          <CheckAllButton draft={draft} dirty={dirty} />
          <Button
            size="sm"
            icon={<Shirt className="size-3.5" />}
            loading={seeding}
            onClick={() => void applySeedLooks()}
            title="Role trong danh mục mặc định mà chưa chỉnh ngoại hình sẽ nhận ngoại hình mẫu (giới tính, tóc, trang phục…)"
          >
            Áp ngoại hình mẫu
          </Button>
          <Button size="sm" variant="primary" icon={<Plus className="size-3.5" />} onClick={() => add()}>
            Thêm role
          </Button>
        </SectionTitle>
        <div className="space-y-4">
          {LEVELS.map((lv) => {
            const list = visible.filter((r) => r.level === lv.id);
            if (list.length === 0) return null;
            return (
              <div key={lv.id}>
                <div className="mb-1.5 text-[11px] font-semibold tracking-wide text-fg-subtle uppercase">
                  {lv.label} · {list.length}
                </div>
                <div className="space-y-1.5">
                  {list.map((r) => (
                    <RoleRow
                      key={r.id}
                      role={r}
                      draft={draft}
                      errors={errors}
                      dirty={dirty}
                      open={open.has(r.id)}
                      onToggle={() => toggle(r.id)}
                      onChange={(patch) => setRole(r.id, patch)}
                      onDuplicate={() => add(r)}
                      onRemove={() => update((c) => ({ ...c, roles: c.roles.filter((x) => x.id !== r.id) }))}
                    />
                  ))}
                </div>
              </div>
            );
          })}
          {visible.length === 0 && <p className="py-6 text-center text-sm text-fg-subtle">Không có role nào khớp.</p>}
        </div>
      </Panel>
    </div>
  );
}

function BulkAssign({
  draft,
  visible,
  update,
}: {
  draft: AppConfig;
  visible: RoleDef[];
  update: (f: (c: AppConfig) => AppConfig) => void;
}) {
  const [providerId, setProviderId] = useState(draft.providers[0]?.id ?? "");
  const [model, setModel] = useState("");
  const [target, setTarget] = useState<"all" | "visible" | RoleLevel>("all");
  const provider = draft.providers.find((p) => p.id === providerId);
  const count =
    target === "all" ? draft.roles.length : target === "visible" ? visible.length : draft.roles.filter((r) => r.level === target).length;

  const apply = () => {
    const ids = new Set(
      (target === "all" ? draft.roles : target === "visible" ? visible : draft.roles.filter((r) => r.level === target)).map((r) => r.id),
    );
    update((c) => ({
      ...c,
      roles: c.roles.map((r) => (ids.has(r.id) ? { ...r, provider_id: providerId, model: model.trim() } : r)),
    }));
  };

  return (
    <Panel className="p-4">
      <SectionTitle title="Gán model hàng loạt" desc="Chọn provider + model rồi áp cho nhiều role cùng lúc (vd. model rẻ cho Staff, model mạnh cho C-Level)." />
      <div className="grid grid-cols-[1fr_1.6fr_1fr_auto] items-end gap-3">
        <Field label="Provider">
          <select value={providerId} onChange={(e) => setProviderId(e.target.value)} className={INPUT}>
            {draft.providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name || "(chưa đặt tên)"}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Model">
          <ModelPicker value={model} models={provider?.models ?? []} onChange={setModel} />
        </Field>
        <Field label="Áp cho">
          <select value={target} onChange={(e) => setTarget(e.target.value as typeof target)} className={INPUT}>
            <option value="all">Tất cả role</option>
            <option value="visible">Role đang lọc</option>
            {LEVELS.map((l) => (
              <option key={l.id} value={l.id}>
                Cấp {l.label}
              </option>
            ))}
          </select>
        </Field>
        <Button icon={<WandSparkles className="size-3.5" />} disabled={!providerId || !model.trim() || count === 0} onClick={apply}>
          Áp dụng ({count})
        </Button>
      </div>
    </Panel>
  );
}

function RoleRow({
  role: r,
  draft,
  errors,
  dirty,
  open,
  onToggle,
  onChange,
  onDuplicate,
  onRemove,
}: {
  role: RoleDef;
  draft: AppConfig;
  errors: Errors;
  dirty: boolean;
  open: boolean;
  onToggle: () => void;
  onChange: (patch: Partial<RoleDef>) => void;
  onDuplicate: () => void;
  onRemove: () => void;
}) {
  const meta = useMemo(() => makeResolver([], [r])(r.id), [r]);
  const usage = usageOf(draft, r.id);
  const provider = draft.providers.find((p) => p.id === r.provider_id);
  const hasError = Object.keys(errors).some((k) => k.startsWith(`role.${r.id}.`));
  const health = useHealth((s) => s.byRole[r.id]);
  const checking = useHealth((s) => isChecking(s.checking, r.id));
  const check = useHealth((s) => s.check);
  // Đã đổi provider/model (chưa lưu) → kết quả cũ không còn đúng.
  const fresh = health && health.provider_id === r.provider_id && health.model === r.model.trim() ? health : undefined;

  const changeProvider = (id: string) => {
    const next = draft.providers.find((p) => p.id === id);
    const keep = !next || next.models.length === 0 || next.models.includes(r.model);
    onChange({ provider_id: id, model: keep ? r.model : "" });
  };

  return (
    <div className={cx("rounded-lg border bg-canvas/40", hasError ? "border-danger-solid/40" : "border-line")}>
      <button type="button" onClick={onToggle} aria-expanded={open} className="flex w-full items-center gap-2.5 px-3 py-2 text-left hover:bg-surface-2/50">
        {open ? <ChevronDown className="size-4 text-fg-subtle" /> : <ChevronRight className="size-4 text-fg-subtle" />}
        <PixelPortrait meta={meta} size={28} />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span className="text-sm font-medium" style={{ color: r.color }}>
              {r.name || "(chưa đặt tên)"}
            </span>
            {r.title && r.title !== r.name && <span className="truncate text-[11px] text-fg-subtle">{r.title}</span>}
          </span>
          <span className="block truncate text-[11px] text-fg-subtle">{r.description}</span>
        </span>
        <span className="hidden max-w-[220px] shrink-0 truncate font-mono text-[11px] text-fg-muted lg:block" title={`${provider?.name ?? "?"} · ${r.model}`}>
          {!provider ? <span className="text-warn">chưa chọn provider</span> : r.model || <span className="text-warn">chưa chọn model</span>}
        </span>
        <HealthBadge health={fresh} checking={checking} />
        <span className="flex max-w-[240px] shrink-0 flex-wrap justify-end gap-1">
          {usage.slice(0, 2).map((u) => (
            <span key={u} className="rounded-full bg-accent-subtle px-1.5 py-px text-[10px] text-accent-fg">
              {u}
            </span>
          ))}
          {usage.length > 2 && <span className="text-[10px] text-fg-subtle">+{usage.length - 2}</span>}
        </span>
      </button>

      {open && (
        <div className="space-y-3 border-t border-line p-3">
          <div className="grid grid-cols-[1fr_1.4fr_1fr] gap-3">
            <Field label="Tên hiển thị" error={errors[`role.${r.id}.name`]}>
              <TextInput value={r.name} onChange={(v) => onChange({ name: v })} placeholder="VD: CTO" />
            </Field>
            <Field label="Chức danh đầy đủ">
              <TextInput value={r.title} onChange={(v) => onChange({ title: v })} placeholder="VD: Chief Technology Officer" />
            </Field>
            <Field label="Cấp bậc" hint="Quyết định trang phục pixel">
              <select value={r.level} onChange={(e) => onChange({ level: e.target.value as RoleLevel })} className={INPUT}>
                {LEVELS.map((l) => (
                  <option key={l.id} value={l.id}>
                    {LEVEL_LABEL[l.id]}
                  </option>
                ))}
              </select>
            </Field>
          </div>

          <div className="grid grid-cols-[auto_1fr] gap-3">
            <Field label="Màu role">
              <input
                type="color"
                value={/^#[0-9a-f]{6}$/i.test(r.color) ? r.color : "#a1a1aa"}
                onChange={(e) => onChange({ color: e.target.value })}
                className="h-9 w-12 cursor-pointer rounded border border-line-strong bg-canvas"
                aria-label="Màu role"
              />
            </Field>
            <Field label="Icon">
              <div className="flex flex-wrap gap-1">
                {Object.entries(ROLE_ICONS).map(([key, Icon]) => (
                  <button
                    key={key}
                    type="button"
                    onClick={() => onChange({ icon: key })}
                    title={key}
                    aria-label={`Icon ${key}`}
                    aria-pressed={r.icon === key}
                    className={cx(
                      "grid size-7 place-items-center rounded-control border",
                      r.icon === key ? "border-accent bg-accent/20" : "border-line hover:border-line-heavy",
                    )}
                  >
                    <Icon className="size-3.5" style={{ color: r.icon === key ? r.color : "var(--fg-muted)" }} />
                  </button>
                ))}
              </div>
            </Field>
          </div>

          <Field label="Ngoại hình nhân vật pixel" hint="Để Tự động thì nhân vật được sinh theo id + cấp bậc; mỗi role sẽ khác nhau.">
            <div className="rounded-lg border border-line bg-surface/40 p-3">
              <AppearanceEditor role={r} onChange={(appearance) => onChange({ appearance })} />
            </div>
          </Field>

          <Field label="Mô tả ngắn" hint="Hiển thị trên UI">
            <TextInput value={r.description} onChange={(v) => onChange({ description: v })} placeholder="VD: Chi phí, ngân sách, unit economics" />
          </Field>
          <Field label="Chỉ dẫn chuyên môn (persona)" hint="Được đưa vào system prompt: role này soi điều gì, tiêu chí APPROVE/REJECT. Nên viết tiếng Anh.">
            <TextArea
              value={r.instructions}
              onChange={(v) => onChange({ instructions: v })}
              rows={4}
              placeholder="VD: You own the budget. Estimate infrastructure and licensing cost with numbers…"
            />
          </Field>

          <div className="grid grid-cols-[1fr_1.6fr_100px] gap-3">
            <Field label="Provider" error={errors[`role.${r.id}.provider`]}>
              <select value={provider ? r.provider_id : ""} onChange={(e) => changeProvider(e.target.value)} className={INPUT}>
                {!provider && <option value="">-- chọn --</option>}
                {draft.providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name || "(chưa đặt tên)"}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Model" error={errors[`role.${r.id}.model`]}>
              <ModelPicker value={r.model} models={provider?.models ?? []} onChange={(m) => onChange({ model: m })} />
            </Field>
            <Field label="Temperature" error={errors[`role.${r.id}.temperature`]}>
              <NumberInput value={r.temperature} step={0.1} onChange={(v) => onChange({ temperature: v ?? Number.NaN })} />
            </Field>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              icon={<Activity className="size-3.5" />}
              loading={checking}
              disabled={dirty || !provider || !r.model.trim()}
              title={dirty ? "Lưu cấu hình trước khi kiểm tra" : "Gọi thử 1 request rất ngắn: model còn dùng được / còn quota không"}
              onClick={() => void check([r.id])}
            >
              Kiểm tra model
            </Button>
            <Button size="sm" variant="ghost" icon={<CopyPlus className="size-3.5" />} onClick={onDuplicate}>
              Nhân bản
            </Button>
            <Button
              size="sm"
              variant="ghost"
              icon={<Trash className="size-3.5" />}
              disabled={usage.length > 0}
              title={usage.length > 0 ? `Đang dùng: ${usage.join(", ")}` : "Xoá role"}
              onClick={onRemove}
            >
              Xoá
            </Button>
            {usage.length > 0 && <span className="text-[11px] text-fg-subtle">Đang dùng: {usage.join(" · ")}</span>}
          </div>
          {fresh && !checking && (
            <div
              className={cx(
                "flex items-start gap-2 rounded-control px-2.5 py-1.5 text-xs",
                fresh.status === "ok" ? "bg-success-subtle text-success" : "bg-danger-subtle text-danger",
              )}
            >
              {fresh.status === "ok" ? <CircleCheck className="mt-px size-3.5 shrink-0" /> : <CircleX className="mt-px size-3.5 shrink-0" />}
              <span className="min-w-0 break-words">
                <b>{HEALTH_META[fresh.status].label}</b>
                {fresh.latency_ms != null && ` · ${fresh.latency_ms} ms`} · {fresh.provider_name} / {fresh.model}
                <br />
                {fresh.status !== "ok" && <span className="opacity-80">{HEALTH_META[fresh.status].hint} </span>}
                <span className="opacity-70">{fresh.message}</span>
                {fresh.balance && (
                  <>
                    <br />
                    Số dư: <b>{fresh.balance}</b>
                  </>
                )}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Kiểm tra model của mọi role đã gán (mỗi cặp provider + model chỉ gọi 1 lần). */
function CheckAllButton({ draft, dirty }: { draft: AppConfig; dirty: boolean }) {
  const { check, checking } = useHealth();
  const assigned = draft.roles.filter((r) => draft.providers.some((p) => p.id === r.provider_id) && r.model.trim());
  const pairs = new Set(assigned.map((r) => `${r.provider_id}|${r.model.trim()}`)).size;
  const run = async () => {
    const res = await check(assigned.map((r) => r.id));
    if (res.length === 0) return;
    const bad = res.filter((h) => h.status !== "ok");
    toast(
      bad.length ? "warning" : "success",
      bad.length ? `${bad.length}/${res.length} model có vấn đề` : `Cả ${res.length} model đều hoạt động`,
      bad.map((h) => `${h.model}: ${HEALTH_META[h.status].label} (${h.role_ids.length} role)`).join(" · ") || undefined,
    );
  };
  return (
    <Button
      size="sm"
      icon={<Activity className="size-3.5" />}
      loading={checking === "all" || checking instanceof Set}
      disabled={dirty || pairs === 0}
      title={dirty ? "Lưu cấu hình trước khi kiểm tra" : `Gọi thử ${pairs} cặp provider/model (rất ít token)`}
      onClick={() => void run()}
    >
      Kiểm tra tất cả model ({pairs})
    </Button>
  );
}