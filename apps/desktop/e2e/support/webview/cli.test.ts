// @vitest-environment node
// ランチャ(cli.ts)の単体テスト。Playwright は起動せず、依存(platform・dist の有無・空きポート・spawn)を差し替えて確かめる。
// 加えて、サブプロセスでの実行と、cli.ts の値の import の一覧で、依存の取り違え(上流の paths.ts の import など)を検知する。
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildWebviewRun, runLauncher, webviewDistIndexPath, type LauncherDeps } from "./cli.ts";
import { DIST_INDEX_PATH } from "../paths.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** 既定では Windows・dist あり・空きポート 50123・spawn は終了コード 0 で成功する */
function makeDeps(overrides: Partial<LauncherDeps> = {}): LauncherDeps & {
  spawn: ReturnType<typeof vi.fn>;
  freePort: ReturnType<typeof vi.fn>;
} {
  const spawn = vi.fn(() => ({ status: 0 }));
  const freePort = vi.fn(async () => 50123);
  return {
    platform: "win32",
    distExists: true,
    env: {},
    freePort,
    spawn,
    ...overrides,
  } as LauncherDeps & { spawn: ReturnType<typeof vi.fn>; freePort: ReturnType<typeof vi.fn> };
}

describe("buildWebviewRun(Playwright の起動の指定)", () => {
  it("現在の Node で Playwright の CLI を起動し、webview のプロジェクトを 1 ワーカーで走らせる", () => {
    const run = buildWebviewRun([], {}, 50000);
    expect(run.command).toBe(process.execPath);
    expect(run.args[0]).toMatch(/[\\/]@playwright[\\/]test[\\/]cli\.js$/);
    expect(run.args.slice(1)).toEqual(["test", "--project=webview", "--workers=1"]);
  });

  it("利用者の引数はそのまま後ろに渡す(--grep など)", () => {
    const run = buildWebviewRun(["--grep", "smoke-01", "--list"], {}, 50000);
    expect(run.args.slice(-3)).toEqual(["--grep", "smoke-01", "--list"]);
  });

  it("E2E_WEBVIEW=1 と E2E_SKIP_PREREQ=1 を必ず付け、空きポートを E2E_PORT に入れる", () => {
    const run = buildWebviewRun([], {}, 50123);
    expect(run.env.E2E_WEBVIEW).toBe("1");
    expect(run.env.E2E_SKIP_PREREQ).toBe("1");
    expect(run.env.E2E_PORT).toBe("50123");
  });

  it("利用者が E2E_PORT を指定していれば、空きポートより優先する", () => {
    const run = buildWebviewRun([], { E2E_PORT: "14400" }, 50123);
    expect(run.env.E2E_PORT).toBe("14400");
  });

  it("既存の環境変数(E2E_APP_BIN など)は引き継ぐ", () => {
    const run = buildWebviewRun([], { E2E_APP_BIN: "C:/x/app.exe", CARGO_TARGET_DIR: "C:/t" }, 50000);
    expect(run.env.E2E_APP_BIN).toBe("C:/x/app.exe");
    expect(run.env.CARGO_TARGET_DIR).toBe("C:/t");
  });
});

describe("webviewDistIndexPath(dist のパス)", () => {
  it("上流の DIST_INDEX_PATH と同じ場所を指す", () => {
    expect(path.resolve(webviewDistIndexPath())).toBe(path.resolve(DIST_INDEX_PATH));
  });
});

