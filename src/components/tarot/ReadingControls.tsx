import { CircleStop, Sparkles, SquarePlus, TriangleAlert } from "lucide-react";
import type { ReadingStatus, Spread } from "../../lib/contracts";
import { formatTokens } from "../../lib/agents";
import { estimateReading } from "../../lib/estimate";
import type { Phase } from "../../lib/readingSync";
import { useConfig } from "../../store/config";
import { useReading } from "../../store/reading";
import { Button, Panel, StatusBadge } from "../ui";

const SAFETY_LABEL = { medical: "sức khoẻ", financial: "tài chính", legal: "pháp lý", self_harm: "" } as const;

/** `phase` cập nhật theo event (nhanh hơn `reading.status` trong bản ghi đã nạp). */
const PHASE_STATUS: Partial<Record<Phase, ReadingStatus>> = {
  interpreting: "interpreting",
  done: "done",
  error: "failed",
  cancelled: "cancelled",
  support: "support",
};

/** Thanh điều khiển lượt đọc (vị trí của PromptPanel/ProgressBar trong PlanCouncil). */
export function ReadingControls({ spread }: { spread: Spread }) {
  const s = useReading();
  const config = useConfig((c) => c.config);
  const reading = s.reading;
  if (!reading) return null;

  const full = reading.draws.length === spread.positions.length;
  const canInterpret = full && (s.phase === "drawing" || s.phase === "error" || s.phase === "cancelled");
  const interpreting = s.phase === "interpreting";
  const flags = reading.safety_flags.filter((f) => f !== "self_harm");
  const estimate = canInterpret && config && !s.usage ? estimateReading(config, spread.positions.length, reading.reader_ids, reading.chief_reader_id) : null;

  return (
    <Panel className="space-y-2 p-3">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="text-[11px] tracking-wide text-fg-subtle uppercase">{spread.name_vi}</p>
          <p className="font-medium break-words">“{reading.question}”</p>
        </div>
        <StatusBadge status={PHASE_STATUS[s.phase] ?? reading.status} />
      </div>
      {flags.length > 0 && (
        <p className="text-xs text-warn">
          Câu hỏi liên quan tới {flags.map((f) => SAFETY_LABEL[f]).join(", ")} — lời giải không thay cho lời khuyên chuyên môn.
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {canInterpret && (
          <Button variant="primary" icon={<Sparkles className="size-4" />} loading={s.busy} onClick={() => void s.interpret()}>
            {s.phase === "drawing" ? "Mời hội đồng luận giải" : "Luận giải lại"}
          </Button>
        )}
        {interpreting && (
          <Button variant="danger" icon={<CircleStop className="size-4" />} onClick={() => void s.cancel()}>
            Dừng
          </Button>
        )}
        {!interpreting && (
          <Button variant="ghost" icon={<SquarePlus className="size-4" />} onClick={s.ask}>
            Lượt mới
          </Button>
        )}
        {estimate && (
          <span className="ml-auto text-[11px] text-fg-subtle tabular-nums" title="Ước lượng tĩnh — chi phí thật có thể cao hơn khi thử lại hoặc chạy song song">
            Ước tính ~{formatTokens(estimate.tokens)} token{estimate.costUsd != null ? ` · ~$${estimate.costUsd.toFixed(4)}` : ""}
          </span>
        )}
        {s.usage && (
          <span className="ml-auto text-[11px] text-fg-subtle tabular-nums" title="Token thực tế của lượt đọc">
            {formatTokens(s.usage.tokens)} token{s.usage.cost_usd != null ? ` · $${s.usage.cost_usd.toFixed(4)}` : ""}
          </span>
        )}
      </div>
      {s.error && (
        <p className="flex gap-1.5 rounded-control bg-warn-subtle p-2 text-xs text-warn" role="alert">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden />
          {s.error.message}
        </p>
      )}
    </Panel>
  );
}
