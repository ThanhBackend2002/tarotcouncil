import { useState } from "react";
import { ShieldCheck } from "lucide-react";
import { ipc } from "../../lib/ipc";
import { useConfig } from "../../store/config";
import { useReading } from "../../store/reading";
import { toastError } from "../../store/toast";
import { Dialog } from "../Dialog";
import { Button } from "../ui";

function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** Plan §3: lần đầu luận giải — câu hỏi + các lá sẽ gửi tới provider; lịch sử lưu cục bộ, không mã hoá. */
export function PrivacyNotice() {
  const open = useReading((s) => s.privacyPrompt);
  const setOpen = useReading((s) => s.setPrivacyPrompt);
  const interpret = useReading((s) => s.interpret);
  const reading = useReading((s) => s.reading);
  const config = useConfig((s) => s.config);
  const [busy, setBusy] = useState(false);

  const ids = reading ? [...reading.reader_ids, reading.chief_reader_id] : [];
  const endpoints = [
    ...new Set(
      ids
        .map((id) => config?.roles.find((r) => r.id === id))
        .map((r) => config?.providers.find((p) => p.id === r?.provider_id))
        .filter((p) => !!p)
        .map((p) => `${p!.name} (${host(p!.base_url)})`),
    ),
  ];

  const agree = async () => {
    setBusy(true);
    try {
      const cfg = await ipc.acknowledgePrivacy();
      useConfig.setState({ config: cfg });
      setOpen(false);
      await interpret();
    } catch (e) {
      toastError(e, "Không lưu được lựa chọn");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={() => setOpen(false)}
      title={
        <span className="flex items-center gap-2">
          <ShieldCheck className="size-4 text-accent-fg" aria-hidden /> Trước khi hội đồng luận giải
        </span>
      }
      footer={
        <>
          <Button variant="ghost" onClick={() => setOpen(false)}>
            Huỷ
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void agree()}>
            Đồng ý
          </Button>
        </>
      }
    >
      <ul className="list-disc space-y-1.5 pl-5 text-sm text-fg-soft">
        <li>
          Câu hỏi và các lá đã bốc sẽ được gửi tới provider:{" "}
          <b className="text-fg">{endpoints.length ? endpoints.join(", ") : "provider bạn đã cấu hình"}</b>.
        </li>
        <li>Lịch sử lượt đọc được lưu cục bộ trên máy này và không được mã hoá.</li>
        <li>API key không bao giờ rời khỏi phần lõi của app; giao diện không đọc được key.</li>
        <li>Kết quả chỉ mang tính giải trí, không thay cho lời khuyên y tế, tài chính hay pháp lý.</li>
      </ul>
    </Dialog>
  );
}
