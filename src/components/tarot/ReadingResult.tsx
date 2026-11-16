import { CircleStop, LoaderCircle, Sparkles, SquarePlus, TriangleAlert } from "lucide-react";
import type { Card, Draw, ReadingStatus, Spread } from "../../lib/contracts";
import type { Phase } from "../../lib/readingSync";
import { formatTokens } from "../../lib/agents";
import { estimateReading } from "../../lib/estimate";
import { cardImage } from "../../lib/cardArt";
import { useCatalog } from "../../store/catalog";
import { useConfig, useResolver } from "../../store/config";
import { useReading } from "../../store/reading";
import { Badge, Button, Panel, StatusBadge, cx } from "../ui";

const SAFETY_LABEL = { medical: "sức khoẻ", financial: "tài chính", legal: "pháp lý", self_harm: "" } as const;

/** `phase` cập nhật theo event (nhanh hơn `reading.status` trong bản ghi đã nạp). */
const PHASE_STATUS: Partial<Record<Phase, ReadingStatus>> = {
  interpreting: "interpreting",
  done: "done",
  error: "failed",
  cancelled: "cancelled",
  support: "support",
};

function StaticMeaning({ card, draw }: { card: Card | undefined; draw: Draw }) {
  if (!card) return null;
  const kw = draw.reversed ? card.keywords_reversed : card.keywords_upright;
  return (
    <div className="space-y-1">
      <p className="text-xs text-fg-subtle">Nghĩa gốc · {kw.join(" · ")}</p>
      <p className="text-sm leading-relaxed text-fg-soft">{draw.reversed ? card.meaning_reversed : card.meaning_upright}</p>
    </div>
  );
}

/** Trạng thái từng thầy bói (thay Timeline của PlanCouncil). */
function ReadingTimeline() {
  const reading = useReading((s) => s.reading)!;
  const readers = useReading((s) => s.readers);
  const synthesisRunning = useReading((s) => s.synthesisRunning);
  const synthesis = useReading((s) => s.synthesis);
  const resolve = useResolver(reading.cast);
  const rows = [
    ...reading.reader_ids.map((id) => {
      const v = readers[id];
      const text =
        v?.status === "thinking"
          ? `đang suy ngẫm${v.chars ? ` (${v.chars.toLocaleString("vi-VN")} ký tự)` : "…"}`
          : v?.status === "done"
            ? "đã luận xong"
            : v?.status === "failed"
              ? `không luận được — ${v.reason ?? ""}`
              : "đang chờ";
      return { id, text, tone: v?.status };
    }),
    {
      id: reading.chief_reader_id,
      text: synthesisRunning ? "đang tổng hợp…" : synthesis ? "đã tổng hợp" : "chờ hội đồng",
      tone: synthesisRunning ? "thinking" : synthesis ? "done" : undefined,
    },
  ];
  return (
    <ul className="space-y-1 text-xs" aria-label="Tiến trình hội đồng">
      {rows.map((r) => {
        const m = resolve(r.id);
        return (
          <li key={r.id} className="flex items-center gap-2">
            <span className="size-2 shrink-0 rounded-full" style={{ background: m.color }} aria-hidden />
            <span className="font-medium text-fg-soft">{m.name}</span>
            <span className={cx("min-w-0 truncate", r.tone === "failed" ? "text-warn" : r.tone === "done" ? "text-success" : "text-fg-muted")}>
              {r.tone === "thinking" && <LoaderCircle className="mr-1 inline size-3 animate-spin" aria-hidden />}
              {r.text}
            </span>
          </li>
        );
      })}
    </ul>
  );
}

