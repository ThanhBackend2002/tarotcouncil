// Cầu nối event Rust → store. Gọi đúng 1 lần trong main.tsx (ngoài React để tránh
// StrictMode đăng ký listener 2 lần).
//
// Plan §3 bước 0: listener `reading-event` được đăng ký MỘT lần khi app khởi động, trước mọi
// command. M0 chưa có lượt đọc nên chỉ log; M1 chuyển event vào store reading.

import { listen } from "@tauri-apps/api/event";
import { READING_EVENT, type ReadingEnvelope } from "./contracts";
import { inTauri } from "./ipc";
import { useConfig } from "../store/config";
import { toast } from "../store/toast";

let started = false;

export async function initBridge(): Promise<void> {
  if (started) return;
  started = true;

  if (!inTauri()) {
    toast("warning", "Đang chạy ngoài Tauri", "Dùng `npm run tauri dev` để kết nối backend Rust.");
    return;
  }

  await listen<ReadingEnvelope>(READING_EVENT, (e) => {
    const env = e.payload;
    console.debug(`[reading] ${env.reading_id} #${env.seq} ${env.payload.type}`);
  });

  void useConfig.getState().load();
}
