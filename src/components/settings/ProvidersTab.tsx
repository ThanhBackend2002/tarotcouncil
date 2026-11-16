import { useState, type ReactNode } from "react";
import { ChevronDown, CircleCheck, CircleX, LoaderCircle, Plus, RotateCcw, Server, Trash } from "lucide-react";
import { formatDate } from "../../lib/agents";
import { MASK_PREFIX, type AppConfig, type Provider } from "../../lib/contracts";
import { isLocalUrl } from "../../store/config";
import { Button, Panel } from "../ui";
import {
  Field,
  INPUT,
  KeyInput,
  LeftoverFilesWarning,
  ModelPicker,
  SecretControls,
  SectionTitle,
  TextInput,
  findStatus,
  newId,
  useSecretStatus,
  type Errors,
} from "./fields";

export const PRESETS: { name: string; base_url: string }[] = [
  { name: "OpenAI", base_url: "https://api.openai.com/v1" },
  { name: "OpenRouter", base_url: "https://openrouter.ai/api/v1" },
  { name: "Anthropic", base_url: "https://api.anthropic.com/v1" },
  { name: "Google Gemini", base_url: "https://generativelanguage.googleapis.com/v1beta/openai" },
  { name: "Groq", base_url: "https://api.groq.com/openai/v1" },
  { name: "DeepSeek", base_url: "https://api.deepseek.com/v1" },
  { name: "Ollama (local)", base_url: "http://localhost:11434/v1" },
  { name: "LM Studio (local)", base_url: "http://localhost:1234/v1" },
  { name: "Tuỳ chỉnh", base_url: "" },
];

export const apiRoot = (u: string) => u.trim().replace(/\/+$/, "").replace(/\/chat\/completions$/, "");

export interface FetchState {
  loading: boolean;
  error: string | null;
}

export function ProvidersTab({
  draft,
  saved,
  errors,
  update,
  fetches,
  onFetch,
  onCommit,
}: {
  draft: AppConfig;
  saved: AppConfig;
  errors: Errors;
  update: (f: (c: AppConfig) => AppConfig) => void;
  fetches: Record<string, FetchState>;
  onFetch: (p: Provider) => void;
  onCommit: (id: string) => void;
}) {
  // Trạng thái nơi lưu key — tải lại sau mỗi lần lưu cấu hình.
  const secrets = useSecretStatus(saved);
  const setProvider = (id: string, patch: Partial<Provider>) =>
    update((c) => ({ ...c, providers: c.providers.map((p) => (p.id === id ? { ...p, ...patch } : p)) }));

  const add = (preset: (typeof PRESETS)[number]) => {
    const p: Provider = {
      id: newId("p"),
      name: preset.base_url ? preset.name : "",
      base_url: preset.base_url,
      api_key: null,
      models: [],
      models_fetched_at: null,
    };
    // Role chưa có provider hợp lệ (vd. lần đầu cài) → gán luôn cho provider mới.
    update((c) => {
      const valid = new Set(c.providers.map((x) => x.id));
      return {
        ...c,
        providers: [...c.providers, p],
        roles: c.roles.map((r) => (valid.has(r.provider_id) ? r : { ...r, provider_id: p.id })),
      };
    });
    if (isLocalUrl(p.base_url)) onFetch(p);
  };

  /** Xoá provider; role đang dùng được chuyển sang `target` (null = để trống, chọn sau). */
  const remove = (id: string, target: string | null, model: string) =>
    update((c) => ({
      ...c,
      providers: c.providers.filter((x) => x.id !== id),
      roles: c.roles.map((r) =>
        r.provider_id === id ? { ...r, provider_id: target ?? "", model: target && model.trim() ? model.trim() : target ? r.model : "" } : r,
      ),
    }));

  return (
    <Panel className="p-4">
      <SectionTitle
        title="Nhà cung cấp (API key)"
        desc="Mỗi provider là 1 endpoint tương thích OpenAI. Nhập key xong, danh sách model được kéo tự động từ GET /models để các role chọn."
      >
        <AddProviderMenu onPick={add} />
      </SectionTitle>
      <LeftoverFilesWarning files={secrets.report?.leftover_files ?? []} refresh={secrets.refresh} />
      {draft.providers.length === 0 && (
        <p className="mb-3 rounded-md border border-warn-line bg-warn-subtle px-3 py-2 text-xs text-warn">
          Chưa có provider nào. Bấm <b>Thêm provider</b>, nhập base URL + API key; mọi role sẽ được gán vào provider đầu tiên.
        </p>
      )}
      <div className="space-y-3">
        {draft.providers.map((p) => (
          <ProviderCard
            key={p.id}
            provider={p}
            saved={saved.providers.find((x) => x.id === p.id)}
            usedBy={draft.roles.filter((r) => r.provider_id === p.id).length}
            errors={errors}
            fetch={fetches[p.id]}
            onChange={(patch) => setProvider(p.id, patch)}
            onCommit={() => onCommit(p.id)}
            onFetch={() => onFetch(p)}
            others={draft.providers.filter((x) => x.id !== p.id)}
            onRemove={(target, model) => remove(p.id, target, model)}
            secret={
              <SecretControls
                target={{ kind: "provider", id: p.id }}
                status={findStatus(secrets.report, { kind: "provider", id: p.id })}
                value={p.api_key}
                canStore={(() => {
                  const s = saved.providers.find((x) => x.id === p.id);
                  return !!s && apiRoot(s.base_url) === apiRoot(p.base_url);
                })()}
                update={update}
                refresh={secrets.refresh}
              />
            }
          />
        ))}
      </div>
    </Panel>
  );
}

