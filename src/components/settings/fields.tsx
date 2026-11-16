// Primitive form dùng chung cho các tab Settings.

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ChevronDown, Eye, EyeOff, KeyRound, LockKeyholeOpen, Plus, RotateCcw, Trash, TriangleAlert, Undo2, X } from "lucide-react";
import { LEVELS, makeResolver } from "../../lib/agents";
import {
  MASK_PREFIX,
  type AppConfig,
  type KeyStorage,
  type RoleDef,
  type RoleId,
  type SecretCommandError,
  type SecretStatusReport,
  type SecretTarget,
  type SecretTargetStatus,
} from "../../lib/contracts";
import { ipc, secretIpc } from "../../lib/ipc";
import { useConfig } from "../../store/config";
import { toast } from "../../store/toast";
import { Dialog } from "../Dialog";
import { PixelPortrait } from "../PixelCharacter";
import { Badge, Button, cx } from "../ui";

export type Errors = Partial<Record<string, string>>;

export const INPUT =
  "h-9 w-full rounded-md border border-line-strong bg-canvas px-2.5 text-sm text-fg placeholder:text-fg-faint focus:border-accent/60 focus:outline-none";

export function SectionTitle({ title, desc, children }: { title: string; desc?: string; children?: ReactNode }) {
  return (
    <div className="mb-4 flex items-start gap-3">
      <div className="min-w-0 flex-1">
        <h2 className="text-sm font-semibold text-fg">{title}</h2>
        {desc && <p className="mt-0.5 text-xs text-fg-subtle">{desc}</p>}
      </div>
      {children}
    </div>
  );
}

export function Field({
  label,
  error,
  hint,
  className,
  children,
}: {
  label: string;
  error?: string;
  hint?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cx("block min-w-0", className)}>
      <span className="mb-1 block text-xs font-medium text-fg-muted">{label}</span>
      {children}
      {error ? (
        <span className="mt-1 block text-[11px] text-danger">{error}</span>
      ) : hint ? (
        <span className="mt-1 block text-[11px] text-fg-faint">{hint}</span>
      ) : null}
    </div>
  );
}

export function TextInput({
  value,
  onChange,
  onBlur,
  placeholder,
  mono,
}: {
  value: string;
  onChange: (v: string) => void;
  onBlur?: () => void;
  placeholder?: string;
  mono?: boolean;
}) {
  return (
    <input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onBlur={onBlur}
      placeholder={placeholder}
      spellCheck={false}
      autoComplete="off"
      className={cx(INPUT, mono && "font-mono text-[13px]")}
    />
  );
}

export function TextArea({
  value,
  onChange,
  placeholder,
  rows = 3,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  rows?: number;
}) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      rows={rows}
      className="w-full resize-y rounded-md border border-line-strong bg-canvas px-2.5 py-2 text-sm text-fg placeholder:text-fg-faint focus:border-accent/60 focus:outline-none"
    />
  );
}

/** Giữ text thô cục bộ để gõ "0." hay xoá trắng không bị ép giá trị. */
export function NumberInput({
  value,
  onChange,
  nullable,
  step = 1,
}: {
  value: number | null;
  onChange: (v: number | null) => void;
  nullable?: boolean;
  step?: number;
}) {
  const fmt = (v: number | null) => (v == null || Number.isNaN(v) ? "" : String(v));
  const [text, setText] = useState(fmt(value));
  const last = useRef(value);
  useEffect(() => {
    if (!Object.is(value, last.current)) {
      last.current = value;
      setText(fmt(value));
    }
  }, [value]);

  return (
    <input
      type="number"
      inputMode="decimal"
      step={step}
      value={text}
      onChange={(e) => {
        const t = e.target.value;
        setText(t);
        const v = t.trim() === "" ? (nullable ? null : Number.NaN) : Number(t);
        last.current = v;
        onChange(v);
      }}
      className={cx(INPUT, "tabular-nums")}
    />
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <label className="flex h-9 cursor-pointer items-center gap-2 text-sm text-fg-soft">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="size-4 accent-accent" />
      {label}
    </label>
  );
}

/**
 * Ô API key. Backend trả key đã mask (`••••abcd`):
 * - giữ nguyên mask = giữ key cũ;
 * - "Đổi key" = nhập key mới; để trống khi lưu = xoá key.
 */
