// Primitive UI dùng chung.

import type { ButtonHTMLAttributes, ReactNode } from "react";
import { LoaderCircle } from "lucide-react";
import { STATUS_META, TONE_CLASS, type Tone } from "../lib/agents";
import type { ReadingStatus } from "../lib/contracts";

export type Phase = "idle" | ReadingStatus;

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

type Variant = "primary" | "danger" | "ghost" | "outline";

const VARIANT: Record<Variant, string> = {
  primary: "bg-accent text-on-accent hover:bg-accent-hover disabled:bg-accent/40",
  danger: "bg-danger-solid text-on-accent hover:brightness-110 disabled:bg-danger-solid/40",
  ghost: "text-fg-soft hover:bg-surface-2 hover:text-fg",
  outline: "border border-line-strong text-fg-soft hover:bg-surface-2 hover:border-line-heavy",
};

export function Button({
  variant = "outline",
  size = "md",
  loading,
  icon,
  className,
  children,
  disabled,
  title,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: "sm" | "md";
  loading?: boolean;
  icon?: ReactNode;
}) {
  // Nút chỉ có icon: lấy `title` làm tên cho trình đọc màn hình nếu chưa có aria-label.
  const iconOnly = children == null || children === false;
  return (
    <button
      type="button"
      disabled={disabled || loading}
      title={title}
      aria-label={rest["aria-label"] ?? (iconOnly ? title : undefined)}
      aria-busy={loading || undefined}
      className={cx(
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-control font-medium transition-colors select-none",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        "disabled:cursor-not-allowed disabled:opacity-60",
        size === "sm" ? (iconOnly ? "size-7" : "h-7 px-2.5 text-xs") : iconOnly ? "size-9" : "h-9 px-3.5 text-sm",
        VARIANT[variant],
        className,
      )}
      {...rest}
    >
      {loading ? <LoaderCircle className="size-4 animate-spin" aria-hidden /> : icon}
      {children}
    </button>
  );
}

/** Nút chỉ có icon: bắt buộc `label` (vừa là aria-label vừa là tooltip). */
export function IconButton({
  label,
  className,
  children,
  ...rest
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "title"> & { label: string; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-control text-fg-subtle transition-colors hover:bg-surface-2 hover:text-fg",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        "disabled:cursor-not-allowed disabled:opacity-60",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Badge({ tone = "neutral", children, className }: { tone?: Tone; children: ReactNode; className?: string }) {
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium ring-1 ring-inset whitespace-nowrap",
        TONE_CLASS[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

export function StatusBadge({ status }: { status: Phase }) {
  const m = STATUS_META[status];
  return (
    <Badge tone={m.tone}>
      {(status === "drawing" || status === "interpreting") && <span className="size-1.5 animate-pulse rounded-full bg-info-solid" aria-hidden />}
      {m.label}
    </Badge>
  );
}

export function VerdictBadge({ verdict, carried }: { verdict: "APPROVE" | "REJECT" | "ABSTAIN"; carried?: boolean }) {
  if (verdict === "ABSTAIN") return <Badge tone="neutral">RÚT</Badge>;
  return (
    <Badge tone={verdict === "APPROVE" ? "success" : "danger"}>
      {verdict}
      {carried && <span className="opacity-70">· giữ nguyên</span>}
    </Badge>
  );
}

export function Panel({ className, children }: { className?: string; children: ReactNode }) {
  return <section className={cx("rounded-panel border border-line bg-panel", className)}>{children}</section>;
}
