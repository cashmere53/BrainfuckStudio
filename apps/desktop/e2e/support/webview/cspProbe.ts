// CSP の検証の補助(CspProbe)。
// (1) ページの securitypolicyviolation を記録し、読み出せるようにする(拒否された違反の有効なディレクティブと URI)。
// (2) 127.0.0.1 の空きポートで待ち受け、受けた要求を数えるだけの外部の受け口。拒否が効いて要求が届かなかったことを確かめる。
import { createServer, type Server } from "node:http";
import type { Page } from "@playwright/test";

/** 拒否された CSP の違反(有効なディレクティブと、拒否された URI) */
export type CspViolation = { effectiveDirective: string; blockedURI: string };

/** 外部の受け口。受けた要求を数えるだけで、内容には応答しない */
export interface ExternalSink {
  readonly origin: string;
  requestCount(): number;
  close(): Promise<void>;
}

/**
 * ページ内で実行する。page.evaluate に渡されるため自己完結させ、モジュールのスコープの値を参照しない。
 * 二重に呼ばれても登録は 1 回だけにする。
 */
export function installCspListener(): void {
  const cspWindow = window as unknown as {
    __bfE2eCspViolations?: { effectiveDirective: string; blockedURI: string }[];
  };
  if (cspWindow.__bfE2eCspViolations) {
    return;
  }
  const violations: { effectiveDirective: string; blockedURI: string }[] = [];
  cspWindow.__bfE2eCspViolations = violations;
  window.addEventListener(
    "securitypolicyviolation",
    (event: Event) => {
      const violation = event as SecurityPolicyViolationEvent;
      violations.push({
        effectiveDirective: violation.effectiveDirective,
        blockedURI: violation.blockedURI,
      });
    },
    true,
  );
}

/** 違反の記録を、アプリの描画後に設置する(以後の違反を記録する) */
export async function installCspViolationCollector(page: Page): Promise<void> {
  await page.evaluate(installCspListener);
}

/** 記録された違反を読み出す。記録が無ければ空の配列 */
export async function readCspViolations(page: Page): Promise<readonly CspViolation[]> {
  const raw = await page.evaluate(
    () =>
      (window as unknown as { __bfE2eCspViolations?: CspViolation[] }).__bfE2eCspViolations ?? [],
  );
  return raw.map((violation) => ({
    effectiveDirective: violation.effectiveDirective,
    blockedURI: violation.blockedURI,
  }));
}

/** 127.0.0.1 の空きポート(0 で OS に選ばせる)で、受けた要求を数えるだけの HTTP サーバを起動する */
export function startExternalSink(): Promise<ExternalSink> {
  return new Promise<ExternalSink>((resolve, reject) => {
    let count = 0;
    const server: Server = createServer((_request, response) => {
      count += 1;
      response.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
      response.end("sink");
    });
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") {
        server.close();
        reject(new Error("外部の受け口の待ち受けのポートを取得できませんでした"));
        return;
      }
      let closing: Promise<void> | null = null;
      resolve({
        origin: `http://127.0.0.1:${address.port}`,
        requestCount: () => count,
        close: () => {
          closing ??= new Promise<void>((resolveClose, rejectClose) => {
            server.close((error) => (error ? rejectClose(error) : resolveClose()));
            server.closeAllConnections();
          });
          return closing;
        },
      });
    });
  });
}
