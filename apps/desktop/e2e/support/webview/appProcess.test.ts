// @vitest-environment node
import { mkdir, mkdtemp, readdir, rm as fsRm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AppLaunchError,
  DEFAULT_READY_WITHIN_MS,
  appSpawnOptions,
  appendTail,
  buildAppEnv,
  devToolsActivePortPath,
  launchApp,
  parseDevToolsActivePort,
  readyWithinMsFromEnv,
  removeUserDataDir,
  scenarioUserDataDir,
  taskkillArgs,
  waitForCdpEndpoint,
  waitForDevToolsPort,
  type CdpVersion,
  type RemoveFn,
} from "./appProcess.ts";
import { SCENARIO_FOLDER_PREFIX, isScenarioFolderName } from "./userDataCleanup.ts";

const VERSION: CdpVersion = {
  browser: "Edg/120.0.0.0",
  webSocketDebuggerUrl: "ws://127.0.0.1:9222/devtools/browser/abc",
};

function expectLaunchError(promise: Promise<unknown>, reason: AppLaunchError["reason"]): Promise<AppLaunchError> {
  return promise.then(
    () => {
      throw new Error("AppLaunchError が投げられませんでした");
    },
    (error: unknown) => {
      expect(error).toBeInstanceOf(AppLaunchError);
      const launchError = error as AppLaunchError;
      expect(launchError.reason).toBe(reason);
      return launchError;
    },
  );
}

describe("buildAppEnv(継承した WebView2 の環境変数を除いた環境の組み立て)", () => {
  it("継承した WEBVIEW2_* を除き、デバッグ口と利用者データのフォルダの 2 つだけを設定する", () => {
    const base = {
      PATH: "C:\\Windows\\System32",
      SystemRoot: "C:\\Windows",
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--inherited=1",
      WEBVIEW2_USER_DATA_FOLDER: "C:\\inherited",
      WEBVIEW2_BROWSER_EXECUTABLE_FOLDER: "C:\\fixed-runtime",
      webview2_language: "ja",
    };

    const env = buildAppEnv(base, "C:\\tmp\\bfstudio-e2e-webview\\scenario-1");

    expect(env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS).toBe("--remote-debugging-port=0");
    expect(env.WEBVIEW2_USER_DATA_FOLDER).toBe("C:\\tmp\\bfstudio-e2e-webview\\scenario-1");
    expect(env.WEBVIEW2_BROWSER_EXECUTABLE_FOLDER).toBeUndefined();
    expect(env.webview2_language).toBeUndefined();
    expect(Object.keys(env).filter((key) => key.toUpperCase().startsWith("WEBVIEW2_")).sort()).toEqual([
      "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
      "WEBVIEW2_USER_DATA_FOLDER",
    ]);
  });

  it("それ以外の環境変数は残し、引数の環境を書き換えない", () => {
    const base = { PATH: "C:\\bin", FOO: "bar" };
    const snapshot = { ...base };

    const env = buildAppEnv(base, "C:\\data");

    expect(env.PATH).toBe("C:\\bin");
    expect(env.FOO).toBe("bar");
    expect(base).toEqual(snapshot);
  });
});

describe("parseDevToolsActivePort(DevToolsActivePort の解析)", () => {
  it("1 行目のポート番号を返す(2 行目の WebSocket のパスは読まない)", () => {
    expect(parseDevToolsActivePort("9222\n/devtools/browser/abc-123\n")).toBe(9222);
    expect(parseDevToolsActivePort("54321\r\n/devtools/browser/x\r\n")).toBe(54321);
  });

  it("1 行目と 2 行目を取り違えていれば null になる(ポートは数字だけ、パスは / で始まる)", () => {
    expect(parseDevToolsActivePort("/devtools/browser/abc-123\n9222\n")).toBeNull();
  });

  it("空・書きかけ(2 行目が無い、またはパスが途中)は null で、呼び出し側が再試行できる", () => {
    expect(parseDevToolsActivePort("")).toBeNull();
    expect(parseDevToolsActivePort("9222")).toBeNull();
    expect(parseDevToolsActivePort("9222\n")).toBeNull();
  });

  it("ポートとして不正な値(数字でない、0、65536 以上)は null", () => {
    expect(parseDevToolsActivePort("abc\n/devtools/browser/x\n")).toBeNull();
    expect(parseDevToolsActivePort("0\n/devtools/browser/x\n")).toBeNull();
    expect(parseDevToolsActivePort("65536\n/devtools/browser/x\n")).toBeNull();
  });
});

