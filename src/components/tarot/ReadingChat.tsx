import { memo, useEffect, useRef } from "react";
import { LoaderCircle, TriangleAlert } from "lucide-react";
import type { RoleResolver } from "../../lib/agents";
import type { Reading, RoleId, Spread, Synthesis } from "../../lib/contracts";
import type { Phase, ReaderView } from "../../lib/readingSync";
import { useResolver } from "../../store/config";
import { useLanded } from "../../store/flight";
import { useReading } from "../../store/reading";
import { PixelPortrait } from "../PixelCharacter";
import { Badge, Panel, cx } from "../ui";

type Msg =
  | { key: string; kind: "system"; text: string }
  | { key: string; kind: "reader"; id: RoleId; view: ReaderView | undefined }
  | { key: string; kind: "chief"; id: RoleId; running: boolean; synthesis: Synthesis | null }
  | { key: string; kind: "outcome"; text: string; tone: "success" | "warning" | "neutral" };

const TONE_LABEL = { hopeful: ["Lạc quan", "success"], cautionary: ["Thận trọng", "warning"], neutral: ["Trung tính", "neutral"] } as const;

function build(r: Reading, phase: Phase, readers: Record<RoleId, ReaderView>, synthesisRunning: boolean, synthesis: Synthesis | null, spread: Spread): Msg[] {
  const out: Msg[] = [{ key: "q", kind: "system", text: `Câu hỏi: “${r.question}” · ${spread.name_vi}` }];
  r.draws.forEach((d) =>
    out.push({ key: `d${d.position}`, kind: "system", text: `Bốc lá ${d.position + 1} · ${d.position_label_vi}: ${d.name_vi}${d.reversed ? " (ngược)" : ""}` }),
  );
  const talked = phase === "interpreting" || phase === "done" || phase === "error" || phase === "cancelled";
  if (talked) {
    r.reader_ids.forEach((id) => out.push({ key: `r-${id}`, kind: "reader", id, view: readers[id] }));
    if (synthesisRunning || synthesis) out.push({ key: "chief", kind: "chief", id: r.chief_reader_id, running: synthesisRunning, synthesis });
  }
  if (phase === "done") out.push({ key: "end", kind: "outcome", text: "Hội đồng đã luận xong.", tone: "success" });
  if (phase === "cancelled") out.push({ key: "end", kind: "outcome", text: "Đã dừng luận giải — các lá đã bốc vẫn được giữ.", tone: "neutral" });
  if (phase === "support") out.push({ key: "end", kind: "outcome", text: "Hội đồng tạm dừng và không hiển thị lời giải.", tone: "warning" });
  return out;
}

