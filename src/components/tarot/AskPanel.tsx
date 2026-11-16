import { useEffect, useMemo, useState } from "react";
import { KeyRound, Shuffle } from "lucide-react";
import type { RoleId, Spread } from "../../lib/contracts";
import { useCatalog } from "../../store/catalog";
import { hasUsableKeys, useConfig, useResolver } from "../../store/config";
import { useReading } from "../../store/reading";
import { useUi } from "../../store/ui";
import { PixelPortrait } from "../PixelCharacter";
import { Button, Panel, cx } from "../ui";

const MAX_READERS = 4;
const Q_MIN = 3;
const Q_MAX = 500;

/** Sơ đồ thu nhỏ vị trí các lá của kiểu trải. */
function SpreadDiagram({ spread }: { spread: Spread }) {
  return (
    <div className="relative h-14 w-20 shrink-0 rounded-sm bg-violet-950/60" aria-hidden>
      {spread.positions.map((p) => (
        <span
          key={p.index}
          className="absolute h-[18px] w-[11px] -translate-x-1/2 -translate-y-1/2 rounded-[2px] border border-amber-300/70 bg-violet-800"
          style={{ left: `${8 + p.x * 84}%`, top: `${12 + p.y * 76}%`, rotate: `${p.rotation}deg` }}
        />
      ))}
    </div>
  );
}

export function AskPanel({ animate }: { animate: boolean }) {
  const config = useConfig((s) => s.config);
  const spreads = useCatalog((s) => s.spreads);
  const start = useReading((s) => s.start);
  const busy = useReading((s) => s.busy);
  const openSettings = useUi((s) => s.openSettings);
  const resolve = useResolver();

  const [question, setQuestion] = useState("");
  const [spreadId, setSpreadId] = useState<string>("");
  const [readers, setReaders] = useState<RoleId[]>([]);

  // Mặc định theo Cài đặt → Đọc bài (chỉ áp lần đầu có config).
  useEffect(() => {
    if (!config) return;
    setSpreadId((s) => s || config.tarot.default_spread);
    setReaders((r) => (r.length ? r : config.tarot.default_readers));
  }, [config]);

  const candidates = useMemo(() => (config ? config.roles.filter((r) => r.id !== config.tarot.chief_reader_id) : []), [config]);
  const q = question.trim();
  const qOk = q.length >= Q_MIN && q.length <= Q_MAX;
  const ready = !!config && config.providers.length > 0 && hasUsableKeys(config, readers);
  const canStart = qOk && !!spreadId && readers.length > 0 && !busy;

  const toggle = (id: RoleId) =>
    setReaders((r) => (r.includes(id) ? r.filter((x) => x !== id) : r.length >= MAX_READERS ? r : [...r, id]));

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-6">
      <header className="text-center">
        <p className="text-xs tracking-[0.3em] text-accent-fg uppercase">Hội đồng thầy bói</p>
        <h1 className="mt-1 text-2xl font-semibold">Bạn muốn hỏi điều gì?</h1>
        <p className="mt-1 text-sm text-fg-muted">Tĩnh tâm một chút, rồi viết câu hỏi của bạn. Lượt đọc chỉ mang tính giải trí.</p>
      </header>

      {!ready && config && (
        <p className="flex flex-wrap items-center gap-2 rounded-control border border-warn-line bg-warn-subtle px-3 py-2 text-sm text-warn">
          Vẫn bốc bài được, nhưng để hội đồng luận giải cần provider + API key cho các thầy bói đã chọn.
          <Button size="sm" icon={<KeyRound className="size-3.5" />} onClick={() => openSettings("providers")}>
            Cài đặt provider
          </Button>
        </p>
      )}

      <Panel className="space-y-2 p-4">
        <label htmlFor="question" className="text-sm font-medium">
          Câu hỏi
        </label>
        <textarea
          id="question"
          value={question}
          onChange={(e) => setQuestion(e.target.value.slice(0, Q_MAX))}
          rows={3}
          placeholder="Ví dụ: Tôi nên chuẩn bị gì cho công việc mới?"
          aria-describedby="question-hint"
          className="w-full resize-none rounded-control border border-line-strong bg-surface px-3 py-2 text-sm focus:border-accent focus:outline-none"
        />
        <p id="question-hint" className={cx("text-right text-[11px]", qOk || !q ? "text-fg-subtle" : "text-warn")}>
          {q.length}/{Q_MAX} {q && q.length < Q_MIN ? `· cần ít nhất ${Q_MIN} ký tự` : ""}
        </p>
      </Panel>

      <Panel className="p-4">
        <fieldset>
          <legend className="mb-2 text-sm font-medium">Kiểu trải</legend>
          <div className="grid gap-2 sm:grid-cols-2">
            {spreads.map((s) => (
              <label
                key={s.id}
                className={cx(
                  "flex cursor-pointer items-center gap-3 rounded-control border p-2.5 transition-colors",
                  spreadId === s.id ? "border-accent bg-accent-subtle" : "border-line hover:border-line-strong",
                )}
              >
                <input type="radio" name="spread" value={s.id} checked={spreadId === s.id} onChange={() => setSpreadId(s.id)} className="sr-only" />
                <SpreadDiagram spread={s} />
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{s.name_vi}</span>
                  <span className="block text-xs text-fg-muted">{s.description_vi}</span>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
      </Panel>

      <Panel className="p-4">
        <fieldset>
          <legend className="mb-2 text-sm font-medium">
            Thầy bói ({readers.length}/{MAX_READERS}) <span className="font-normal text-fg-subtle">· Trưởng thầy luôn tham gia tổng hợp</span>
          </legend>
          <div className="flex flex-wrap gap-2">
            {candidates.map((r) => {
              const on = readers.includes(r.id);
              return (
                <button
                  key={r.id}
                  type="button"
                  aria-pressed={on}
                  disabled={!on && readers.length >= MAX_READERS}
                  onClick={() => toggle(r.id)}
                  title={r.description}
                  className={cx(
                    "flex items-center gap-2 rounded-full py-1 pr-3 pl-1 text-sm ring-1 transition-colors disabled:opacity-40",
                    on ? "bg-accent-subtle ring-accent" : "ring-line-strong hover:bg-surface-2",
                  )}
                >
                  <PixelPortrait meta={resolve(r.id)} size={26} />
                  {r.name}
                </button>
              );
            })}
          </div>
        </fieldset>
      </Panel>

      <div className="flex justify-center">
        <Button
          variant="primary"
          icon={<Shuffle className="size-4" />}
          loading={busy}
          disabled={!canStart}
          onClick={() => void start(q, spreadId, readers, animate)}
        >
          Tráo bài
        </Button>
      </div>
    </div>
  );
}