describe("devToolsActivePortPath(利用者データのフォルダ内のファイルの場所)", () => {
  it("<利用者データ>/EBWebView/DevToolsActivePort を指す", () => {
    expect(devToolsActivePortPath("C:\\data\\scenario-x")).toBe(path.join("C:\\data\\scenario-x", "EBWebView", "DevToolsActivePort"));
  });
});

describe("waitForDevToolsPort(DevToolsActivePort が現れるまでの待機)", () => {
  it("ファイルが現れて書き終わるまで再試行し、ポートを返す", async () => {
    const readFile = vi
      .fn<() => Promise<string | null>>()
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce("9222\n")
      .mockResolvedValueOnce("9222\n/devtools/browser/abc\n");

    const port = await waitForDevToolsPort({
      readyWithinMs: 5_000,
      intervalMs: 1,
      readFile,
      hasExited: () => false,
    });

    expect(port).toBe(9222);
    expect(readFile).toHaveBeenCalledTimes(3);
  });

  it("ファイルが現れる前にプロセスが終了したら、早期終了の理由と出力の末尾を含めて失敗する", async () => {
    const error = await expectLaunchError(
      waitForDevToolsPort({
        readyWithinMs: 5_000,
        intervalMs: 1,
        readFile: async () => null,
        hasExited: () => true,
        outputTail: () => "fatal: sample output",
      }),
      "exitedEarly",
    );

    expect(error.message).toContain("早期に終了");
    expect(error.outputTail).toBe("fatal: sample output");
  });

  it("上限時間までファイルが現れなければ、notReady で案内を含めて失敗する", async () => {
    const error = await expectLaunchError(
      waitForDevToolsPort({
        readyWithinMs: 30,
        intervalMs: 5,
        readFile: async () => null,
        hasExited: () => false,
      }),
      "notReady",
    );

    expect(error.message).toContain("30 ms");
    expect(error.message).toContain("DevToolsActivePort");
    expect(error.message).toContain("ウイルス対策");
    expect(error.message).toContain("CARGO_TARGET_DIR");
  });

  it("書きかけの内容(解析できない)は成功扱いせず、読み直す", async () => {
    const readFile = vi
      .fn<() => Promise<string | null>>()
      .mockResolvedValueOnce("/devtools/browser/abc\n9222\n")
      .mockResolvedValue("9222\n");

    await expectLaunchError(
      waitForDevToolsPort({
        readyWithinMs: 30,
        intervalMs: 5,
        readFile,
        hasExited: () => false,
      }),
      "notReady",
    );
    expect(readFile.mock.calls.length).toBeGreaterThan(1);
  });
});

describe("readyWithinMsFromEnv(上限時間の既定と上書き)", () => {
  it("未指定と空文字は既定の 60 秒", () => {
    expect(DEFAULT_READY_WITHIN_MS).toBe(60_000);
    expect(readyWithinMsFromEnv({})).toBe(60_000);
    expect(readyWithinMsFromEnv({ E2E_APP_READY_MS: "" })).toBe(60_000);
  });

  it("E2E_APP_READY_MS の正の整数を使う。正の整数でなければ既定に戻す", () => {
    expect(readyWithinMsFromEnv({ E2E_APP_READY_MS: "15000" })).toBe(15_000);
    expect(readyWithinMsFromEnv({ E2E_APP_READY_MS: "abc" })).toBe(60_000);
    expect(readyWithinMsFromEnv({ E2E_APP_READY_MS: "0" })).toBe(60_000);
  });
});

