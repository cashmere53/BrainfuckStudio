// @vitest-environment node
import path from "node:path";
import { execFile } from "node:child_process";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WINDOWS_HOST_SCRIPT_PATH } from "./paths.ts";
import { HostActionError, bindWindowsHost, buildHostArgs, parseHostResult, runHostAction } from "./windowsHost.ts";

// 補助スクリプトの起動(execFile)を差し替え、起動の引数・オプション・終了の扱いを確かめる
vi.mock("node:child_process", () => ({ execFile: vi.fn() }));

type FakeOutcome = { error: Error | null; stdout: string; stderr: string };
type FakeCall = { file: string; args: string[]; options: Record<string, unknown> };

const fakeCalls: FakeCall[] = [];
let fakeOutcome: FakeOutcome = { error: null, stdout: "", stderr: "" };

beforeEach(() => {
  fakeCalls.length = 0;
  fakeOutcome = { error: null, stdout: "", stderr: "" };
  vi.mocked(execFile).mockImplementation(((file: string, args: string[], options: Record<string, unknown>, callback: (error: Error | null, stdout: string, stderr: string) => void) => {
    fakeCalls.push({ file, args, options });
    callback(fakeOutcome.error, fakeOutcome.stdout, fakeOutcome.stderr);
    return {} as never;
  }) as never);
});

function firstCall(): FakeCall {
  const call = fakeCalls[0];
  if (call === undefined) {
    throw new Error("補助スクリプトが起動されていません");
  }
  return call;
}

function exitFailure(stdout: string, stderr = ""): void {
  fakeOutcome = { error: Object.assign(new Error("Command failed"), { code: 1 }), stdout, stderr };
}

function exitSuccess(stdout: string): void {
  fakeOutcome = { error: null, stdout, stderr: "" };
}

function expectHostError(fn: () => unknown, action: string, reasonPart: string): void {
  let caught: unknown;
  try {
    fn();
  } catch (error) {
    caught = error;
  }
  expect(caught).toBeInstanceOf(HostActionError);
  const error = caught as HostActionError;
  expect(error.action).toBe(action);
  expect(error.reason).toContain(reasonPart);
}

