import { useCallback, useEffect, useState } from "react";
import { BadgeCheck, FolderOpen, RefreshCw, Search, ShieldAlert, Trash } from "lucide-react";
import type { ReadingSummary, Verification } from "../../lib/contracts";
import { formatDate } from "../../lib/agents";
import { inTauri, isCommandError, readingIpc } from "../../lib/ipc";
import { spreadOf, useCatalog } from "../../store/catalog";
import { useReading } from "../../store/reading";
import { toast, toastError } from "../../store/toast";
import { useUi } from "../../store/ui";
import { Dialog } from "../Dialog";
import { Badge, Button, Panel, StatusBadge } from "../ui";

/** Nhật ký lượt đọc (thay HistoryView): liệt kê, tìm không dấu (FTS), mở lại, kiểm chứng, xoá. */
export function JournalView({ active }: { active: boolean }) {
  const spreads = useCatalog((s) => s.spreads);
  const loadCatalog = useCatalog((s) => s.load);
  const open = useReading((s) => s.open);
  const current = useReading((s) => s.reading?.id);
  const setTab = useUi((s) => s.setTab);
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<ReadingSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [verified, setVerified] = useState<Record<string, Verification>>({});
  const [confirm, setConfirm] = useState<ReadingSummary | null>(null);

  const refresh = useCallback(async (q: string) => {
    if (!inTauri()) return;
    setLoading(true);
    try {
      setItems(q.trim() ? await readingIpc.search(q) : await readingIpc.list());
    } catch (e) {
      if (isCommandError(e) && e.kind === "index_unavailable") {
        toast("warning", "Chưa tìm kiếm được", "Chỉ mục đang được dựng lại — hiển thị toàn bộ danh sách.");
        setItems(await readingIpc.list());
      } else toastError(e, "Không tải được Nhật ký");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void loadCatalog();
    const t = setTimeout(() => void refresh(query), query ? 250 : 0);
    return () => clearTimeout(t);
  }, [active, query, refresh, loadCatalog]);

  const verify = async (id: string) => {
    try {
      const v = await readingIpc.verify(id);
      setVerified((m) => ({ ...m, [id]: v }));
    } catch (e) {
      toastError(e, "Không kiểm chứng được");
    }
  };

  const remove = async (r: ReadingSummary) => {
    try {
      await readingIpc.remove(r.id);
      setItems((xs) => xs.filter((x) => x.id !== r.id));
      if (current === r.id) useReading.getState().reset();
      toast("success", "Đã xoá lượt đọc");
    } catch (e) {
      toastError(e, "Không xoá được");
    } finally {
      setConfirm(null);
    }
  };

  return (
    <div className="mx-auto h-full max-w-4xl space-y-3 overflow-y-auto p-6">
      <div className="flex items-center gap-3">
        <h1 className="text-lg font-semibold">Nhật ký</h1>
        <div className="relative ml-auto w-72">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-subtle" aria-hidden />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Tìm câu hỏi, lá bài, lời giải… (gõ không dấu được)"
            aria-label="Tìm trong Nhật ký"
            className="h-8 w-full rounded-control border border-line-strong bg-surface pr-2 pl-8 text-sm focus:border-accent focus:outline-none"
          />
        </div>
        <Button size="sm" variant="ghost" icon={<RefreshCw className="size-3.5" />} loading={loading} onClick={() => void refresh(query)} title="Tải lại" />
      </div>

      {items.length === 0 && !loading && (
        <p className="py-12 text-center text-sm text-fg-subtle">{query ? "Không tìm thấy lượt đọc nào." : "Chưa có lượt đọc nào."}</p>
      )}

      <ul className="space-y-2">
        {items.map((r) => {
          const v = verified[r.id];
          return (
            <li key={r.id}>
              <Panel className="space-y-1.5 p-3">
                <div className="flex items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <p className="text-[11px] text-fg-subtle">
                      {formatDate(r.created_at)} · {spreadOf(spreads, r.spread_id)?.name_vi ?? r.spread_id}
                    </p>
                    <p className="font-medium break-words">“{r.question}”</p>
                  </div>
                  <StatusBadge status={r.status} />
                </div>
                <div className="flex flex-wrap gap-1">
                  {r.cards.map(([name, rev], i) => (
                    <Badge key={i} tone={rev ? "warning" : "neutral"}>
                      {name}
                      {rev ? " (ngược)" : ""}
                    </Badge>
                  ))}
                </div>
                {r.summary && <p className="line-clamp-2 text-sm text-fg-muted">{r.summary}</p>}
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  <Button
                    size="sm"
                    icon={<FolderOpen className="size-3.5" />}
                    onClick={() => {
                      void open(r.id);
                      setTab("reading");
                    }}
                  >
                    Mở
                  </Button>
                  <Button size="sm" variant="ghost" icon={<BadgeCheck className="size-3.5" />} onClick={() => void verify(r.id)}>
                    Kiểm chứng
                  </Button>
                  <Button size="sm" variant="ghost" icon={<Trash className="size-3.5" />} onClick={() => setConfirm(r)}>
                    Xoá
                  </Button>
                  {v && (
                    <span className={v.verified ? "text-xs text-success" : "flex items-center gap-1 text-xs text-warn"} title={v.problems.join("\n")}>
                      {!v.verified && <ShieldAlert className="size-3.5" aria-hidden />}
                      {v.label}
                    </span>
                  )}
                </div>
              </Panel>
            </li>
          );
        })}
      </ul>

      <Dialog
        open={!!confirm}
        onClose={() => setConfirm(null)}
        tone="danger"
        title="Xoá lượt đọc?"
        description="Lượt đọc và dòng chỉ mục tìm kiếm tương ứng sẽ bị xoá vĩnh viễn khỏi máy này."
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirm(null)}>
              Huỷ
            </Button>
            <Button variant="danger" onClick={() => confirm && void remove(confirm)}>
              Xoá
            </Button>
          </>
        }
      >
        {confirm && <p className="text-sm break-words text-fg-soft">“{confirm.question}”</p>}
      </Dialog>
    </div>
  );
}
