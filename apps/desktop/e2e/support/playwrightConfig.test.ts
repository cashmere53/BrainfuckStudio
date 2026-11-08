// @vitest-environment node
// playwright.config.ts の配線の単体テスト。projects の分離(@perf)とポートの配線を、設定の読み込み結果で確かめる。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Pattern = RegExp | RegExp[] | undefined;

/** Playwright の grep と同じく、パターンのどれか一つに一致すれば真 */
function matchesAny(pattern: Pattern, title: string): boolean {
  if (pattern === undefined) {
    return false;
  }
  const list = Array.isArray(pattern) ? pattern : [pattern];
  return list.some((re) => re.test(title));
}

type ProjectLike = { readonly grep?: Pattern; readonly grepInvert?: Pattern };

/** テストのタイトルがそのプロジェクトで走るか(grep に一致し、grepInvert に一致しない) */
function runsIn(project: ProjectLike, title: string): boolean {
  const inGrep = project.grep === undefined || matchesAny(project.grep, title);
  return inGrep && !matchesAny(project.grepInvert, title);
}

/** 環境変数を反映した状態で設定を読み直す */
async function loadConfig() {
  vi.resetModules();
  const mod = await import("../../playwright.config.ts");
  return mod.default;
}

describe("playwright.config.ts の projects(@perf の分離)", () => {
  beforeEach(() => {
    vi.stubEnv("E2E_SKIP_PREREQ", "1");
    vi.stubEnv("CI", "");
    vi.stubEnv("E2E_PORT", "");
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("projects は chromium と perf の 2 件で、この順に並ぶ", async () => {
    const config = await loadConfig();
    expect((config.projects ?? []).map((p) => p.name)).toEqual(["chromium", "perf"]);
  });

  it("chromium は grepInvert で @perf を除き、grep は持たない", async () => {
    const config = await loadConfig();
    const chromium = (config.projects ?? [])[0];
    expect(chromium?.name).toBe("chromium");
    expect(chromium?.grep).toBeUndefined();
    expect(matchesAny(chromium?.grepInvert, "x @perf")).toBe(true);
  });

  it("perf は grep で @perf だけを対象にし、grepInvert は持たない", async () => {
    const config = await loadConfig();
    const perf = (config.projects ?? [])[1];
    expect(perf?.name).toBe("perf");
    expect(perf?.grepInvert).toBeUndefined();
    expect(matchesAny(perf?.grep, "x @perf")).toBe(true);
    expect(matchesAny(perf?.grep, "通常のタイトル")).toBe(false);
  });

  it("通常のタイトルは chromium だけで走り、@perf のタイトルは perf だけで走る", async () => {
    const config = await loadConfig();
    const [chromium, perf] = config.projects ?? [];
    expect(runsIn(chromium ?? {}, "通常のタイトル")).toBe(true);
    expect(runsIn(perf ?? {}, "通常のタイトル")).toBe(false);
    expect(runsIn(chromium ?? {}, "x @perf")).toBe(false);
    expect(runsIn(perf ?? {}, "x @perf")).toBe(true);
  });
});

describe("playwright.config.ts のポートの配線(E2E_PORT)", () => {
  beforeEach(() => {
    vi.stubEnv("E2E_SKIP_PREREQ", "1");
    vi.stubEnv("CI", "");
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("E2E_PORT が無いとき 14273 を使い、開発サーバの 1420 は使わない", async () => {
    vi.stubEnv("E2E_PORT", "");
    const config = await loadConfig();
    const web = config.webServer as { command: string; url: string };
    const baseURL = config.use?.baseURL;

    expect(baseURL).toBe("http://127.0.0.1:14273");
    expect(web.url).toBe("http://127.0.0.1:14273");
    expect(web.command).toContain("--port 14273");
    expect(`${baseURL} ${web.url} ${web.command}`).not.toContain("1420");
  });

  it("E2E_PORT=15555 のとき baseURL・webServer.url・--port の 3 か所とも 15555 になる", async () => {
    vi.stubEnv("E2E_PORT", "15555");
    const config = await loadConfig();
    const web = config.webServer as { command: string; url: string };

    expect(config.use?.baseURL).toBe("http://127.0.0.1:15555");
    expect(web.url).toBe("http://127.0.0.1:15555");
    expect(web.command).toContain("--port 15555");
  });

  it("webServer.command は --strictPort を含む(別のポートへ黙って移らない)", async () => {
    vi.stubEnv("E2E_PORT", "");
    const config = await loadConfig();
    const web = config.webServer as { command: string; url: string };
    expect(web.command).toContain("--strictPort");
  });
});
