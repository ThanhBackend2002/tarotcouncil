import {
  CircleHelp,
  Moon,
  BadgeCheck,
  Brain,
  Bug,
  Calendar,
  ChartColumn,
  ClipboardList,
  Cloud,
  Coins,
  Compass,
  Cpu,
  Crown,
  Database,
  FlaskConical,
  Gauge,
  ListChecks,
  Megaphone,
  Monitor,
  Palette,
  Radio,
  Scale,
  Search,
  Server,
  Settings,
  Shield,
  ShieldExclamationPoint,
  Smartphone,
  Target,
  User,
  Users,
  Wallet,
  Workflow,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { Appearance, CastMember, ReadingStatus, RoleDef, RoleId, RoleLevel } from "./contracts";

/** Icon key (lưu trong RoleDef.icon) → component lucide. */
export const ROLE_ICONS: Record<string, LucideIcon> = {
  crown: Crown,
  moon: Moon,
  "help-circle": CircleHelp,
  cpu: Cpu,
  target: Target,
  wallet: Wallet,
  settings: Settings,
  shield: Shield,
  users: Users,
  database: Database,
  clipboard: ClipboardList,
  calendar: Calendar,
  search: Search,
  megaphone: Megaphone,
  coins: Coins,
  radio: Radio,
  compass: Compass,
  wrench: Wrench,
  "badge-check": BadgeCheck,
  gauge: Gauge,
  "shield-alert": ShieldExclamationPoint,
  server: Server,
  monitor: Monitor,
  smartphone: Smartphone,
  cloud: Cloud,
  workflow: Workflow,
  brain: Brain,
  flask: FlaskConical,
  "list-checks": ListChecks,
  bug: Bug,
  palette: Palette,
  chart: ChartColumn,
  scale: Scale,
  user: User,
};

export const LEVELS: { id: RoleLevel; label: string }[] = [
  { id: "c_level", label: "C-Level" },
  { id: "head", label: "Head / Director" },
  { id: "manager", label: "Manager" },
  { id: "lead", label: "Lead / Architect" },
  { id: "engineer", label: "Engineer" },
  { id: "specialist", label: "Specialist / Staff" },
];
export const LEVEL_LABEL: Record<RoleLevel, string> = Object.fromEntries(LEVELS.map((l) => [l.id, l.label])) as Record<
  RoleLevel,
  string
>;

export interface RoleMeta {
  id: RoleId;
  name: string;
  title: string;
  color: string;
  icon: LucideIcon;
  iconKey: string;
  model: string;
  level: RoleLevel;
  appearance?: Appearance;
}

export type RoleResolver = (id: RoleId) => RoleMeta;

/** Ưu tiên ảnh chụp trong lượt đọc (cast) → thầy bói hiện tại trong config → mặc định. */
export function makeResolver(cast: CastMember[], roles: RoleDef[]): RoleResolver {
  const byCast = new Map(cast.map((c) => [c.id, c]));
  const byRole = new Map(roles.map((r) => [r.id, r]));
  const cache = new Map<RoleId, RoleMeta>();
  return (id) => {
    const hit = cache.get(id);
    if (hit) return hit;
    const c = byCast.get(id) ?? byRole.get(id);
    const meta: RoleMeta = c
      ? {
          id,
          name: c.name || id,
          title: c.title,
          color: c.color || "#a1a1aa",
          icon: ROLE_ICONS[c.icon] ?? User,
          iconKey: c.icon,
          model: c.model,
          level: c.level ?? "engineer",
          appearance: c.appearance,
        }
      : { id, name: id, title: "", color: "#a1a1aa", icon: User, iconKey: "user", model: "", level: "engineer" };
    cache.set(id, meta);
    return meta;
  };
}

export type Tone = "neutral" | "info" | "success" | "warning" | "danger";

export const STATUS_META: Record<ReadingStatus | "idle", { label: string; tone: Tone }> = {
  idle: { label: "Sẵn sàng", tone: "neutral" },
  drawing: { label: "Đang bốc bài", tone: "info" },
  drawn: { label: "Đã bốc xong", tone: "info" },
  interpreting: { label: "Đang luận giải", tone: "info" },
  done: { label: "Hoàn tất", tone: "success" },
  failed: { label: "Lỗi", tone: "danger" },
  cancelled: { label: "Đã dừng", tone: "neutral" },
  support: { label: "Cần hỗ trợ", tone: "warning" },
};

export const TONE_CLASS: Record<Tone, string> = {
  neutral: "bg-surface-2 text-fg-soft ring-line-strong",
  info: "bg-info-subtle text-info ring-info-line",
  success: "bg-success-subtle text-success ring-success-line",
  warning: "bg-warn-subtle text-warn ring-warn-line",
  danger: "bg-danger-subtle text-danger ring-danger-line",
};

export const ERROR_TITLE: Record<string, string> = {
  invalid: "Dữ liệu không hợp lệ",
  busy: "Lượt đọc đang bận",
  not_implemented: "Chưa hỗ trợ",
  persist_failed: "Không lưu được lượt đọc",
  not_found: "Không tìm thấy",
  storage: "Lỗi lưu trữ",
  llm: "Lỗi LLM",
  internal: "Lỗi nội bộ",
};

/** 950 → "950", 12_345 → "12.3k", 1_234_567 → "1.23M" */
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 2 : 1).replace(/\.?0+$/, "")}k`;
  return `${(n / 1_000_000).toFixed(2).replace(/\.?0+$/, "")}M`;
}

export const USAGE_KIND_LABEL: Record<string, string> = {
  reader: "Luận giải",
  reader_retry: "Luận giải (thử lại)",
  synthesis: "Tổng hợp",
  synthesis_retry: "Tổng hợp (thử lại)",
};

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("vi-VN", { dateStyle: "short", timeStyle: "short" });
}