describe("waitForCdpEndpoint(CDP の口の待機)", () => {
  it("応答するまで再試行し、応答のバージョン情報を返す", async () => {
    const fetchVersion = vi
      .fn<(port: number) => Promise<CdpVersion | null>>()
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(VERSION);

    const version = await waitForCdpEndpoint(9333, {
      readyWithinMs: 5_000,
      intervalMs: 1,
      fetchVersion,
      hasExited: () => false,
    });

    expect(version).toEqual(VERSION);
    expect(fetchVersion).toHaveBeenCalledTimes(3);
    expect(fetchVersion).toHaveBeenCalledWith(9333);
  });

  it("応答する前にプロセスが終了したら、早期終了の理由と出力の末尾を含めて失敗する", async () => {
    const fetchVersion = vi.fn(async () => null);

    const error = await expectLaunchError(
      waitForCdpEndpoint(9333, {
        readyWithinMs: 5_000,
        intervalMs: 1,
        fetchVersion,
        hasExited: () => true,
        outputTail: () => "fatal: sample output",
      }),
      "exitedEarly",
    );

    expect(error.message).toContain("早期に終了");
    expect(error.outputTail).toBe("fatal: sample output");
  });

  it("上限時間を超えたら、ウイルス対策ソフト・新しい CARGO_TARGET_DIR・再起動の案内を含めて失敗する", async () => {
    const error = await expectLaunchError(
      waitForCdpEndpoint(9333, {
        readyWithinMs: 30,
        intervalMs: 5,
        fetchVersion: async () => null,
        hasExited: () => false,
      }),
      "notReady",
    );

    expect(error.message).toContain("30 ms");
    expect(error.message).toContain("ウイルス対策");
    expect(error.message).toContain("除外");
    expect(error.message).toContain("一時停止");
    expect(error.message).toContain("CARGO_TARGET_DIR");
    expect(error.message).toContain("再起動");
  });
});

describe("appendTail(標準出力と標準エラーの末尾の保持)", () => {
  it("上限を超える入力では長さが上限ちょうどになり、結果は入力の末尾と一致する", () => {
    const limit = 64 * 1024;
    const input = "a".repeat(limit) + "0123456789";
    let tail = "";
    for (let index = 0; index < input.length; index += 1000) {
      tail = appendTail(tail, input.slice(index, index + 1000));
    }
    expect(tail.length).toBe(limit);
    expect(input.endsWith(tail)).toBe(true);
    expect(tail.endsWith("0123456789")).toBe(true);
  });

  it("上限を超えたら先頭が落ち、上限以内なら全部残る", () => {
    const limit = 64 * 1024;
    const head = "HEAD-MARKER";
    const tail = appendTail(head, "x".repeat(limit));
    expect(tail.length).toBe(limit);
    expect(tail.startsWith(head)).toBe(false);

    expect(appendTail("abc", "def")).toBe("abcdef");
  });
});

describe("taskkillArgs(プロセスツリーの終了の引数)", () => {
  it("/T(子プロセスを含む)と /F(強制)を含む完全一致の引数を返す", () => {
    expect(taskkillArgs(123)).toEqual(["/PID", "123", "/T", "/F"]);
  });
});

describe("scenarioUserDataDir(シナリオごとの利用者データのフォルダ)", () => {
  it("根の直下に作り、名前は掃除の判定(isScenarioFolderName)を満たし、呼び出しごとに異なる", () => {
    const root = path.join(tmpdir(), "bfstudio-e2e-webview");
    const first = scenarioUserDataDir(root);
    const second = scenarioUserDataDir(root);

    expect(path.dirname(first)).toBe(path.resolve(root));
    expect(path.dirname(second)).toBe(path.resolve(root));
    expect(isScenarioFolderName(path.basename(first))).toBe(true);
    expect(path.basename(first).startsWith(SCENARIO_FOLDER_PREFIX)).toBe(true);
    expect(first).not.toBe(second);
  });
});

