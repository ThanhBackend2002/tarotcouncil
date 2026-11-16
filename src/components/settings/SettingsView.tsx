import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LoaderCircle, RotateCcw, Save } from "lucide-react";
import type { AppConfig, Provider } from "../../lib/contracts";
import { ipc, isCommandError } from "../../lib/ipc";
import { isLocalUrl, useConfig } from "../../store/config";
import { toast, toastError } from "../../store/toast";
import { useUi } from "../../store/ui";
import { Button, Panel, cx } from "../ui";
import { Field, INPUT, NumberInput, SectionTitle, type Errors } from "./fields";
import { InterfaceTab } from "./InterfaceTab";
import { PricingTab } from "./PricingTab";
import { ProvidersTab, apiRoot, type FetchState } from "./ProvidersTab";
import { RolesTab } from "./RolesTab";
import { TarotTab } from "./TarotTab";

type TabId = "providers" | "roles" | "tarot" | "general" | "interface" | "pricing";

const TABS: { id: TabId; label: string; match: (k: string) => boolean }[] = [
  { id: "tarot", label: "Đọc bài", match: (k) => k.startsWith("tarot.") },
  { id: "roles", label: "Thầy bói", match: (k) => k.startsWith("role.") },
  { id: "general", label: "Chung", match: (k) => k.startsWith("general.") },
  { id: "providers", label: "Provider", match: (k) => k.startsWith("provider") },
  { id: "pricing", label: "Giá model", match: (k) => k.startsWith("pricing") },
  // Tuỳ chọn hiển thị trên máy này (localStorage), không thuộc AppConfig → không có lỗi validate.
  { id: "interface", label: "Giao diện", match: () => false },
];

function isUrl(s: string): boolean {
  try {
    return ["http:", "https:"].includes(new URL(s.trim()).protocol);
  } catch {
    return false;
  }
}

/** Validate phía client khớp với `AppConfig::validate` trong config.rs. */
function validate(c: AppConfig): Errors {
  const e: Errors = {};
  const bad = (v: number, lo: number, hi: number) => !Number.isFinite(v) || v < lo || v > hi;
  const range = (key: string, v: number, lo: number, hi: number) => {
    if (bad(v, lo, hi)) e[key] = `Trong khoảng ${lo}–${hi}`;
  };
  range("general.request_timeout_secs", c.request_timeout_secs, 10, 1800);
  range("general.stream_idle_timeout_secs", c.stream_idle_timeout_secs, 10, 600);
  range("general.max_retries", c.max_retries, 0, 6);
  range("general.max_parallel_calls", c.max_parallel_calls, 1, 16);
  if (c.max_tokens != null && (c.max_tokens < 1 || !Number.isInteger(c.max_tokens))) e["general.max_tokens"] = "Số nguyên ≥ 1 hoặc để trống";


  for (const p of c.providers) {
    if (!p.name.trim()) e[`provider.${p.id}.name`] = "Không được để trống";
    if (!isUrl(p.base_url)) e[`provider.${p.id}.base_url`] = "URL http(s) không hợp lệ";
  }

  const roleIds = new Set(c.roles.map((r) => r.id));
  for (const r of c.roles) {
    if (!r.name.trim()) e[`role.${r.id}.name`] = "Không được để trống";
    // Thầy bói chưa gán provider/model vẫn lưu được; chỉ bắt buộc khi tham gia lượt đọc.
    if (r.provider_id && !c.providers.some((p) => p.id === r.provider_id)) e[`role.${r.id}.provider`] = "Provider không tồn tại";
    range(`role.${r.id}.temperature`, r.temperature, 0, 2);
  }

  for (const [model, p] of Object.entries(c.pricing ?? {})) {
    const vals = [p.input_per_m, p.cached_per_m, p.output_per_m];
    if (vals.some((v) => typeof v !== "number" || !Number.isFinite(v) || v < 0)) e[`pricing.${model}`] = "Giá phải là số ≥ 0";
  }

  const t = c.tarot;
  if (t.default_readers.length === 0 || t.default_readers.length > 4) e["tarot.readers"] = "Chọn 1–4 thầy bói";
  else if (t.default_readers.some((r) => !roleIds.has(r))) e["tarot.readers"] = "Có thầy bói không tồn tại";
  if (!roleIds.has(t.chief_reader_id)) e["tarot.chief"] = "Chọn trưởng thầy";
  if (t.max_total_tokens != null && (!Number.isInteger(t.max_total_tokens) || t.max_total_tokens < 1000)) {
    e["tarot.max_total_tokens"] = "Số nguyên ≥ 1000 hoặc để trống";
  }
  return e;
}

function normalize(c: AppConfig): AppConfig {
  return {
    ...c,
    providers: c.providers.map((p) => ({ ...p, name: p.name.trim(), base_url: p.base_url.trim() })),
    roles: c.roles.map((r) => ({ ...r, name: r.name.trim(), title: r.title.trim(), model: r.model.trim() })),
  };
}

