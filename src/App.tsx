import { MotionConfig } from "motion/react";
import { Header } from "./components/Header";
import { ReadingPlaceholder } from "./components/ReadingPlaceholder";
import { SettingsView } from "./components/settings/SettingsView";
import { Toasts } from "./components/Toasts";
import { ErrorBoundary } from "./components/ErrorBoundary";
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
              <ReadingPlaceholder />
            </ErrorBoundary>
          </div>
          <div className={tab === "journal" ? "h-full" : "hidden"}>
            <div className="flex h-full items-center justify-center text-sm text-fg-subtle">Nhật ký lượt đọc có ở M2.</div>
          </div>
          <div className={tab === "settings" ? "h-full" : "hidden"}>
            <ErrorBoundary label="Cài đặt">
              <SettingsView />
            </ErrorBoundary>
          </div>
        </main>
        <Toasts />
      </div>
    </MotionConfig>
  );
}
