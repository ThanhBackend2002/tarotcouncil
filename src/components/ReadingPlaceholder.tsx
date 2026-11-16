import { KeyRound, Sparkles } from "lucide-react";
import { useConfig, hasUsableKeys } from "../store/config";
import { useUi } from "../store/ui";
import { Button, Panel } from "./ui";

/** M0: app khởi động + cấu hình provider. Bàn đọc bài thật đến ở M1 (xem plan §1 Milestones). */
export function ReadingPlaceholder() {
  const config = useConfig((s) => s.config);
  const openSettings = useUi((s) => s.openSettings);
  const ready = !!config && config.providers.length > 0 && hasUsableKeys(config);

  return (
    <div className="flex h-full items-center justify-center p-6">
      <Panel className="max-w-lg space-y-4 p-6 text-center">
        <Sparkles className="mx-auto size-8 text-accent-fg" aria-hidden />
        <h1 className="text-lg font-semibold">Bàn đọc bài đang được dựng</h1>
        <p className="text-sm text-fg-muted">
          Bản này mới hoàn tất phần nền (M0): cấu hình provider, model cho từng thầy bói và nơi lưu API key. Tráo bài,
          bốc lá và hội đồng luận giải sẽ có ở mốc M1–M2.
        </p>
        {ready ? (
          <p className="text-sm text-success">Provider và thầy bói mặc định đã sẵn sàng.</p>
        ) : (
          <Button variant="primary" icon={<KeyRound className="size-4" />} onClick={() => openSettings("providers")}>
            Thêm provider & API key
          </Button>
        )}
      </Panel>
    </div>
  );
}