function AddProviderMenu({ onPick }: { onPick: (p: (typeof PRESETS)[number]) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative shrink-0" onBlur={(e) => !e.currentTarget.contains(e.relatedTarget) && setOpen(false)}>
      <Button size="sm" icon={<Plus className="size-3.5" />} onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        Thêm provider <ChevronDown className="size-3.5" />
      </Button>
      {open && (
        <div className="absolute right-0 z-20 mt-1 w-64 overflow-hidden rounded-lg border border-line-strong bg-surface py-1 shadow-xl shadow-black/50">
          {PRESETS.map((p) => (
            <button
              key={p.name}
              type="button"
              onClick={() => {
                onPick(p);
                setOpen(false);
              }}
              className="flex w-full flex-col items-start px-3 py-1.5 text-left hover:bg-surface-2"
            >
              <span className="text-sm text-fg">{p.name}</span>
              {p.base_url && <span className="font-mono text-[11px] text-fg-subtle">{p.base_url}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ProviderCard({
  provider: p,
  saved,
  usedBy,
  errors,
  fetch,
  onChange,
  onCommit,
  onFetch,
  others,
  onRemove,
  secret,
}: {
  provider: Provider;
  saved: Provider | undefined;
  usedBy: number;
  errors: Errors;
  fetch: FetchState | undefined;
  onChange: (patch: Partial<Provider>) => void;
  onCommit: () => void;
  onFetch: () => void;
  others: Provider[];
  onRemove: (target: string | null, model: string) => void;
  /** Nơi lưu key + thao tác keyring (badge, thử lại, hộp thoại đồng ý). */
  secret?: ReactNode;
}) {
  const [removing, setRemoving] = useState(false);
  const [target, setTarget] = useState<string>(others[0]?.id ?? "");
  const [newModel, setNewModel] = useState("");
  const targetProvider = others.find((o) => o.id === target);
  const baseChanged = !!saved && apiRoot(saved.base_url) !== apiRoot(p.base_url);
  const maskedKey = !!p.api_key?.startsWith(MASK_PREFIX);
  let validUrl = false;
  try {
    validUrl = ["http:", "https:"].includes(new URL(p.base_url).protocol);
  } catch {
    validUrl = false;
  }

  return (
    <div className="rounded-lg border border-line bg-canvas/40 p-3">
      <div className="mb-3 flex items-center gap-2">
        <span className="grid size-7 place-items-center rounded-md bg-surface-2">
          <Server className="size-4 text-fg-soft" />
        </span>
        <span className="text-sm font-medium text-fg">{p.name || "Provider mới"}</span>
        {!saved && <span className="text-[11px] text-warn">chưa lưu</span>}
        <span className="text-[11px] text-fg-subtle">{usedBy} role đang dùng</span>
        <Button
          size="sm"
          variant="ghost"
          className="ml-auto"
          icon={<Trash className="size-3.5" />}
          title="Xoá provider"
          onClick={() => setRemoving((v) => !v)}
          aria-label="Xoá provider"
        />
      </div>
      {removing && (
        <div className="mb-3 rounded-md border border-danger-line bg-danger-subtle p-3 text-xs">
          {usedBy === 0 ? (
            <div className="flex items-center gap-2">
              <span className="text-fg-soft">Xoá provider "{p.name || "chưa đặt tên"}"?</span>
              <Button size="sm" variant="danger" onClick={() => onRemove(null, "")}>
                Xoá
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setRemoving(false)}>
                Huỷ
              </Button>
            </div>
          ) : (
            <div className="space-y-2">
              <div className="text-fg-soft">
                Có <b>{usedBy} role</b> đang dùng provider này. Chuyển các role đó sang:
              </div>
              <div className="grid grid-cols-[1fr_1.6fr] gap-2">
                <select value={target} onChange={(e) => setTarget(e.target.value)} className={INPUT}>
                  {others.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.name || "(chưa đặt tên)"}
                    </option>
                  ))}
                  <option value="">Để trống (chọn sau)</option>
                </select>
                {target ? (
                  <div>
                    <ModelPicker value={newModel} models={targetProvider?.models ?? []} onChange={setNewModel} />
                    <span className="mt-1 block text-[11px] text-fg-subtle">Để trống = giữ tên model hiện tại của từng role.</span>
                  </div>
                ) : (
                  <span className="self-center text-fg-subtle">Các role sẽ chưa có provider/model cho tới khi bạn chọn lại.</span>
                )}
              </div>
              <div className="flex gap-2">
                <Button size="sm" variant="danger" onClick={() => onRemove(target || null, newModel)}>
                  Chuyển {usedBy} role & xoá
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setRemoving(false)}>
                  Huỷ
                </Button>
              </div>
            </div>
          )}
        </div>
      )}
      <div className="grid grid-cols-[1fr_2fr] gap-3">
        <Field label="Tên" error={errors[`provider.${p.id}.name`]}>
          <TextInput value={p.name} onChange={(v) => onChange({ name: v })} placeholder="VD: OpenRouter" />
        </Field>
        <Field label="Base URL" error={errors[`provider.${p.id}.base_url`]}>
          <TextInput
            value={p.base_url}
            onChange={(v) => onChange({ base_url: v })}
            onBlur={onCommit}
            placeholder="https://api.example.com/v1"
            mono
          />
        </Field>
        <Field label="API key" className="col-span-2">
          <KeyInput
            value={p.api_key}
            saved={saved?.api_key ?? null}
            onChange={(v) => onChange({ api_key: v })}
            onCommit={onCommit}
            placeholder={isLocalUrl(p.base_url) ? "Thường không cần với server local" : "sk-…"}
          />
          {baseChanged && maskedKey && (
            <span className="mt-1 block text-[11px] text-warn">
              Đã đổi base URL: key cũ sẽ không được gửi tới host mới. Hãy nhập lại key.
            </span>
          )}
          {secret}
        </Field>
      </div>
      <div className="mt-3 flex items-center gap-3 rounded-md bg-surface/60 px-3 py-2 text-xs">
        {fetch?.loading ? (
          <span className="inline-flex items-center gap-1.5 text-info">
            <LoaderCircle className="size-3.5 animate-spin" /> Đang lấy danh sách model…
          </span>
        ) : fetch?.error ? (
          <span className="inline-flex min-w-0 items-start gap-1.5 text-danger">
            <CircleX className="mt-px size-3.5 shrink-0" />
            <span className="break-words">Không lấy được model: {fetch.error}</span>
          </span>
        ) : p.models.length > 0 ? (
          <span className="inline-flex items-center gap-1.5 text-success">
            <CircleCheck className="size-3.5" /> {p.models.length} model
            {p.models_fetched_at && <span className="text-fg-subtle">· cập nhật {formatDate(p.models_fetched_at)}</span>}
          </span>
        ) : (
          <span className="text-fg-subtle">Chưa có danh sách model. Nhập key rồi bấm "Lấy model".</span>
        )}
        <Button
          size="sm"
          className="ml-auto"
          icon={<RotateCcw className="size-3.5" />}
          loading={fetch?.loading}
          disabled={!validUrl}
          onClick={onFetch}
        >
          {p.models.length > 0 ? "Làm mới" : "Lấy model"}
        </Button>
      </div>
    </div>
  );
}
