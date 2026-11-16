// Mirror kiểu dữ liệu của backend Rust (src-tauri/src). Đổi ở Rust → đổi ở đây.

export type RoleId = string;

/** Trạng thái lượt đọc (plan §4 — Lưu trữ). */
export type ReadingStatus = "drawing" | "drawn" | "interpreting" | "done" | "failed" | "cancelled" | "support";

// ───────────────────────────── Config (schema v1) ─────────────────────────────

/** Giá trị key trả về từ get_config đã bị mask; gửi lại nguyên giá trị mask = giữ key cũ, "" = xoá. */
export const MASK_PREFIX = "••••";
export type JsonMode = "off" | "json_object";
export type RoleLevel = "c_level" | "head" | "manager" | "lead" | "engineer" | "specialist";

/** Nơi lưu API key — người dùng chỉ chọn được `keyring` hoặc `memory`; `file` chỉ để báo key plaintext sót lại. */
export type KeyStorage = "keyring" | "file" | "memory";

/** Nơi duy nhất quản lý nguồn API: base URL + key + cache danh sách model. */
export interface Provider {
  id: string;
  name: string;
  base_url: string;
  api_key: string | null;
  /** Backend tự quyết; chỉ đổi được qua set_secret. */
  key_storage?: KeyStorage;
  models: string[];
  models_fetched_at: string | null;
}

/** Ngoại hình nhân vật pixel. Chuỗi rỗng = tự sinh từ id/cấp bậc. */
export interface Appearance {
  gender: string;
  skin: string;
  hair_style: string;
  hair_color: string;
  outfit: string;
  outfit_color: string;
  eyewear: string;
  accessory: string;
}

/** Một thầy bói (ReaderPersona). `instructions` = lối luận bài, chèn vào system prompt. */
export interface RoleDef {
  id: RoleId;
  name: string;
  title: string;
  level: RoleLevel;
  icon: string;
  color: string;
  description: string;
  instructions: string;
  provider_id: string;
  model: string;
  temperature: number;
  appearance: Appearance;
}

/** Thông tin hiển thị của 1 nhân vật (snapshot trong lượt đọc, hoặc lấy từ config). */
export interface CastMember {
  id: RoleId;
  name: string;
  title: string;
  color: string;
  icon: string;
  model: string;
  level: RoleLevel;
  appearance?: Appearance;
}

export type Animations = "system" | "on" | "off";

export interface TarotSettings {
  reversed_enabled: boolean;
  default_spread: string;
  default_readers: RoleId[];
  chief_reader_id: RoleId;
  animations: Animations;
  /** Chỉ đổi qua `acknowledge_privacy`; save_config bỏ qua trường này. */
  privacy_ack: boolean;
  max_total_tokens: number | null;
}

export interface ModelPrice {
  input_per_m: number;
  cached_per_m: number;
  output_per_m: number;
}
export type Pricing = Record<string, ModelPrice>;

export interface AppConfig {
  schema_version: number;
  providers: Provider[];
  /** Danh sách thầy bói (tên trường giữ `roles` để tái dùng UI). */
  roles: RoleDef[];
  tarot: TarotSettings;
  max_tokens: number | null;
  request_timeout_secs: number;
  stream_idle_timeout_secs: number;
  max_retries: number;
  json_mode: JsonMode;
  max_parallel_calls: number;
  pricing: Pricing;
}

// ───────────────────────────── Lỗi command ─────────────────────────────

export interface CommandError {
  kind:
    | "invalid"
    | "not_implemented"
    | "not_found"
    | "storage"
    | "llm"
    | "internal"
    | "secret_missing"
    | "busy"
    | "persist_failed";
  message: string;
  /** Chỉ có với `secret_missing`: key nào đang thiếu. */
  target?: SecretTarget;
}

export interface ConnectionTestResult { agent: RoleId; model: string; latency_ms: number; reply: string }
export interface ModelList { provider_id: string; models: string[]; fetched_at: string; cached: boolean }

export type HealthStatus =
  | "ok"
  | "quota"
  | "auth"
  | "not_found"
  | "rate_limit"
  | "bad_request"
  | "server"
  | "timeout"
  | "network"
  | "unassigned"
  | "other";

/** Kết quả kiểm tra 1 cặp provider + model (dùng chung cho mọi thầy bói gán cặp đó). */
export interface ModelHealth {
  provider_id: string;
  provider_name: string;
  model: string;
  role_ids: RoleId[];
  status: HealthStatus;
  message: string;
  latency_ms: number | null;
  /** Số dư / hạn mức nếu provider có API (OpenRouter). */
  balance: string | null;
  checked_at: string;
}

// ───────────────────────────── API key — mirror secrets.rs ─────────────────────────────

export type SecretTargetKind = "provider";
export interface SecretTarget { kind: SecretTargetKind; id: string }

export type SecretErrorCode = "keyring_unavailable" | "keyring_locked" | "storage" | "invalid";

export interface SecretTargetStatus {
  target: SecretTarget;
  /** Nơi key đang thực sự nằm; key plaintext chưa migrate khỏi config.json = "file". */
  storage: KeyStorage;
  migrated: boolean;
  has_key: boolean;
  /** Key đáng lẽ phải có nhưng không có (RAM sau khi mở lại app, keyring khoá…). */
  missing: boolean;
  error?: string;
  error_code?: SecretErrorCode;
}

export interface SecretStatusReport {
  targets: SecretTargetStatus[];
  /** File backup/hỏng còn chứa api_key plaintext. */
  leftover_files: string[];
  secrets_file: string;
}

export interface SetSecretResult { storage: KeyStorage; masked: string }

/** Lỗi command liên quan tới key (giữ nguyên `kind` từ backend). */
export interface SecretCommandError {
  kind: string;
  message: string;
  target?: SecretTarget;
}

// ───────────────────────────── Event contract `reading-event` (plan §3) ─────────────────────────────

export const READING_EVENT = "reading-event";

export type ReadingPayload =
  | { type: "reader_started"; reader_id: RoleId }
  | { type: "reader_progress"; reader_id: RoleId; chars: number }
  | { type: "reader_done"; reader_id: RoleId; output: unknown }
  | { type: "reader_failed"; reader_id: RoleId; reason: string }
  | { type: "synthesis_started" }
  | { type: "synthesis_done"; synthesis: unknown }
  | { type: "support_required" }
  | { type: "usage"; tokens: number; cost_usd: number | null }
  | { type: "reading_done" }
  | { type: "cancelled" }
  | { type: "error"; code: string; message: string; retryable: boolean };

/** `seq` tăng đơn điệu theo từng lượt đọc, xuyên suốt mọi attempt. */
export interface ReadingEnvelope {
  reading_id: string;
  attempt_id: number;
  seq: number;
  ts: string;
  payload: ReadingPayload;
}
