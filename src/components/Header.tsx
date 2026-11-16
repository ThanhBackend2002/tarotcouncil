import { BookOpen, LifeBuoy, NotebookPen, Settings } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useUi, type Tab } from "../store/ui";
import { cx } from "./ui";

const TABS: { id: Tab; label: string; icon: typeof BookOpen }[] = [
  { id: "reading", label: "Bàn đọc bài", icon: BookOpen },
  { id: "journal", label: "Nhật ký", icon: NotebookPen },
  { id: "settings", label: "Cài đặt", icon: Settings },
];

export function Header() {
  const { tab, setTab, openSupport } = useUi(useShallow((s) => ({ tab: s.tab, setTab: s.setTab, openSupport: s.openSupport })));

  return (
    <header className="flex h-12 shrink-0 items-center gap-6 border-b border-line bg-canvas px-4">
      <div className="flex items-center gap-2">
        <div
          className="grid size-6 place-items-center rounded-control bg-gradient-to-br from-violet-600 to-amber-500 text-[13px] text-white"
          aria-hidden
        >
          ☾
        </div>
        <span className="text-sm font-semibold tracking-tight">TarotCouncil</span>
      </div>

      <nav className="flex items-center gap-1" aria-label="Điều hướng">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            aria-current={tab === id ? "page" : undefined}
            className={cx(
              "inline-flex h-8 items-center gap-1.5 rounded-control px-3 text-sm transition-colors",
              tab === id ? "bg-surface-2 text-fg" : "text-fg-muted hover:bg-surface hover:text-fg-soft",
            )}
          >
            <Icon className="size-4" />
            {label}
          </button>
        ))}
      </nav>

      <div className="ml-auto flex items-center gap-3 text-xs text-fg-muted">
        <span className="hidden md:inline">Chỉ mang tính giải trí</span>
        {/* Plan §5: lối vào hỗ trợ luôn hiển thị trên mọi màn hình. */}
        <button
          type="button"
          onClick={() => openSupport("manual")}
          className="inline-flex h-8 items-center gap-1.5 rounded-control px-3 text-sm text-warn hover:bg-warn-subtle"
        >
          <LifeBuoy className="size-4" />
          Cần hỗ trợ?
        </button>
      </div>
    </header>
  );
}