describe("parseHostResult(補助スクリプトの 1 行の JSON の解析)", () => {
  it("runtime: WebView2 の版の文字列を返す", () => {
    expect(parseHostResult("runtime", '{"action":"runtime","webView2Version":"158.0.4370.7"}')).toEqual({
      action: "runtime",
      webView2Version: "158.0.4370.7",
    });
  });

  it("runtime: 導入されていなければ版は null(エラーにしない)", () => {
    expect(parseHostResult("runtime", '{"action":"runtime","webView2Version":null}')).toEqual({
      action: "runtime",
      webView2Version: null,
    });
  });

  it("window: 見つかったウィンドウのタイトルを返す(日本語を含む)", () => {
    expect(parseHostResult("window", '{"action":"window","found":true,"title":"Brainfuck Studio - 無題"}')).toEqual({
      action: "window",
      found: true,
      title: "Brainfuck Studio - 無題",
    });
  });

  it("window: ウィンドウが無ければ理由つきのエラーにする", () => {
    expectHostError(
      () => parseHostResult("window", '{"action":"window","found":false,"title":null}'),
      "window",
      "主ウィンドウ",
    );
  });

  it("close: 届いた結果は delivered: true として返す", () => {
    expect(parseHostResult("close", '{"action":"close","delivered":true,"reason":null}')).toEqual({
      action: "close",
      delivered: true,
      reason: null,
    });
  });

  it("close: 届けられなかった(delivered: false)結果は、理由つきの HostActionError にする", () => {
    expectHostError(
      () => parseHostResult("close", '{"action":"close","delivered":false,"reason":"主ウィンドウが見つかりません"}'),
      "close",
      "主ウィンドウが見つかりません",
    );
  });

  it("keys: 届けられなかった(前面化できない)結果は、理由つきの HostActionError にする", () => {
    expectHostError(
      () => parseHostResult("keys", '{"action":"keys","delivered":false,"reason":"前面化できません"}'),
      "keys",
      "前面化できません",
    );
  });

  it("keys: 届いた結果は delivered: true として返す", () => {
    expect(parseHostResult("keys", '{"action":"keys","delivered":true,"reason":null}')).toEqual({
      action: "keys",
      delivered: true,
      reason: null,
    });
  });

  it("menu: 既定のコンテキストメニューの有無を返す", () => {
    expect(parseHostResult("menu", '{"action":"menu","menuOpen":false}')).toEqual({ action: "menu", menuOpen: false });
    expect(parseHostResult("menu", '{"action":"menu","menuOpen":true}')).toEqual({ action: "menu", menuOpen: true });
  });

  it("menu: 検出したメニューの診断(ポップアップのクラスと項目の名前)を返す", () => {
    expect(
      parseHostResult(
        "menu",
        '{"action":"menu","menuOpen":true,"menu":{"windowClass":"Chrome_WidgetWin_1","items":["戻る","最新の情報に更新","名前を付けて保存"]}}',
      ),
    ).toEqual({
      action: "menu",
      menuOpen: true,
      menu: { windowClass: "Chrome_WidgetWin_1", items: ["戻る", "最新の情報に更新", "名前を付けて保存"] },
    });
  });

  it("menu: 項目が無いメニューは items が空の配列として返す", () => {
    expect(parseHostResult("menu", '{"action":"menu","menuOpen":true,"menu":{"windowClass":"Chrome_WidgetWin_1","items":[]}}')).toEqual({
      action: "menu",
      menuOpen: true,
      menu: { windowClass: "Chrome_WidgetWin_1", items: [] },
    });
  });

  it("menu: 検出なし(menu が null)は menu: null として返す", () => {
    expect(parseHostResult("menu", '{"action":"menu","menuOpen":false,"menu":null}')).toEqual({
      action: "menu",
      menuOpen: false,
      menu: null,
    });
  });

  it("menu: items の型が違えば HostActionError にする(文字列・数値の混在)", () => {
    expectHostError(
      () => parseHostResult("menu", '{"action":"menu","menuOpen":true,"menu":{"windowClass":"Chrome_WidgetWin_1","items":"戻る"}}'),
      "menu",
      "形",
    );
    expectHostError(
      () => parseHostResult("menu", '{"action":"menu","menuOpen":true,"menu":{"windowClass":"Chrome_WidgetWin_1","items":["戻る",1]}}'),
      "menu",
      "items",
    );
  });

  it("menu: windowClass が文字列でなければ HostActionError にする", () => {
    expectHostError(
      () => parseHostResult("menu", '{"action":"menu","menuOpen":true,"menu":{"windowClass":1,"items":[]}}'),
      "menu",
      "形",
    );
  });

  it("error の行は、その理由を含む HostActionError にする", () => {
    expectHostError(
      () => parseHostResult("runtime", '{"error":"レジストリの読み取りに失敗"}'),
      "runtime",
      "レジストリの読み取りに失敗",
    );
  });

  it("出力が空なら HostActionError にする", () => {
    expectHostError(() => parseHostResult("menu", ""), "menu", "1 行");
  });

  it("出力が複数行なら形が違うとして HostActionError にする", () => {
    expectHostError(
      () => parseHostResult("menu", '{"action":"menu","menuOpen":false}\n{"action":"menu","menuOpen":false}'),
      "menu",
      "1 行",
    );
  });

  it("JSON として読めない出力は HostActionError にする", () => {
    expectHostError(() => parseHostResult("menu", "menuOpen=false"), "menu", "JSON");
  });

  it("JSON でもオブジェクトでもない出力(配列・数値)は HostActionError にする", () => {
    expectHostError(() => parseHostResult("menu", "[1,2]"), "menu", "形");
    expectHostError(() => parseHostResult("menu", "42"), "menu", "形");
  });

  it("動作の名前が要求と違う出力は HostActionError にする", () => {
    expectHostError(() => parseHostResult("menu", '{"action":"close","delivered":true,"reason":null}'), "menu", "動作");
  });

  it("フィールドの型が違えば HostActionError にする(menuOpen が文字列など)", () => {
    expectHostError(() => parseHostResult("menu", '{"action":"menu","menuOpen":"yes"}'), "menu", "形");
    expectHostError(() => parseHostResult("runtime", '{"action":"runtime","webView2Version":""}'), "runtime", "形");
    expectHostError(() => parseHostResult("window", '{"action":"window","found":true,"title":null}'), "window", "形");
  });

  it("末尾の改行や空行は無視して 1 行として扱う", () => {
    expect(parseHostResult("menu", '\r\n{"action":"menu","menuOpen":true}\r\n\r\n')).toEqual({
      action: "menu",
      menuOpen: true,
    });
  });
});