export function ReadingResult({ spread }: { spread: Spread }) {
  const s = useReading();
  const byId = useCatalog((c) => c.byId);
  const config = useConfig((c) => c.config);
  const reading = s.reading;
  const resolve = useResolver(reading?.cast);
  if (!reading) return null;

  const full = reading.draws.length === spread.positions.length;
  const canInterpret = full && (s.phase === "drawing" || s.phase === "error" || s.phase === "cancelled");
  const interpreting = s.phase === "interpreting";
  const outputs = reading.reader_ids.map((id) => ({ id, view: s.readers[id] }));
  const flags = reading.safety_flags.filter((f) => f !== "self_harm");
  const estimate = canInterpret && config && !s.usage ? estimateReading(config, spread.positions.length, reading.reader_ids, reading.chief_reader_id) : null;

  return (
    <div className="space-y-3" aria-live="polite" aria-busy={interpreting}>
      <Panel className="space-y-2 p-4">
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <h2 className="text-xs tracking-wide text-fg-subtle uppercase">{spread.name_vi}</h2>
            <p className="mt-0.5 font-medium break-words">“{reading.question}”</p>
          </div>
          <StatusBadge status={PHASE_STATUS[s.phase] ?? reading.status} />
        </div>
        {flags.length > 0 && (
          <p className="text-xs text-warn">
            Câu hỏi liên quan tới {flags.map((f) => SAFETY_LABEL[f]).join(", ")} — lời giải sẽ không thay cho lời khuyên chuyên môn.
          </p>
        )}
        <div className="flex flex-wrap gap-2 pt-1">
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
            <span
              className="ml-auto self-center text-[11px] text-fg-subtle tabular-nums"
              title="Ước lượng tĩnh — chi phí thật có thể cao hơn khi thử lại hoặc chạy song song"
            >
              Ước tính ~{formatTokens(estimate.tokens)} token{estimate.costUsd != null ? ` · ~$${estimate.costUsd.toFixed(4)}` : ""}
            </span>
          )}
          {s.usage && (
            <span className="ml-auto self-center text-[11px] text-fg-subtle tabular-nums" title="Token thực tế của lượt đọc">
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
        {(interpreting || s.phase === "done" || s.phase === "error") && <ReadingTimeline />}
      </Panel>

      {s.synthesis && (
        <Panel className="space-y-2 border-amber-500/30 p-4">
          <h3 className="flex items-center gap-1.5 text-sm font-semibold text-amber-300">
            <span aria-hidden>♛</span> Lời Trưởng Thầy
          </h3>
          <p className="text-sm leading-relaxed">{s.synthesis.summary}</p>
          {(
            [
              ["Hội đồng đồng thuận", s.synthesis.agreements],
              ["Các cách hiểu khác nhau", s.synthesis.divergences],
              ["Lời khuyên", s.synthesis.advice],
            ] as const
          ).map(([title, items]) =>
            items.length ? (
              <div key={title}>
                <h4 className="text-xs font-medium text-fg-muted">{title}</h4>
                <ul className="mt-0.5 list-disc space-y-0.5 pl-5 text-sm text-fg-soft">
                  {items.map((x, i) => (
                    <li key={i}>{x}</li>
                  ))}
                </ul>
              </div>
            ) : null,
          )}
          <p className="border-t border-line pt-2 text-[11px] text-fg-subtle italic">{s.synthesis.disclaimer}</p>
        </Panel>
      )}

      {reading.draws.map((d) => {
        const card = byId.get(d.card_id);
        const pos = spread.positions[d.position];
        const done = outputs.filter((o) => o.view?.result?.output);
        return (
          <Panel key={d.position} className="p-3">
            <article aria-label={`${d.position_label_vi}: ${d.name_vi}`} className="flex gap-3">
              <img
                src={cardImage(d.art_key)}
                alt=""
                className={cx("h-24 shrink-0 rounded-sm object-cover shadow", d.reversed && "rotate-180")}
                style={{ aspectRatio: "35 / 60" }}
              />
              <div className="min-w-0 flex-1 space-y-2">
                <div>
                  <p className="text-[11px] tracking-wide text-fg-subtle uppercase">
                    {d.position + 1}. {d.position_label_vi}
                    {pos && <span className="normal-case"> — {pos.meaning_hint}</span>}
                  </p>
                  <h3 className="font-semibold">
                    {d.name_vi} <span className="text-xs font-normal text-fg-muted">({d.name_en})</span>{" "}
                    <Badge tone={d.reversed ? "warning" : "success"}>{d.reversed ? "Ngược" : "Xuôi"}</Badge>
                  </h3>
                </div>
                {done.length > 0 ? (
                  done.map(({ id, view }) => {
                    const p = view!.result!.output!.positions.find((x) => x.position === d.position);
                    if (!p) return null;
                    const m = resolve(id);
                    return (
                      <p key={id} className="text-sm leading-relaxed">
                        <span className="font-medium" style={{ color: m.color }}>
                          {m.name}:
                        </span>{" "}
                        {p.interpretation}
                      </p>
                    );
                  })
                ) : (
                  <StaticMeaning card={card} draw={d} />
                )}
              </div>
            </article>
          </Panel>
        );
      })}

      {outputs.some((o) => o.view?.result?.output) && (
        <Panel className="space-y-2 p-4">
          <h3 className="text-sm font-semibold">Tổng quan từng thầy bói</h3>
          {outputs.map(({ id, view }) => {
            const m = resolve(id);
            const out = view?.result?.output;
            return (
              <div key={id} className="text-sm">
                <span className="font-medium" style={{ color: m.color }}>
                  {m.name}
                </span>
                {out ? (
                  <p className="leading-relaxed text-fg-soft">{out.overall}</p>
                ) : (
                  <p className="text-xs text-warn">{view?.reason ?? "Chưa có lời giải"} — xem nghĩa gốc của các lá ở trên.</p>
                )}
                {view?.result?.warnings.map((w) => (
                  <p key={w} className="text-[11px] text-fg-subtle">
                    ⚑ {w}
                  </p>
                ))}
              </div>
            );
          })}
        </Panel>
      )}
      {reading.draws.length === 0 && <p className="px-1 text-sm text-fg-subtle">Chọn lá từ quạt bài bên dưới để bắt đầu.</p>}
    </div>
  );
}