const hasKey = (k: string | null) => !!k && k.trim() !== "";

export function SettingsView() {
  const { config, revision, load, save, saving, loading, patchModels } = useConfig();
  const [draft, setDraft] = useState<AppConfig | null>(null);
  const [tab, setTab] = useState<TabId>("tarot");
  // Lần đầu có config: chưa có provider nào → mở thẳng tab Provider (cần thêm API key trước).
  const tabInit = useRef(false);
  useEffect(() => {
    if (!config || tabInit.current) return;
    tabInit.current = true;
    if (config.providers.length === 0) setTab("providers");
  }, [config]);
  // Nơi khác yêu cầu mở thẳng 1 tab (vd. onboarding ở Plan Viewer → Provider).
  const settingsFocus = useUi((s) => s.settingsFocus);
  useEffect(() => {
    if (settingsFocus && TABS.some((t) => t.id === settingsFocus.tab)) {
      tabInit.current = true;
      setTab(settingsFocus.tab as TabId);
    }
  }, [settingsFocus]);
  const [serverErrors, setServerErrors] = useState<string[]>([]);
  const [fetches, setFetches] = useState<Record<string, FetchState>>({});
  const lastFetched = useRef(new Map<string, string>());

  // Chỉ reset bản nháp khi load/save, không phải khi backend ghi cache model.
  useEffect(() => {
    const c = useConfig.getState().config;
    if (c) setDraft(structuredClone(c));
  }, [revision]);

  const update = useCallback((f: (c: AppConfig) => AppConfig) => setDraft((d) => (d ? f(d) : d)), []);

  const fetchModels = useCallback(
    async (p: Provider, opts: { silent?: boolean } = {}) => {
      if (!isUrl(p.base_url)) return;
      lastFetched.current.set(p.id, `${apiRoot(p.base_url)}|${p.api_key ?? ""}`);
      setFetches((f) => ({ ...f, [p.id]: { loading: true, error: null } }));
      try {
        const r = await ipc.listModels(p.id, p.base_url, p.api_key);
        update((d) => ({
          ...d,
          providers: d.providers.map((x) => (x.id === p.id ? { ...x, models: r.models, models_fetched_at: r.fetched_at } : x)),
        }));
        if (r.cached) patchModels(p.id, r.models, r.fetched_at);
        setFetches((f) => ({ ...f, [p.id]: { loading: false, error: null } }));
        if (!opts.silent) toast("success", `${p.name || "Provider"}: ${r.models.length} model`);
      } catch (e) {
        setFetches((f) => ({ ...f, [p.id]: { loading: false, error: isCommandError(e) ? e.message : String(e) } }));
      }
    },
    [patchModels, update],
  );

  // Lần đầu mở Settings: tự kéo model cho provider đã có key (hoặc local) mà chưa có cache.
  const autoDone = useRef(false);
  useEffect(() => {
    if (!draft || autoDone.current) return;
    autoDone.current = true;
    for (const p of draft.providers) {
      if (p.models.length === 0 && (hasKey(p.api_key) || isLocalUrl(p.base_url))) void fetchModels(p, { silent: true });
    }
  }, [draft, fetchModels]);

  const maybeFetch = (id: string) => {
    const p = draft?.providers.find((x) => x.id === id);
    if (!p || !isUrl(p.base_url)) return;
    if (!hasKey(p.api_key) && !isLocalUrl(p.base_url)) return;
    if (lastFetched.current.get(id) === `${apiRoot(p.base_url)}|${p.api_key ?? ""}`) return;
    void fetchModels(p, { silent: true });
  };

  const errors = useMemo(() => (draft ? validate(draft) : {}), [draft]);
  const dirty = !!draft && !!config && JSON.stringify(draft) !== JSON.stringify(config);
  const errorKeys = Object.keys(errors);

  if (!draft || !config) {
    return (
      <div className="flex h-full items-center justify-center gap-3 text-sm text-fg-subtle">
        {loading ? <LoaderCircle className="size-4 animate-spin" /> : null}
        {loading ? "Đang tải cấu hình…" : "Chưa tải được cấu hình."}
        {!loading && (
          <Button size="sm" onClick={() => void load()}>
            Thử lại
          </Button>
        )}
      </div>
    );
  }

  const onSave = async () => {
    setServerErrors([]);
    try {
      await save(normalize(draft));
      toast("success", "Đã lưu cấu hình");
    } catch (e) {
      if (isCommandError(e) && e.kind === "invalid") setServerErrors(e.message.split("; "));
      else toastError(e, "Không lưu được cấu hình");
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="sticky top-0 z-20 border-b border-line bg-canvas/95 backdrop-blur">
        <div className="mx-auto flex max-w-5xl items-center gap-3 px-6 pt-3">
          <h1 className="text-lg font-semibold">Cài đặt</h1>
          {dirty && <span className="text-xs text-warn">Có thay đổi chưa lưu</span>}
          {errorKeys.length > 0 && dirty && <span className="text-xs text-danger">Còn {errorKeys.length} lỗi</span>}
          <div className="ml-auto flex gap-2">
            <Button
              size="sm"
              variant="ghost"
              icon={<RotateCcw className="size-3.5" />}
              disabled={!dirty || saving}
              onClick={() => {
                setDraft(structuredClone(config));
                setServerErrors([]);
              }}
            >
              Hoàn tác
            </Button>
            <Button
              size="sm"
              variant="primary"
              icon={<Save className="size-3.5" />}
              loading={saving}
              disabled={!dirty || errorKeys.length > 0}
              onClick={() => void onSave()}
            >
              Lưu
            </Button>
          </div>
        </div>
        <nav className="mx-auto flex max-w-5xl gap-1 px-6 pt-2" aria-label="Mục cài đặt">
          {TABS.map((t) => {
            const n = errorKeys.filter(t.match).length;
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => setTab(t.id)}
                aria-current={tab === t.id ? "page" : undefined}
                className={cx(
                  "-mb-px inline-flex items-center gap-1.5 border-b-2 px-3 py-2 text-sm",
                  tab === t.id ? "border-accent text-fg" : "border-transparent text-fg-muted hover:text-fg-soft",
                )}
              >
                {t.label}
                {t.id === "roles" && <span className="text-[11px] text-fg-subtle">{draft.roles.length}</span>}
                {n > 0 && <span className="rounded-full bg-danger-subtle px-1.5 text-[10px] text-danger">{n}</span>}
              </button>
            );
          })}
        </nav>
      </div>

      <div className="mx-auto max-w-5xl space-y-4 p-6 pb-24">
        {serverErrors.length > 0 && (
          <div className="rounded-lg border border-danger-line bg-danger-subtle p-3 text-sm text-danger" role="alert">
            <div className="mb-1 font-medium">Backend từ chối cấu hình:</div>
            <ul className="list-disc space-y-0.5 pl-5 text-xs">
              {serverErrors.map((m, i) => (
                <li key={i}>{m}</li>
              ))}
            </ul>
          </div>
        )}

        {tab === "providers" && (
          <ProvidersTab
            draft={draft}
            saved={config}
            errors={errors}
            update={update}
            fetches={fetches}
            onFetch={(p) => void fetchModels(p)}
            onCommit={maybeFetch}
          />
        )}
        {tab === "roles" && <RolesTab draft={draft} errors={errors} dirty={dirty} update={update} />}
        {tab === "tarot" && <TarotTab draft={draft} errors={errors} update={update} />}
        {tab === "general" && <GeneralTab draft={draft} errors={errors} update={update} />}
        {tab === "pricing" && <PricingTab draft={draft} errors={errors} update={update} />}
        {tab === "interface" && <InterfaceTab />}
      </div>
    </div>
  );
}