describe("runLauncher(ランチャの本体。依存を差し替えて確かめる)", () => {
  let errors: string[];

  beforeEach(() => {
    errors = [];
    vi.spyOn(console, "error").mockImplementation((...args: unknown[]) => {
      errors.push(args.map(String).join(" "));
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("dist/ が無ければ spawn を呼ばずに 1 を返し、準備のコマンドを含む不足の一覧を出す", async () => {
    const deps = makeDeps({ distExists: false });
    const code = await runLauncher([], deps);
    expect(code).toBe(1);
    expect(deps.spawn).not.toHaveBeenCalled();
    expect(errors.length).toBeGreaterThan(0);
    expect(errors.join("\n")).toContain("npm run e2e:webview:prepare");
  });

  it("Windows 以外では spawn を呼ばずに 1 を返し、Windows 限定の案内を出す", async () => {
    const deps = makeDeps({ platform: "linux" });
    const code = await runLauncher([], deps);
    expect(code).toBe(1);
    expect(deps.spawn).not.toHaveBeenCalled();
    expect(errors.join("\n")).toContain("Windows 限定");
  });

  it("準備がそろっていれば buildWebviewRun の結果で spawn を呼び、その終了コードを返す", async () => {
    const deps = makeDeps({
      spawn: vi.fn(() => ({ status: 3 })),
      env: { E2E_APP_BIN: "C:/x/app.exe" },
    });
    const code = await runLauncher(["--grep", "smoke-01"], deps);
    expect(code).toBe(3);
    expect(deps.spawn).toHaveBeenCalledTimes(1);
    expect(deps.spawn).toHaveBeenCalledWith(
      buildWebviewRun(["--grep", "smoke-01"], { E2E_APP_BIN: "C:/x/app.exe" }, 50123),
    );
  });

  it("空きポートの戻り値を spawn に渡す env.E2E_PORT に反映する(既定のポートを固定しない)", async () => {
    const deps = makeDeps({ freePort: vi.fn(async () => 50999) });
    await runLauncher([], deps);
    const [run] = deps.spawn.mock.calls[0] as unknown as [{ env: NodeJS.ProcessEnv }];
    expect(run.env.E2E_PORT).toBe("50999");
  });

  it("E2E_PORT が指定されていれば、空きポートの戻り値より優先して spawn に渡す", async () => {
    const deps = makeDeps({ env: { E2E_PORT: "14400" }, freePort: vi.fn(async () => 50999) });
    await runLauncher([], deps);
    const [run] = deps.spawn.mock.calls[0] as unknown as [{ env: NodeJS.ProcessEnv }];
    expect(run.env.E2E_PORT).toBe("14400");
  });

  it("空きポートの取得が reject したら spawn を呼ばずに 1 を返し、理由を出す(未処理の拒否にしない)", async () => {
    const deps = makeDeps({
      freePort: vi.fn(async () => {
        throw new Error("EADDRINUSE: no port");
      }),
    });
    const code = await runLauncher([], deps);
    expect(code).toBe(1);
    expect(deps.spawn).not.toHaveBeenCalled();
    expect(errors.join("\n")).toContain("空きポートを取得できませんでした");
    expect(errors.join("\n")).toContain("EADDRINUSE: no port");
  });

  it("spawn が error を返したときは 1 を返し、起動の失敗を出す", async () => {
    const deps = makeDeps({
      spawn: vi.fn(() => ({ status: null, error: new Error("ENOENT spawn failed") })),
    });
    const code = await runLauncher([], deps);
    expect(code).toBe(1);
    expect(errors.join("\n")).toContain("Playwright の起動に失敗しました");
  });
});

describe("cli.ts のサブプロセスでの実行(依存の取り違えのバックストップ)", () => {
  let tmp: string;

  beforeEach(() => {
    // dist/ を持たない一時ディレクトリに cli.ts と prerequisites.ts だけを置く。上流の paths.ts は置かない。
    tmp = mkdtempSync(path.join(os.tmpdir(), "webview-cli-"));
    const dir = path.join(tmp, "e2e", "support", "webview");
    mkdirSync(dir, { recursive: true });
    cpSync(path.join(HERE, "cli.ts"), path.join(dir, "cli.ts"));
    cpSync(path.join(HERE, "prerequisites.ts"), path.join(dir, "prerequisites.ts"));
    writeFileSync(path.join(tmp, "package.json"), JSON.stringify({ type: "module" }));
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  it("dist/ が無い状態で終了コード 1 になり、Windows なら準備のコマンドを、それ以外なら Windows 限定の案内を stderr に出す", () => {
    const script = path.join(tmp, "e2e", "support", "webview", "cli.ts");
    const result = spawnSync(process.execPath, [script], { encoding: "utf8" });
    expect(result.status).toBe(1);
    if (process.platform === "win32") {
      expect(result.stderr).toContain("e2e:webview:prepare");
    } else {
      expect(result.stderr).toContain("Windows 限定");
    }
  });
});

describe("cli.ts の値の import(依存の境界)", () => {
  it("値として import するのは prerequisites.ts と node: の組み込みモジュールだけ", () => {
    const source = readFileSync(path.join(HERE, "cli.ts"), "utf8");
    const specifiers = [...source.matchAll(/^import\s+(?!type\s)[^;]*?from\s+["']([^"']+)["']/gm)].map(
      (m) => m[1] ?? "",
    );
    expect(specifiers).toContain("./prerequisites.ts");
    for (const specifier of specifiers) {
      expect(specifier === "./prerequisites.ts" || specifier.startsWith("node:")).toBe(true);
    }
  });
});
