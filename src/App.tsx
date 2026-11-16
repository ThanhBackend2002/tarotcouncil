import { MotionConfig } from "motion/react";
import { Header } from "./components/Header";
import { SettingsView } from "./components/settings/SettingsView";
import { Toasts } from "./components/Toasts";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { JournalView } from "./components/tarot/JournalView";
import { ReadingView } from "./components/tarot/ReadingView";
import { SupportScreen } from "./components/tarot/SupportScreen";
import { useUi } from "./store/ui";

export default function App() {
  const tab = useUi((s) => s.tab);
  const motion = useUi((s) => s.reducedMotion);
  return (
    <MotionConfig reducedMotion={motion === "on" ? "always" : motion === "off" ? "never" : "user"}>
      <div className="flex h-full flex-col bg-canvas text-fg">
        <Header />
        {/* Giữ các view mounted để không mất scroll/state khi đổi tab. */}
        <main className="relative min-h-0 flex-1">
          <div className={tab === "reading" ? "h-full" : "hidden"}>
            <ErrorBoundary label="Bàn đọc bài">
              <ReadingView />
            </ErrorBoundary>
          </div>
          <div className={tab === "journal" ? "h-full" : "hidden"}>
            <ErrorBoundary label="Nhật ký">
              <JournalView active={tab === "journal"} />
            </ErrorBoundary>
          </div>
          <div className={tab === "settings" ? "h-full" : "hidden"}>
            <ErrorBoundary label="Cài đặt">
              <SettingsView />
            </ErrorBoundary>
          </div>
        </main>
        <SupportScreen />
        <Toasts />
      </div>
    </MotionConfig>
  );
}
