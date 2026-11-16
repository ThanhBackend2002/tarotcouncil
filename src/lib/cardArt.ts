// Ảnh lá bài: 78 bản quét Rider–Waite–Smith 1909 (public domain) đóng gói cùng app → chạy offline.
// `art_key` (vd "m00", "c13") do Rust trả về trong `Card` / `Draw`.

const urls = import.meta.glob<string>("../assets/cards/*.jpg", { eager: true, query: "?url", import: "default" });

const byKey = new Map(Object.entries(urls).map(([path, url]) => [path.split("/").pop()!.replace(/\.jpg$/, ""), url]));

export const cardImage = (artKey: string): string | undefined => byKey.get(artKey);

export const CARD_ASPECT = 350 / 600;
