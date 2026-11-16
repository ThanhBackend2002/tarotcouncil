import { useMemo } from "react";
import { create } from "zustand";
import type { AppConfig, CastMember, Provider, RoleDef, RoleId } from "../lib/contracts";
import { makeResolver, type RoleResolver } from "../lib/agents";
import { ipc } from "../lib/ipc";
import { toastError } from "./toast";

interface ConfigStore {
  config: AppConfig | null;
  /** Tăng mỗi lần load/save → form Settings reset bản nháp. Cập nhật cache model không tăng. */
  revision: number;
  loading: boolean;
  saving: boolean;
  load: () => Promise<void>;
  /** Ném `CommandError` nếu backend từ chối để form hiển thị lỗi validate tại chỗ. */
  save: (cfg: AppConfig) => Promise<AppConfig>;
  /** Ghi cache model mà backend đã lưu, không đụng tới bản nháp của form. */
  patchModels: (providerId: string, models: string[], fetchedAt: string) => void;
}

export const useConfig = create<ConfigStore>((set, get) => ({
  config: null,
  revision: 0,
  loading: false,
  saving: false,
  load: async () => {
    set({ loading: true });
    try {
      set({ config: await ipc.getConfig(), revision: get().revision + 1 });
    } catch (e) {
      toastError(e, "Không đọc được cấu hình");
    } finally {
      set({ loading: false });
    }
  },
  save: async (cfg) => {
    set({ saving: true });
    try {
      const saved = await ipc.saveConfig(cfg);
      set({ config: saved, revision: get().revision + 1 });
      return saved;
    } finally {
      set({ saving: false });
    }
  },
  patchModels: (providerId, models, fetchedAt) => {
    const c = get().config;
    if (!c) return;
    set({
      config: {
        ...c,
        providers: c.providers.map((p) =>
          p.id === providerId ? { ...p, models, models_fetched_at: fetchedAt } : p,
        ),
      },
    });
  },
}));

export function isLocalUrl(url: string): boolean {
  try {
    const h = new URL(url).hostname;
    return h === "localhost" || h === "127.0.0.1" || h === "[::1]" || h === "::1";
  } catch {
    return false;
  }
}

export function providerOf(c: AppConfig, roleId: RoleId): Provider | undefined {
  const r = c.roles.find((x) => x.id === roleId);
  return r ? c.providers.find((p) => p.id === r.provider_id) : undefined;
}

/** Thầy bói tham gia 1 lượt đọc + trưởng thầy (pha tổng hợp). */
export function readersInReading(c: AppConfig, readerIds: RoleId[]): RoleId[] {
  return [...new Set<RoleId>([...readerIds, c.tarot.chief_reader_id])];
}

/** Mọi thầy bói tham gia đều có model + provider có key (hoặc provider local, thường không cần key). */
export function hasUsableKeys(c: AppConfig, readerIds: RoleId[] = c.tarot.default_readers): boolean {
  return readersInReading(c, readerIds).every((id) => {
    const r = c.roles.find((x) => x.id === id);
    const p = providerOf(c, id);
    return !!r && !!r.model.trim() && !!p && (!!p.api_key?.trim() || isLocalUrl(p.base_url));
  });
}

const NO_ROLES: RoleDef[] = [];
const NO_CAST: CastMember[] = [];

/** Resolver id → tên/màu/icon cho 1 phiên (ưu tiên cast đã lưu trong phiên). */
export function useResolver(cast: CastMember[] = NO_CAST): RoleResolver {
  const roles = useConfig((s) => s.config?.roles ?? NO_ROLES);
  return useMemo(() => makeResolver(cast, roles), [cast, roles]);
}
