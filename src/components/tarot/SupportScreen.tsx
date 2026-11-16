import { useEffect, useState } from "react";
import { Heart, Phone } from "lucide-react";
import type { SupportInfo } from "../../lib/contracts";
import { inTauri, readingIpc } from "../../lib/ipc";
import { useReading } from "../../store/reading";
import { useUi } from "../../store/ui";
import { Dialog } from "../Dialog";
import { Button } from "../ui";

/**
 * Màn hỗ trợ (plan §5). Mở được từ mọi màn hình ("Cần hỗ trợ?"), khi câu hỏi bị chặn, hoặc khi
 * mô hình báo khủng hoảng. Không có nút "vẫn đọc với câu này". Số đường dây chỉ hiện khi có trong
 * `support.json` với `verified_at` chưa quá 12 tháng.
 */
export function SupportScreen() {
  const mode = useUi((s) => s.support);
  const close = useUi((s) => s.closeSupport);
  const ask = useReading((s) => s.ask);
  const [info, setInfo] = useState<SupportInfo | null>(null);

  useEffect(() => {
    if (mode && !info && inTauri()) void readingIpc.supportInfo().then(setInfo, () => setInfo({ entries: [], hidden_stale: 0 }));
  }, [mode, info]);

  const footer =
    mode === "blocked" ? (
      <Button variant="primary" onClick={close}>
        Quay lại sửa câu hỏi
      </Button>
    ) : mode === "crisis" ? (
      <>
        <Button variant="ghost" onClick={close}>
          Đóng
        </Button>
        <Button
          onClick={() => {
            close();
            ask();
          }}
        >
          Về trang đặt câu hỏi
        </Button>
      </>
    ) : (
      <Button onClick={close}>Đóng</Button>
    );

  return (
    <Dialog
      open={!!mode}
      onClose={close}
      tone="warning"
      title={
        <span className="flex items-center gap-2">
          <Heart className="size-4 text-rose-400" aria-hidden /> Bạn không phải một mình
        </span>
      }
      footer={footer}
      className="max-w-lg"
    >
      <div className="space-y-3 text-sm leading-relaxed text-fg-soft">
        {mode === "blocked" && (
          <p>
            Câu hỏi của bạn nhắc tới những điều có vẻ rất nặng nề. Hội đồng sẽ không bốc bài cho câu hỏi này — điều bạn đang
            trải qua xứng đáng được một con người thật lắng nghe.
          </p>
        )}
        {mode === "crisis" && (
          <p>
            Hội đồng dừng luận giải vì nhận thấy bạn có thể đang gặp khó khăn lớn. Không có lời giải nào được hiển thị. Cảm
            xúc của bạn là thật và quan trọng.
          </p>
        )}
        <p className="rounded-control border border-danger-line bg-danger-subtle p-3 text-danger">
          <b>Nếu bạn đang gặp nguy hiểm trước mắt</b>, hãy gọi ngay số khẩn cấp (cấp cứu / công an) tại nơi bạn đang ở, hoặc
          nhờ người ở gần đưa bạn tới cơ sở y tế gần nhất.
        </p>
        <p>
          Hãy nhắn tin hoặc gọi cho một người bạn tin tưởng — bạn bè, người thân, thầy cô hay đồng nghiệp — và nói với họ
          rằng bạn đang không ổn. Bạn không cần phải giải thích mọi thứ ngay.
        </p>
        {info && info.entries.length > 0 && (
          <div>
            <h3 className="mb-1 text-xs font-medium text-fg-muted uppercase">Đường dây hỗ trợ đã xác minh</h3>
            <ul className="space-y-1.5">
              {info.entries.map((e) => (
                <li key={`${e.country}-${e.name}`} className="flex items-start gap-2">
                  <Phone className="mt-0.5 size-3.5 shrink-0" aria-hidden />
                  <span>
                    <b>{e.name}</b> ({e.country}){e.phone && <> · {e.phone}</>}
                    {e.hours && <> · {e.hours}</>}
                    {e.url && <> · {e.url}</>}
                    <span className="block text-[11px] text-fg-subtle">Xác minh ngày {e.verified_at}</span>
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Dialog>
  );
}