function GeneralTab({
  draft,
  errors,
  update,
}: {
  draft: AppConfig;
  errors: Errors;
  update: (f: (c: AppConfig) => AppConfig) => void;
}) {
  const set = (patch: Partial<AppConfig>) => update((c) => ({ ...c, ...patch }));
  return (
    <Panel className="p-4">
      <SectionTitle title="Mạng và giới hạn" />
      <div className="grid grid-cols-3 gap-4">
        <Field label="Số request song song" error={errors["general.max_parallel_calls"]} hint="1–16 · giảm nếu provider báo 429">
          <NumberInput value={draft.max_parallel_calls} onChange={(v) => set({ max_parallel_calls: v ?? Number.NaN })} />
        </Field>
        <Field label="Số lần retry" error={errors["general.max_retries"]} hint="0–6">
          <NumberInput value={draft.max_retries} onChange={(v) => set({ max_retries: v ?? Number.NaN })} />
        </Field>
        <Field label="Max tokens" error={errors["general.max_tokens"]} hint="Trống = mặc định của model">
          <NumberInput value={draft.max_tokens} nullable onChange={(v) => set({ max_tokens: v })} />
        </Field>
        <Field label="Timeout request (giây)" error={errors["general.request_timeout_secs"]} hint="10–1800">
          <NumberInput value={draft.request_timeout_secs} onChange={(v) => set({ request_timeout_secs: v ?? Number.NaN })} />
        </Field>
        <Field label="Idle timeout khi stream (giây)" error={errors["general.stream_idle_timeout_secs"]} hint="10–600">
          <NumberInput value={draft.stream_idle_timeout_secs} onChange={(v) => set({ stream_idle_timeout_secs: v ?? Number.NaN })} />
        </Field>
        <Field label="JSON mode" hint="Tự fallback nếu provider không hỗ trợ">
          <select value={draft.json_mode} onChange={(e) => set({ json_mode: e.target.value as AppConfig["json_mode"] })} className={INPUT}>
            <option value="json_object">json_object</option>
            <option value="off">off (chỉ dựa vào prompt)</option>
          </select>
        </Field>
      </div>
    </Panel>
  );
}
