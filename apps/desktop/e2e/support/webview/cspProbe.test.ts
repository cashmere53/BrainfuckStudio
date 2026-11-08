// @vitest-environment node
// CSP の検証の補助(CspProbe)の単体テスト。
// 違反の記録は page.evaluate に渡す関数を、Node の EventTarget を window に見立てて直接呼んで確かめる。
// 外部の受け口は 127.0.0.1 の実サーバに Node の fetch で要求し、数えられて閉じられることを確かめる。
import { afterEach, describe, expect, it } from "vitest";
import type { Page } from "@playwright/test";
import {
  installCspViolationCollector,
  readCspViolations,
  startExternalSink,
  type ExternalSink,
} from "./cspProbe.ts";

// page.evaluate は渡された関数をそのまま呼ぶ偽のページ(引数は 1 つまで)
function fakePage(): Page {
  return {
    evaluate: (fn: (arg?: unknown) => unknown, arg?: unknown) => Promise.resolve(fn(arg)),
  } as unknown as Page;
}

// window の代わりに EventTarget を置き、securitypolicyviolation を発火させる
function installFakeWindow(): EventTarget {
  const target = new EventTarget();
  (globalThis as { window?: unknown }).window = target;
  return target;
}

function dispatchViolation(target: EventTarget, effectiveDirective: string, blockedURI: string): void {
  const event = new Event("securitypolicyviolation");
  Object.assign(event, { effectiveDirective, blockedURI });
  target.dispatchEvent(event);
}

const openSinks: ExternalSink[] = [];

afterEach(async () => {
  delete (globalThis as { window?: unknown }).window;
  for (const sink of openSinks.splice(0)) {
    await sink.close().catch(() => undefined);
  }
});

describe("installCspViolationCollector / readCspViolations(CSP の違反の記録)", () => {
  it("記録が無ければ空の配列を返す", async () => {
    installFakeWindow();
    await installCspViolationCollector(fakePage());
    expect(await readCspViolations(fakePage())).toEqual([]);
  });

  it("違反の有効なディレクティブと拒否された URI を順に記録する", async () => {
    const target = installFakeWindow();
    await installCspViolationCollector(fakePage());
    dispatchViolation(target, "script-src-elem", "inline");
    dispatchViolation(target, "connect-src", "http://127.0.0.1:50000/");
    expect(await readCspViolations(fakePage())).toEqual([
      { effectiveDirective: "script-src-elem", blockedURI: "inline" },
      { effectiveDirective: "connect-src", blockedURI: "http://127.0.0.1:50000/" },
    ]);
  });

  it("二重に設置しても違反は 1 回ずつ記録される", async () => {
    const target = installFakeWindow();
    await installCspViolationCollector(fakePage());
    await installCspViolationCollector(fakePage());
    dispatchViolation(target, "script-src-elem", "inline");
    expect(await readCspViolations(fakePage())).toHaveLength(1);
  });

  it("二重に設置しても、設置前の記録は保持され、以後の違反も記録される", async () => {
    const target = installFakeWindow();
    await installCspViolationCollector(fakePage());
    dispatchViolation(target, "script-src-elem", "inline");
    await installCspViolationCollector(fakePage());
    dispatchViolation(target, "connect-src", "http://127.0.0.1:1/");
    expect(await readCspViolations(fakePage())).toEqual([
      { effectiveDirective: "script-src-elem", blockedURI: "inline" },
      { effectiveDirective: "connect-src", blockedURI: "http://127.0.0.1:1/" },
    ]);
  });
});

describe("startExternalSink(外部の受け口)", () => {
  it("127.0.0.1 の空きポートで待ち受け、受けた要求を数える", async () => {
    const sink = await startExternalSink();
    openSinks.push(sink);
    expect(sink.origin).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
    expect(sink.requestCount()).toBe(0);

    expect((await fetch(`${sink.origin}/a`)).status).toBe(200);
    expect((await fetch(`${sink.origin}/b`)).status).toBe(200);
    expect(sink.requestCount()).toBe(2);
  });

  it("127.0.0.1 だけで待ち受け、別のループバックアドレスからは届かず数えられない", async () => {
    const sink = await startExternalSink();
    openSinks.push(sink);
    const port = new URL(sink.origin).port;

    await expect(fetch(`http://127.0.0.2:${port}/`)).rejects.toThrow();
    expect(sink.requestCount()).toBe(0);

    expect((await fetch(sink.origin)).status).toBe(200);
    expect(sink.requestCount()).toBe(1);
  });

  it("別の受け口とは別のポートを使う", async () => {
    const first = await startExternalSink();
    const second = await startExternalSink();
    openSinks.push(first, second);
    expect(first.origin).not.toBe(second.origin);
  });

  it("close の後は接続できず、二度目の close も失敗しない", async () => {
    const sink = await startExternalSink();
    openSinks.push(sink);
    await fetch(`${sink.origin}/`);
    await sink.close();
    await expect(fetch(`${sink.origin}/`)).rejects.toThrow();
    await expect(sink.close()).resolves.toBeUndefined();
  });
});
