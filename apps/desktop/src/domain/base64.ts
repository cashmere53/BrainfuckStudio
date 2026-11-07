// Uint8Array ↔ Base64(標準アルファベット、パディングあり)。
// バイト列の JSON 表現(DTO の `inputBase64` / `dataBase64`)に使う。

export type Base64DecodeResult = { ok: true; bytes: Uint8Array } | { ok: false };

// 標準アルファベットのみ・パディングは末尾 1 つか 2 つ・長さは 4 の倍数
const BASE64_PATTERN = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

// String.fromCharCode への引数展開がスタックを超えないよう分割して連結する
const CHUNK = 0x8000;

export function encodeBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

export function decodeBase64(text: string): Base64DecodeResult {
  if (!BASE64_PATTERN.test(text)) {
    return { ok: false };
  }
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return { ok: true, bytes };
}
