// CSP の実効のシナリオ(7.1〜7.4)。配布と同じ CSP を持つ実アプリのページで、インラインのスクリプトの挿入、
// 外部のオリジンのスクリプトの挿入、外部のオリジンへの取得がそれぞれ拒否されることを確かめる。
// 拒否のたびに、違反が記録され、外部の受け口に要求が届かず(受信数 0)、スクリプトの副作用が残らないことを見る。
// 陽性対照(7.4)では同じオリジンの取得が成功することを確かめ、拒否が CSP によるものであることを示す。
import { expect, test } from "../support/webview/fixtures.ts";
import {
  installCspViolationCollector,
  readCspViolations,
  startExternalSink,
  type CspViolation,
} from "../support/webview/cspProbe.ts";

/** 指定のディレクティブで、指定の URI(前方一致。外部のオリジンは origin で判定する)の違反が記録されたか */
function hasViolation(violations: readonly CspViolation[], effectiveDirective: string, blockedURI: string): boolean {
  return violations.some(
    (violation) => violation.effectiveDirective === effectiveDirective && violation.blockedURI.startsWith(blockedURI),
  );
}

test.describe("CSP: 実効", () => {
  test("CSP 7.1 インラインのスクリプトは実行されず、script-src-elem の違反が inline として記録される", async ({
    app,
  }) => {
    await installCspViolationCollector(app.page);

    await app.page.evaluate(() => {
      const script = document.createElement("script");
      script.textContent = "window.__bfE2eInlineRan = true;";
      document.head.appendChild(script);
    });

    await expect
      .poll(async () => (await readCspViolations(app.page)).some((v) => v.effectiveDirective === "script-src-elem" && v.blockedURI === "inline"))
      .toBe(true);
    const ran = await app.page.evaluate(() => (window as unknown as { __bfE2eInlineRan?: boolean }).__bfE2eInlineRan);
    expect(ran).toBeUndefined();
  });

  test("CSP 7.2 外部のオリジンのスクリプトは実行されず、受け口に要求が届かず、script-src-elem の違反が記録される", async ({
    app,
  }) => {
    const sink = await startExternalSink();
    try {
      await installCspViolationCollector(app.page);

      // 読み込みの結果(load か error)を待つ。CSP が拒否すると error になり、緩めて読み込まれると load になる
      const outcome = await app.page.evaluate(
        (src: string) =>
          new Promise<string>((resolve) => {
            const script = document.createElement("script");
            script.onload = () => resolve("load");
            script.onerror = () => resolve("error");
            script.src = src;
            document.head.appendChild(script);
          }),
        `${sink.origin}/external.js`,
      );
      expect(outcome).toBe("error");

      await expect
        .poll(async () => hasViolation(await readCspViolations(app.page), "script-src-elem", sink.origin))
        .toBe(true);
      const ran = await app.page.evaluate(() => (window as unknown as { __bfE2eExternalRan?: boolean }).__bfE2eExternalRan);
      expect(ran).toBeUndefined();
      expect(sink.requestCount()).toBe(0);
    } finally {
      await sink.close();
    }
  });

  test("CSP 7.3 外部のオリジンへの取得は拒否され、受け口に要求が届かず、connect-src の違反が記録される", async ({
    app,
  }) => {
    const sink = await startExternalSink();
    try {
      await installCspViolationCollector(app.page);

      // no-cors: CSP が無ければ不透明な応答で成功し、CSP が拒否したときだけ TypeError になる(CORS の失敗と区別する)
      const outcome = await app.page.evaluate(async (url: string) => {
        try {
          await fetch(url, { mode: "no-cors" });
          return "resolved";
        } catch (error) {
          return error instanceof TypeError ? "TypeError" : "other";
        }
      }, `${sink.origin}/`);

      expect(outcome).toBe("TypeError");
      await expect
        .poll(async () => hasViolation(await readCspViolations(app.page), "connect-src", sink.origin))
        .toBe(true);
      expect(sink.requestCount()).toBe(0);
    } finally {
      await sink.close();
    }
  });

  test("CSP 7.4 陽性対照: 同じオリジンの取得は成功して 200 を返し、connect-src の違反は記録されない", async ({ app }) => {
    const sink = await startExternalSink();
    try {
      await installCspViolationCollector(app.page);

      const status = await app.page.evaluate(async () => (await fetch("/")).status);

      expect(status).toBe(200);
      expect(sink.requestCount()).toBe(0);
      const violations = await readCspViolations(app.page);
      expect(violations.some((v) => v.effectiveDirective === "connect-src")).toBe(false);
    } finally {
      await sink.close();
    }
  });
});