export function KeyInput({
  value,
  saved,
  onChange,
  onCommit,
  placeholder,
}: {
  value: string | null;
  saved: string | null;
  onChange: (v: string | null) => void;
  onCommit?: () => void;
  placeholder?: string;
}) {
  const [reveal, setReveal] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  const masked = !!value && value.startsWith(MASK_PREFIX);
  const hasSaved = !!saved && saved.startsWith(MASK_PREFIX);

  if (masked) {
    return (
      <div className="flex gap-2">
        <input readOnly value={value ?? ""} className={cx(INPUT, "font-mono text-fg-muted")} aria-label="API key đã lưu" />
        <Button
          onClick={() => {
            onChange("");
            setTimeout(() => ref.current?.focus(), 0);
          }}
        >
          Đổi key
        </Button>
      </div>
    );
  }

  return (
    <div>
      <div className="flex gap-2">
        <div className="relative flex-1">
          <input
            ref={ref}
            type={reveal ? "text" : "password"}
            value={value ?? ""}
            onChange={(e) => onChange(e.target.value)}
            onBlur={onCommit}
            placeholder={placeholder ?? "sk-…"}
            spellCheck={false}
            autoComplete="off"
            className={cx(INPUT, "pr-9 font-mono text-[13px]")}
          />
          <button
            type="button"
            onClick={() => setReveal((r) => !r)}
            className="absolute top-1/2 right-2 -translate-y-1/2 text-fg-subtle hover:text-fg"
            aria-label={reveal ? "Ẩn key" : "Hiện key"}
          >
            {reveal ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
          </button>
        </div>
        {hasSaved && (
          <Button icon={<Undo2 className="size-3.5" />} onClick={() => onChange(saved)} title="Giữ key đã lưu">
            Giữ key cũ
          </Button>
        )}
      </div>
      {hasSaved && !(value ?? "").trim() && (
        <span className="mt-1 block text-[11px] text-warn">Để trống khi lưu sẽ xoá key đã lưu.</span>
      )}
    </div>
  );
}

const MAX_OPTIONS = 300;

/** Combobox: gõ để lọc danh sách model của provider; vẫn cho nhập tay model không có trong list. */
export function ModelPicker({ value, models, onChange }: { value: string; models: string[]; onChange: (m: string) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  const filtered = useMemo(() => {
    const q = (query ?? "").trim().toLowerCase();
    const list = q ? models.filter((m) => m.toLowerCase().includes(q)) : models;
    return list.slice(0, MAX_OPTIONS);
  }, [models, query]);

  useEffect(() => setActive(0), [query, open]);
  useEffect(() => {
    listRef.current?.children[active]?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const pick = (m: string) => {
    onChange(m);
    setOpen(false);
    setQuery(null);
  };
  const unknown = models.length > 0 && value.trim() !== "" && !models.includes(value.trim());

  return (
    <div className="relative">
      <div className="relative">
        <input
          value={query ?? value}
          onChange={(e) => {
            setQuery(e.target.value);
            onChange(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => {
            setOpen(false);
            setQuery(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setOpen(true);
              setActive((a) => Math.min(a + 1, filtered.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setActive((a) => Math.max(a - 1, 0));
            } else if (e.key === "Enter" && open && filtered[active]) {
              e.preventDefault();
              pick(filtered[active]);
            } else if (e.key === "Escape") {
              setOpen(false);
              setQuery(null);
            }
          }}
          placeholder={models.length ? `Chọn trong ${models.length} model…` : "Nhập tên model"}
          spellCheck={false}
          autoComplete="off"
          role="combobox"
          aria-expanded={open}
          className={cx(INPUT, "pr-8 font-mono text-[13px]")}
        />
        <ChevronDown className="pointer-events-none absolute top-1/2 right-2.5 size-4 -translate-y-1/2 text-fg-subtle" />
      </div>
      {open && models.length > 0 && (
        <ul
          ref={listRef}
          role="listbox"
          className="absolute z-30 mt-1 max-h-64 w-full overflow-y-auto rounded-lg border border-line-strong bg-surface py-1 shadow-xl shadow-black/50"
        >
          {filtered.length === 0 ? (
            <li className="px-3 py-1.5 text-xs text-fg-subtle">Không có model khớp, sẽ dùng đúng tên bạn nhập.</li>
          ) : (
            filtered.map((m, i) => (
              <li
                key={m}
                role="option"
                aria-selected={m === value}
                onMouseDown={(e) => {
                  e.preventDefault();
                  pick(m);
                }}
                onMouseEnter={() => setActive(i)}
                className={cx(
                  "cursor-pointer truncate px-3 py-1 font-mono text-[12.5px]",
                  i === active ? "bg-accent/20 text-fg" : "text-fg-soft",
                  m === value && "text-accent-fg",
                )}
              >
                {m}
              </li>
            ))
          )}
          {models.length > MAX_OPTIONS && filtered.length === MAX_OPTIONS && (
            <li className="px-3 py-1 text-[11px] text-fg-subtle">Gõ để lọc thêm ({models.length} model)…</li>
          )}
        </ul>
      )}
      {unknown && <span className="mt-1 block text-[11px] text-warn">Không có trong danh sách của provider</span>}
    </div>
  );
}

/** Dropdown chọn role, nhóm theo cấp bậc. */
export function RoleSelect({
  roles,
  value,
  onChange,
  exclude = [],
  placeholder = "-- chọn role --",
}: {
  roles: RoleDef[];
  value: RoleId | "";
  onChange: (id: RoleId) => void;
  exclude?: RoleId[];
  placeholder?: string;
}) {
  const known = roles.some((r) => r.id === value);
  return (
    <select value={known ? value : ""} onChange={(e) => e.target.value && onChange(e.target.value)} className={INPUT}>
      <option value="">{placeholder}</option>
      {LEVELS.map((l) => {
        const list = roles.filter((r) => r.level === l.id && (r.id === value || !exclude.includes(r.id)));
        if (list.length === 0) return null;
        return (
          <optgroup key={l.id} label={l.label}>
            {list.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name || "(chưa đặt tên)"}
                {r.title && r.title !== r.name ? ` — ${r.title}` : ""}
              </option>
            ))}
          </optgroup>
        );
      })}
    </select>
  );
}

/** Danh sách role dạng chip (có avatar pixel) + ô thêm. */
export function RoleChips({
  roles,
  value,
  onChange,
  exclude = [],
  max = 12,
}: {
  roles: RoleDef[];
  value: RoleId[];
  onChange: (ids: RoleId[]) => void;
  exclude?: RoleId[];
  max?: number;
}) {
  const resolve = useMemo(() => makeResolver([], roles), [roles]);
  const [adding, setAdding] = useState(false);
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {value.map((id) => {
        const m = resolve(id);
        const missing = !roles.some((r) => r.id === id);
        return (
          <span
            key={id}
            className={cx(
              "inline-flex items-center gap-1.5 rounded-full border py-0.5 pr-1 pl-0.5 text-xs",
              missing ? "border-danger-solid/50 text-danger" : "border-line-strong text-fg",
            )}
            title={missing ? "Role không tồn tại" : m.title}
          >
            <PixelPortrait meta={m} size={18} />
            {m.name}
            <button
              type="button"
              onClick={() => onChange(value.filter((x) => x !== id))}
              className="rounded-full p-0.5 text-fg-subtle hover:bg-surface-2 hover:text-fg"
              aria-label={`Bỏ ${m.name}`}
            >
              <X className="size-3" />
            </button>
          </span>
        );
      })}
      {value.length < max &&
        (adding ? (
          <div className="w-60">
            <RoleSelect
              roles={roles}
              value=""
              exclude={[...value, ...exclude]}
              placeholder="-- thêm role --"
              onChange={(id) => {
                onChange([...value, id]);
                setAdding(false);
              }}
            />
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setAdding(true)}
            className="inline-flex items-center gap-1 rounded-full border border-dashed border-line-strong px-2 py-0.5 text-xs text-fg-muted hover:border-line-heavy hover:text-fg"
          >
            <Plus className="size-3" /> Thêm
          </button>
        ))}
    </div>
  );
}

// ───────────────────────────── API key: nơi lưu (keyring / file / RAM) ─────────────────────────────

/** Trạng thái key từ backend; tải lại mỗi khi `dep` (config đã lưu) đổi. */
export function useSecretStatus(dep: unknown) {
  const [report, setReport] = useState<SecretStatusReport | null>(null);
  const refresh = useCallback(async () => {
    try {
      setReport(await secretIpc.status());
    } catch {
      setReport(null); // ngoài Tauri / backend cũ: ẩn phần trạng thái
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [dep, refresh]);
  return { report, refresh };
}

export const sameTarget = (a: SecretTarget, b: SecretTarget) => a.kind === b.kind && a.id === b.id;

export function findStatus(report: SecretStatusReport | null, target: SecretTarget): SecretTargetStatus | undefined {
  return report?.targets.find((s) => sameTarget(s.target, target));
}

/** Sau set_secret/delete_secret: cập nhật config đã lưu + đúng target trong bản nháp (không reset form). */
async function syncTarget(target: SecretTarget, update: (f: (c: AppConfig) => AppConfig) => void) {
  const fresh = await ipc.getConfig();
  useConfig.setState({ config: fresh });
  const p = fresh.providers.find((x) => x.id === target.id);
  update((c) => ({
    ...c,
    providers: c.providers.map((x) => (x.id === target.id ? { ...x, api_key: p?.api_key ?? null, key_storage: p?.key_storage } : x)),
  }));
}

const STORAGE_BADGE: Record<KeyStorage, { label: string; tone: "success" | "warning" }> = {
  keyring: { label: "Keychain", tone: "success" },
  file: { label: "File (plaintext)", tone: "warning" },
  memory: { label: "Chỉ trong RAM — mất khi đóng app", tone: "warning" },
};

export function KeyStorageBadge({ storage }: { storage: KeyStorage }) {
  const b = STORAGE_BADGE[storage];
  return (
    <Badge tone={b.tone} className="gap-1">
      <KeyRound className="size-3" />
      {b.label}
    </Badge>
  );
}

const isNewKey = (v: string | null) => !!v && !!v.trim() && !v.startsWith(MASK_PREFIX);

/**
 * Hàng trạng thái dưới ô API key: badge nơi lưu, lỗi migrate (Thử lại), keyring khoá (Mở khóa / thử lại),
 * nút "Lưu key" cho key vừa nhập, và hộp thoại đồng ý khi keyring không khả dụng.
 * `canStore` = target đã có trong cấu hình đã lưu (provider đã lưu, base URL không đổi / đúng dịch vụ).
 */
export function SecretControls({
  target,
  status,
  value,
  canStore,
  update,
  refresh,
}: {
  target: SecretTarget;
  status: SecretTargetStatus | undefined;
  value: string | null;
  canStore: boolean;
  update: (f: (c: AppConfig) => AppConfig) => void;
  refresh: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [consent, setConsent] = useState(false);
  const [locked, setLocked] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fresh = isNewKey(value);

  // Lưu cấu hình lúc keyring không khả dụng → key đang chỉ ở RAM: hỏi người dùng 1 lần.
  const asked = useRef<string | null>(null);
  useEffect(() => {
    const k = status && status.has_key && status.error_code === "keyring_unavailable" ? `${target.kind}:${target.id}:${status.error}` : null;
    if (k && asked.current !== k) {
      asked.current = k;
      setConsent(true);
    }
  }, [status, target.kind, target.id]);

  const store = async (storage: Exclude<KeyStorage, "file">) => {
    setBusy(true);
    setError(null);
    try {
      const r = await secretIpc.setSecret(target, fresh && value ? value : MASK_PREFIX, storage);
      await syncTarget(target, update);
      setConsent(false);
      setLocked(null);
      toast("success", "Đã lưu API key", STORAGE_BADGE[r.storage].label);
    } catch (e) {
      const err = e as SecretCommandError;
      if (err.kind === "keyring_unavailable") setConsent(true);
      else if (err.kind === "keyring_locked") setLocked(err.message);
      else setError(err.message);
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  const lockedMsg = locked ?? (status?.error_code === "keyring_locked" ? status.error : null);
  const showStatus = !!status && (status.has_key || status.missing || !!status.error);

  if (!showStatus && !(fresh && canStore) && !error) return null;

  return (
    <div className="mt-1.5 space-y-1.5 text-[11px]">
      <div className="flex flex-wrap items-center gap-1.5">
        {status?.has_key && <KeyStorageBadge storage={status.storage} />}
        {status && !status.migrated && (
          <>
            <Badge tone="warning">
              <TriangleAlert className="size-3" /> Key chưa chuyển vào keyring
            </Badge>
            <Button size="sm" variant="ghost" icon={<RotateCcw className="size-3" />} loading={busy} onClick={() => void store("keyring")}>
              Thử lại
            </Button>
          </>
        )}
        {status?.missing && (
          <Badge tone="danger">
            <TriangleAlert className="size-3" /> Thiếu key — nhập lại trước khi đọc bài
          </Badge>
        )}
        {status?.has_key && status.migrated && status.storage !== "keyring" && !fresh && (
          <Button size="sm" variant="ghost" loading={busy} onClick={() => void store("keyring")}>
            Chuyển vào keyring
          </Button>
        )}
        {status?.has_key && status.storage === "memory" && status.error_code === "keyring_unavailable" && (
          <Button size="sm" variant="ghost" onClick={() => setConsent(true)}>
            Chọn cách lưu…
          </Button>
        )}
        {fresh && canStore && (
          <Button size="sm" icon={<KeyRound className="size-3" />} loading={busy} onClick={() => void store("keyring")}>
            Lưu key vào keyring
          </Button>
        )}
      </div>
      {lockedMsg && (
        <div className="flex flex-wrap items-center gap-2 text-warn">
          <span className="min-w-0 break-words">Keyring đang khoá hoặc bị từ chối truy cập.</span>
          <Button size="sm" icon={<LockKeyholeOpen className="size-3" />} loading={busy} onClick={() => void store("keyring")}>
            Mở khóa / thử lại
          </Button>
        </div>
      )}
      {status?.error && !lockedMsg && <span className="block break-words text-warn">{status.error}</span>}
      {error && <span className="block break-words text-danger">{error}</span>}

      <Dialog
        open={consent}
        onClose={() => setConsent(false)}
        tone="warning"
        title="Keyring của hệ điều hành không khả dụng"
        description="Không lưu được API key vào Keychain (macOS), Credential Manager (Windows) hoặc Secret Service (Linux)."
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={() => void store("keyring")}>
              Thử lại keyring
            </Button>
            <Button loading={busy} onClick={() => void store("memory")}>
              Chỉ giữ trong RAM phiên này
            </Button>
          </>
        }
      >
        <div className="space-y-2 text-xs text-fg-soft">
          <p>
            <b>Chỉ giữ trong RAM</b>: key dùng được cho phiên làm việc hiện tại và mất khi đóng app — bạn sẽ phải nhập
            lại trước lượt đọc kế tiếp. TarotCouncil không lưu key ra file.
          </p>
        </div>
      </Dialog>
    </div>
  );
}

/** Cảnh báo file backup/hỏng còn chứa API key plaintext + nút xoá (có xác nhận). */
export function LeftoverFilesWarning({ files, refresh }: { files: string[]; refresh: () => Promise<void> }) {
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (files.length === 0) return null;

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      const left = await secretIpc.deleteLeftoverFiles(files);
      setConfirm(false);
      toast(left.length ? "warning" : "success", left.length ? `Còn ${left.length} file chưa xoá được` : "Đã xoá file chứa key cũ");
    } catch (e) {
      setError((e as SecretCommandError).message);
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  return (
    <div className="mb-3 rounded-md border border-warn-line bg-warn-subtle px-3 py-2 text-xs text-warn" role="alert">
      <div className="flex items-start gap-2">
        <TriangleAlert className="mt-px size-3.5 shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="font-medium">{files.length} file backup/hỏng vẫn chứa API key dạng plaintext:</div>
          <ul className="mt-1 space-y-0.5 font-mono text-[11px] break-all text-warn/80">
            {files.map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        </div>
        <Button size="sm" variant="danger" icon={<Trash className="size-3" />} onClick={() => setConfirm(true)}>
          Xoá
        </Button>
      </div>
      {error && <p className="mt-1 text-danger">{error}</p>}
      <Dialog
        open={confirm}
        onClose={() => setConfirm(false)}
        tone="danger"
        title={`Xoá ${files.length} file chứa key cũ?`}
        description="Các file này là bản backup / file hỏng do app tạo ra trước đây. Xoá rồi không khôi phục được."
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={() => setConfirm(false)}>
              Huỷ
            </Button>
            <Button variant="danger" loading={busy} onClick={() => void remove()}>
              Xoá vĩnh viễn
            </Button>
          </>
        }
      >
        <ul className="space-y-0.5 font-mono text-[11px] break-all text-fg-muted">
          {files.map((f) => (
            <li key={f}>{f}</li>
          ))}
        </ul>
      </Dialog>
    </div>
  );
}

export const newId = (prefix: string) =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? `${prefix}-${crypto.randomUUID().slice(0, 8)}`
    : `${prefix}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