describe("removeUserDataDir(利用者データのフォルダの削除)", () => {
  it("削除が成功したら true を返し、再試行の回数の指定(maxRetries > 0)を渡す", async () => {
    const rmImpl = vi.fn<RemoveFn>(async () => undefined);

    await expect(removeUserDataDir("C:\\data\\scenario-x", rmImpl)).resolves.toBe(true);

    expect(rmImpl).toHaveBeenCalledTimes(1);
    const [dir, options] = rmImpl.mock.calls[0] ?? [];
    expect(dir).toBe("C:\\data\\scenario-x");
    expect(options?.recursive).toBe(true);
    expect(options?.force).toBe(true);
    expect(options?.maxRetries).toBeGreaterThan(0);
  });

  it("削除が投げたら投げずに false を返す", async () => {
    const rmImpl: RemoveFn = async () => {
      throw new Error("EBUSY");
    };

    await expect(removeUserDataDir("C:\\data\\scenario-x", rmImpl)).resolves.toBe(false);
  });
});

describe("appSpawnOptions(実アプリの起動の引数)", () => {
  it("シェルを介さず(shell が未指定か false)、窓は隠さず、標準入力は無視して標準出力と標準エラーを受け取る", () => {
    const env = { PATH: "C:\bin" };
    const options = appSpawnOptions(env);

    expect(options.shell === undefined || options.shell === false).toBe(true);
    expect(options.stdio).toEqual(["ignore", "pipe", "pipe"]);
    expect(options.windowsHide).toBe(false);
    expect(options.env).toBe(env);
  });
});

describe("launchApp(実プロセスでの起動の失敗)", () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(path.join(tmpdir(), "bfstudio-launch-test-"));
  });

  afterEach(async () => {
    await fsRm(root, { recursive: true, force: true });
  });

  it("存在しない実行ファイルは notFound で失敗する", async () => {
    const missing = path.join(root, "does-not-exist.exe");

    const error = await launchApp({ exePath: missing, userDataRoot: root, readyWithinMs: 1_000 }).then(
      () => {
        throw new Error("AppLaunchError が投げられませんでした");
      },
      (caught: unknown) => caught as AppLaunchError,
    );

    expect(error).toBeInstanceOf(AppLaunchError);
    expect(error.reason).toBe("notFound");
    expect(error.message).toContain(missing);
  });

  it("引数なしですぐ終わる実行ファイルは数秒以内に exitedEarly で失敗し、フォルダは残らない", async () => {
    // 引数なし・標準入力を無視した node は、読むスクリプトが無いため直ちに終了する
    const startedAt = Date.now();

    const error = await launchApp({
      exePath: process.execPath,
      userDataRoot: root,
      readyWithinMs: 60_000,
    }).then(
      () => {
        throw new Error("AppLaunchError が投げられませんでした");
      },
      (caught: unknown) => caught as AppLaunchError,
    );

    expect(Date.now() - startedAt).toBeLessThan(15_000);
    expect(error).toBeInstanceOf(AppLaunchError);
    expect(error.reason).toBe("exitedEarly");
    expect(await readdir(root)).toEqual([]);
  });

  it("起動の前に古い DevToolsActivePort を消すため、古いポートを読まずに DevToolsActivePort の段で早期終了する", async () => {
    // 古いファイルには存在しないはずのポート(1)を書く。消されていれば読まれず、CDP の段には進まない
    const userDataDir = path.join(root, "scenario-stale");
    await mkdir(path.join(userDataDir, "EBWebView"), { recursive: true });
    await writeFile(path.join(userDataDir, "EBWebView", "DevToolsActivePort"), "1\n/devtools/browser/stale\n");

    const error = await launchApp({
      exePath: process.execPath,
      userDataRoot: root,
      readyWithinMs: 60_000,
      userDataDir,
    }).then(
      () => {
        throw new Error("AppLaunchError が投げられませんでした");
      },
      (caught: unknown) => caught as AppLaunchError,
    );

    expect(error).toBeInstanceOf(AppLaunchError);
    expect(error.reason).toBe("exitedEarly");
    expect(error.message).toContain("DevToolsActivePort");
    expect(error.message).not.toContain("CDP の口");
  });
});