describe("buildHostArgs(補助スクリプトの固定の引数)", () => {
  it("補助スクリプトのパスを -File に固定し、実行方針を Bypass にする", () => {
    const args = buildHostArgs("runtime", {});
    expect(args).toContain("-File");
    expect(args[args.indexOf("-File") + 1]).toBe(WINDOWS_HOST_SCRIPT_PATH);
    expect(args).toContain("-ExecutionPolicy");
    expect(args[args.indexOf("-ExecutionPolicy") + 1]).toBe("Bypass");
    expect(path.basename(WINDOWS_HOST_SCRIPT_PATH)).toBe("windowsHost.ps1");
  });

  it("runtime は pid を取らない", () => {
    const args = buildHostArgs("runtime", {});
    expect(args).toContain("-Action");
    expect(args[args.indexOf("-Action") + 1]).toBe("runtime");
    expect(args).not.toContain("-ProcessId");
  });

  it("window・close・menu は pid を数値の文字列で渡す", () => {
    for (const action of ["window", "close", "menu"] as const) {
      const args = buildHostArgs(action, { processId: 4242 });
      expect(args[args.indexOf("-ProcessId") + 1]).toBe("4242");
    }
  });

  it("keys は pid とキーの記法を渡す", () => {
    const args = buildHostArgs("keys", { processId: 7, keys: "{F5}" });
    expect(args[args.indexOf("-ProcessId") + 1]).toBe("7");
    expect(args[args.indexOf("-Keys") + 1]).toBe("{F5}");
  });

  it("pid が正の整数でなければ HostActionError にする(利用者の入力を通さない)", () => {
    expect(() => buildHostArgs("close", {})).toThrow(HostActionError);
    expect(() => buildHostArgs("close", { processId: 0 })).toThrow(HostActionError);
    expect(() => buildHostArgs("close", { processId: -1 })).toThrow(HostActionError);
    expect(() => buildHostArgs("close", { processId: 1.5 })).toThrow(HostActionError);
    expect(() => buildHostArgs("close", { processId: Number.NaN })).toThrow(HostActionError);
  });

  it("keys にキーの記法が無ければ HostActionError にする", () => {
    expect(() => buildHostArgs("keys", { processId: 7 })).toThrow(HostActionError);
    expect(() => buildHostArgs("keys", { processId: 7, keys: "" })).toThrow(HostActionError);
  });
});

describe("runHostAction(補助スクリプトの起動と結果の検証)", () => {
  it("powershell.exe を固定の引数・シェルなし・上限時間つきで 1 回起動する", async () => {
    exitSuccess('{"action":"window","found":true,"title":"x"}');
    await runHostAction("window", { processId: 4242 });

    expect(fakeCalls).toHaveLength(1);
    const call = firstCall();
    expect(call.file).toBe("powershell.exe");
    expect(call.args).toEqual([
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      WINDOWS_HOST_SCRIPT_PATH,
      "-Action",
      "window",
      "-ProcessId",
      "4242",
    ]);
    expect(call.options.shell).toBeFalsy();
    expect(call.options.timeout).toBe(30_000);
  });

  it("終了コードが 0 なら出力を型に写して返す", async () => {
    exitSuccess('{"action":"menu","menuOpen":true}');
    await expect(runHostAction("menu", { processId: 9 })).resolves.toEqual({ action: "menu", menuOpen: true });
  });

  it("終了コードが 1 で出力に error の行があれば、その理由を含む HostActionError にする", async () => {
    exitFailure('{"error":"X"}');
    const error = await runHostAction("runtime").catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(HostActionError);
    expect((error as HostActionError).action).toBe("runtime");
    expect((error as HostActionError).reason).toContain("X");
  });

  it("終了コードが 1 で標準出力が空なら、標準エラーの内容を理由にする", async () => {
    exitFailure("", "PowerShell の起動に失敗");
    const error = await runHostAction("close", { processId: 5 }).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(HostActionError);
    expect((error as HostActionError).reason).toContain("PowerShell の起動に失敗");
  });

  it("終了コードが 1 でも出力が正常な結果の形なら、失敗として扱う(成功に読み替えない)", async () => {
    exitFailure('{"action":"close","delivered":true,"reason":null}');
    await expect(runHostAction("close", { processId: 5 })).rejects.toBeInstanceOf(HostActionError);
  });
});

describe("bindWindowsHost(pid に束ねた窓口)", () => {
  const pid = 4242;

  it("windowTitle は -Action window と pid で起動し、タイトルを返す", async () => {
    exitSuccess('{"action":"window","found":true,"title":"Brainfuck Studio - 無題"}');
    await expect(bindWindowsHost(pid).windowTitle()).resolves.toBe("Brainfuck Studio - 無題");
    expect(firstCall().args.slice(-4)).toEqual(["-Action", "window", "-ProcessId", "4242"]);
  });

  it("requestClose は -Action close と pid で起動する", async () => {
    exitSuccess('{"action":"close","delivered":true,"reason":null}');
    await bindWindowsHost(pid).requestClose();
    expect(firstCall().args.slice(-4)).toEqual(["-Action", "close", "-ProcessId", "4242"]);
  });

  it("sendKeys は -Action keys、pid、-Keys で起動する", async () => {
    exitSuccess('{"action":"keys","delivered":true,"reason":null}');
    await bindWindowsHost(pid).sendKeys("{F5}");
    expect(firstCall().args.slice(-6)).toEqual(["-Action", "keys", "-ProcessId", "4242", "-Keys", "{F5}"]);
  });

  it("contextMenuOpen は -Action menu と pid で起動し、menuOpen を返す", async () => {
    exitSuccess('{"action":"menu","menuOpen":true}');
    await expect(bindWindowsHost(pid).contextMenuOpen()).resolves.toBe(true);
    expect(firstCall().args.slice(-4)).toEqual(["-Action", "menu", "-ProcessId", "4242"]);
  });
});
