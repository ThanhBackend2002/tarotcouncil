// Wrapper có kiểu cho các Tauri command. Mọi lỗi được chuẩn hoá thành `CommandError`.

import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AppConfig,
  CreateReadingResult,
  Deck,
  DrawResult,
  Reading,
  ReadingSnapshot,
  ReadingSummary,
  Spread,
  SupportInfo,
  UsageBreakdown,
  Verification,
  CommandError,
  ConnectionTestResult,
  KeyStorage,
  ModelHealth,
  ModelList,
  RoleDef,
  RoleId,
  SecretCommandError,
  SecretStatusReport,
  SecretTarget,
  SetSecretResult,
} from "./contracts";

const KINDS: ReadonlySet<CommandError["kind"]> = new Set([
  "invalid",
  "not_implemented",
  "not_found",
  "storage",
  "llm",
  "internal",
  "secret_missing",
  "busy",
  "persist_failed",
  "privacy_required",
  "index_unavailable",
  "index",
]);

export function toCommandError(e: unknown): CommandError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    const kind = String((e as { kind: unknown }).kind);
    const message = String((e as { message: unknown }).message);
    const target = (e as { target?: CommandError["target"] }).target;
    return {
      kind: KINDS.has(kind as CommandError["kind"]) ? (kind as CommandError["kind"]) : "internal",
      message,
      ...(target ? { target } : {}),
    };
  }
  if (e instanceof Error) return { kind: "internal", message: e.message };
  return { kind: "internal", message: String(e) };
}

export function isCommandError(e: unknown): e is CommandError {
  return !!e && typeof e === "object" && "kind" in e && "message" in e;
}

export const inTauri = (): boolean => {
  try {
    return isTauri();
  } catch {
    return false;
  }
};

const NOT_TAURI = "Không chạy trong Tauri — hãy mở app bằng `npm run tauri dev`.";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) throw { kind: "internal", message: NOT_TAURI } satisfies CommandError;
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toCommandError(e);
  }
}

export const ipc = {
  getConfig: () => call<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => call<AppConfig>("save_config", { config }),
  /** Người dùng đồng ý thông báo quyền riêng tư. */
  acknowledgePrivacy: () => call<AppConfig>("acknowledge_privacy"),
  defaultRoles: () => call<RoleDef[]>("default_roles"),
  /** `roleIds` undefined = kiểm tra mọi thầy bói. */
  checkModels: (roleIds?: RoleId[]) => call<ModelHealth[]>("check_models", { roleIds }),
  testConnection: (agent: RoleId) => call<ConnectionTestResult>("test_connection", { agent }),
  /** `baseUrl`/`apiKey` cho phép thử provider đang sửa dở; bỏ trống = dùng bản đã lưu. */
  listModels: (providerId: string, baseUrl?: string, apiKey?: string | null) =>
    call<ModelList>("list_models", { providerId, baseUrl, apiKey: apiKey ?? undefined }),
  openLogDir: () => call<string>("open_log_dir"),
};

// ── Đọc bài + Nhật ký (backend dùng `rename_all = "snake_case"` cho tham số) ──
export const readingIpc = {
  listSpreads: () => call<Spread[]>("list_spreads"),
  getDeck: () => call<Deck>("get_deck"),
  supportInfo: () => call<SupportInfo>("get_support_info"),
  create: (question: string, spreadId: string, readerIds: RoleId[]) =>
    call<CreateReadingResult>("create_reading", { question, spread_id: spreadId, reader_ids: readerIds }),
  draw: (readingId: string, fanIndex: number) => call<DrawResult>("draw_card", { reading_id: readingId, fan_index: fanIndex }),
  autoDraw: (readingId: string) => call<Reading>("auto_draw", { reading_id: readingId }),
  reshuffle: (readingId: string) => call<Reading>("reshuffle_reading", { reading_id: readingId }),
  interpret: (readingId: string) => call<number>("interpret_reading", { reading_id: readingId }),
  cancel: (readingId: string) => call<boolean>("cancel_reading", { reading_id: readingId }),
  get: (readingId: string) => call<ReadingSnapshot>("get_reading", { reading_id: readingId }),
  list: (limit?: number) => call<ReadingSummary[]>("list_readings", { limit }),
  search: (query: string, limit?: number) => call<ReadingSummary[]>("search_readings", { query, limit }),
  remove: (readingId: string) => call<boolean>("delete_reading", { reading_id: readingId }),
  verify: (readingId: string) => call<Verification>("verify_reading", { reading_id: readingId }),
  usage: (readingId: string) => call<UsageBreakdown>("get_usage", { reading_id: readingId }),
};

// ── API key (keyring) — giữ nguyên `kind` của backend để UI phân biệt keyring_unavailable / keyring_locked ──

async function callSecret<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) throw { kind: "internal", message: NOT_TAURI } satisfies SecretCommandError;
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (e && typeof e === "object" && "kind" in e && "message" in e) {
      const o = e as { kind: unknown; message: unknown; target?: SecretTarget };
      throw { kind: String(o.kind), message: String(o.message), target: o.target } satisfies SecretCommandError;
    }
    throw { kind: "internal", message: e instanceof Error ? e.message : String(e) } satisfies SecretCommandError;
  }
}

export const secretIpc = {
  /** `apiKey` là giá trị mask = dùng key hiện tại (đổi nơi lưu / thử lại / mở khoá). Chỉ "keyring" | "memory". */
  setSecret: (target: SecretTarget, apiKey: string, storage: Exclude<KeyStorage, "file">) =>
    callSecret<SetSecretResult>("set_secret", { target, api_key: apiKey, storage }),
  deleteSecret: (target: SecretTarget) => callSecret<void>("delete_secret", { target }),
  status: () => callSecret<SecretStatusReport>("secret_status"),
  deleteLeftoverFiles: (files: string[]) => callSecret<string[]>("delete_leftover_files", { files }),
};