/** Khung hội thoại của hội đồng (thay Timeline của PlanCouncil). */
export const ReadingChat = memo(function ReadingChat({ spread }: { spread: Spread }) {
  const reading = useReading((s) => s.reading);
  const phase = useReading((s) => s.phase);
  const readers = useReading((s) => s.readers);
  const synthesisRunning = useReading((s) => s.synthesisRunning);
  const synthesis = useReading((s) => s.synthesis);
  const resolve = useResolver(reading?.cast);
  const landed = useLanded(reading?.draws ?? []);
  const list = useRef<HTMLOListElement>(null);

  const msgs = reading ? build({ ...reading, draws: landed }, phase, readers, synthesisRunning, synthesis, spread) : [];
  const sig = msgs.map((m) => (m.kind === "reader" ? `${m.key}:${m.view?.status}` : m.key)).join("|");
  // Chỉ cuộn TRONG khung hội thoại — scrollIntoView sẽ kéo cả cột trái (gây giật khi bốc lá).
  useEffect(() => {
    const el = list.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [sig]);

  if (!reading) return null;
  return (
    <Panel className="flex min-h-[300px] flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2 text-xs font-medium text-fg-muted">
        Hội thoại · Hội đồng thầy bói
        <span className="ml-auto font-normal text-fg-subtle">
          {reading.draws.length}/{spread.positions.length} lá · {reading.reader_ids.length} thầy bói
        </span>
      </div>
      <ol ref={list} className="flex-1 space-y-3 overflow-y-auto p-3" aria-live="polite" aria-label="Hội thoại">
        {msgs.map((m) => (
          <li key={m.key}>
            <Message m={m} reading={reading} resolve={resolve} />
          </li>
        ))}
      </ol>
    </Panel>
  );
});

function Avatar({ id, resolve }: { id: RoleId; resolve: RoleResolver }) {
  return (
    <div className="shrink-0 rounded-sm bg-[#1d1a2c] p-0.5">
      <PixelPortrait meta={resolve(id)} size={32} />
    </div>
  );
}

function Message({ m, reading, resolve }: { m: Msg; reading: Reading; resolve: RoleResolver }) {
  if (m.kind === "system") return <p className="text-center text-[11px] text-fg-subtle">— {m.text} —</p>;
  if (m.kind === "outcome") {
    return (
      <p className="text-center">
        <Badge tone={m.tone}>{m.text}</Badge>
      </p>
    );
  }
  const meta = resolve(m.id);
  const head = (extra?: React.ReactNode) => (
    <div className="flex items-center gap-2 text-xs">
      <span className="font-semibold" style={{ color: meta.color }}>
        {m.kind === "chief" ? `♛ ${meta.name}` : meta.name}
      </span>
      {meta.title && <span className="text-fg-subtle">{meta.title}</span>}
      {extra}
    </div>
  );

  if (m.kind === "chief") {
    return (
      <div className="flex gap-2">
        <Avatar id={m.id} resolve={resolve} />
        <div className="min-w-0 flex-1 space-y-1 rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2">
          {head()}
          {m.running || !m.synthesis ? (
            <p className="flex items-center gap-1.5 text-sm text-fg-muted">
              <LoaderCircle className="size-3.5 animate-spin" aria-hidden /> đang gom lời cả hội đồng…
            </p>
          ) : (
            <>
              <p className="text-sm leading-relaxed">{m.synthesis.summary}</p>
              {m.synthesis.advice.length > 0 && (
                <ul className="list-disc pl-5 text-sm text-fg-soft">
                  {m.synthesis.advice.map((a, i) => (
                    <li key={i}>{a}</li>
                  ))}
                </ul>
              )}
            </>
          )}
        </div>
      </div>
    );
  }

  const v = m.view;
  const out = v?.result?.output;
  return (
    <div className="flex gap-2">
      <Avatar id={m.id} resolve={resolve} />
      <div className={cx("min-w-0 flex-1 space-y-1.5 rounded-md border px-3 py-2", v?.status === "failed" ? "border-warn-line bg-warn-subtle" : "border-line bg-surface")}>
        {head(out ? <Badge tone={TONE_LABEL[out.tone][1]}>{TONE_LABEL[out.tone][0]}</Badge> : null)}
        {!v || v.status === "waiting" ? (
          <p className="text-sm text-fg-subtle">đang chờ tới lượt…</p>
        ) : v.status === "thinking" ? (
          <p className="flex items-center gap-1.5 text-sm text-fg-muted">
            <LoaderCircle className="size-3.5 animate-spin" aria-hidden /> đang suy ngẫm…{v.chars ? ` (${v.chars.toLocaleString("vi-VN")} ký tự)` : ""}
          </p>
        ) : v.status === "failed" ? (
          <p className="flex gap-1.5 text-sm text-warn">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden />
            Không luận được{v.reason ? ` — ${v.reason}` : ""}. Nghĩa gốc của các lá vẫn có ở bàn trải bên phải.
          </p>
        ) : out ? (
          <>
            <ul className="space-y-1 text-sm">
              {out.positions.map((p) => {
                const d = reading.draws.find((x) => x.position === p.position);
                return (
                  <li key={p.position} className="leading-relaxed">
                    <span className="font-medium text-fg-soft">
                      {d ? `${d.position_label_vi} · ${d.name_vi}${d.reversed ? " (ngược)" : ""}` : `Vị trí ${p.position + 1}`}:
                    </span>{" "}
                    {p.interpretation}
                  </li>
                );
              })}
            </ul>
            <p className="border-t border-line pt-1.5 text-sm leading-relaxed text-fg-soft">{out.overall}</p>
            {v.result?.warnings.map((w) => (
              <p key={w} className="text-[11px] text-fg-subtle">
                ⚑ {w}
              </p>
            ))}
          </>
        ) : null}
      </div>
    </div>
  );
}
