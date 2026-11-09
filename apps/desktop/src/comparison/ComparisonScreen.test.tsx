import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ApiError } from "../generated/ApiError";
import type { ComparisonInfoDto } from "../generated/ComparisonInfoDto";
import type { ComparisonResultDto } from "../generated/ComparisonResultDto";
import type { RunComparisonRequest } from "../generated/RunComparisonRequest";
import { apiErrorMessageJa } from "../ipc/errors";
import { createWorkspace, type WorkspaceStore } from "../state/workspace";
import { ComparisonScreen } from "./ComparisonScreen";
import { createFakeComparisonBackend, type ComparisonScript, type FakeComparisonBackend } from "./ipc";
import { DISCLAIMER_TEXT } from "./domain/definitions";

const INFO: ComparisonInfoDto = {
  maxSteps: 1000,
  maxRepetitions: 5,
  defaultRepetitions: 1,
  methods: ["naive", "optimized", "wasm"],
};

const COMPLETED: ComparisonResultDto = { kind: "completed", limit: 100, overall: "allMatch", methods: [] };

/** 共有値の既定: ソース "+."、入力 "A"(Base64 は QQ==)、上限 100 */
function makeWorkspace(): WorkspaceStore {
  const workspace = createWorkspace();
  workspace.setSource("+.");
  workspace.setInput({ mode: "text", text: "A" });
  workspace.setLimitText("100");
  return workspace;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/** 画面を描画し、比較の情報の取得が終わるまで待つ */
async function renderReady(script: ComparisonScript = {}, workspace: WorkspaceStore = makeWorkspace()) {
  const backend = createFakeComparisonBackend({ getInfo: () => INFO, ...script });
  render(<ComparisonScreen backend={backend} workspace={workspace} />);
  await screen.findByText(`比較できる命令数の上限は ${INFO.maxSteps} です`);
  const runButton = () => screen.getByRole("button", { name: "比較を実行" }) as HTMLButtonElement;
  return { backend, workspace, runButton };
}

/** 実行ボタンの無効理由(aria-describedby で結び付いた文言)を返す */
function reasonOf(button: HTMLButtonElement): string | null {
  const id = button.getAttribute("aria-describedby");
  return id === null ? null : (document.getElementById(id)?.textContent ?? null);
}

function sentRequests(backend: FakeComparisonBackend): RunComparisonRequest[] {
  return backend.calls.filter((call) => call.method === "run").map((call) => call.args[0] as RunComparisonRequest);
}

describe("ComparisonScreen: 表示と要求の導出(要件 11.1・11.2・11.9)", () => {
  it("画面表示時に比較の情報を取得し、既定の繰り返し回数を示す", async () => {
    const { backend } = await renderReady();

    expect(backend.calls.map((call) => call.method)).toEqual(["getInfo"]);
    expect((screen.getByLabelText("繰り返し回数(1〜5)") as HTMLInputElement).value).toBe("1");
  });

  it("共有値が有効なら実行でき、既定の選択(最適化・WASM)と繰り返し回数で要求を送る", async () => {
    const { backend, runButton } = await renderReady({ run: () => COMPLETED });

    expect(runButton().disabled).toBe(false);
    fireEvent.click(runButton());

    await screen.findByRole("table");
    expect(sentRequests(backend)).toEqual([
      { source: "+.", inputBase64: "QQ==", maxSteps: 100, methods: ["optimized", "wasm"], repetitions: 1 },
    ]);
  });

  it("方式の選択を要求の methods へ反映する(WASM のみ・最適化のみ・どちらも無し)", async () => {
    const { backend, runButton } = await renderReady({ run: () => COMPLETED });
    const optimized = screen.getByRole("checkbox", { name: "最適化インタプリタ" });
    const wasm = screen.getByRole("checkbox", { name: "WASM コンパイル実行" });

    fireEvent.click(optimized);
    fireEvent.click(runButton());
    await waitFor(() => expect(sentRequests(backend)).toHaveLength(1));

    fireEvent.click(optimized);
    fireEvent.click(wasm);
    fireEvent.click(runButton());
    await waitFor(() => expect(sentRequests(backend)).toHaveLength(2));

    fireEvent.click(optimized);
    fireEvent.click(runButton());
    await waitFor(() => expect(sentRequests(backend)).toHaveLength(3));

    expect(sentRequests(backend).map((request) => request.methods)).toEqual([["wasm"], ["optimized"], []]);
  });

  it("繰り返し回数の入力を要求の repetitions へ反映する", async () => {
    const { backend, runButton } = await renderReady({ run: () => COMPLETED });

    fireEvent.change(screen.getByLabelText("繰り返し回数(1〜5)"), { target: { value: "3" } });
    fireEvent.click(runButton());

    await waitFor(() => expect(sentRequests(backend)).toHaveLength(1));
    expect(sentRequests(backend)[0]?.repetitions).toBe(3);
  });
});

describe("ComparisonScreen: 実行ボタンの無効化と理由(要件 11.5〜11.7)", () => {
  it("上限なしでは実行を無効にし、理由を実行ボタンに関連付けて示す", async () => {
    const { workspace, runButton, backend } = await renderReady();

    act(() => workspace.setUnlimited(true));

    expect(runButton().disabled).toBe(true);
    expect(reasonOf(runButton())).toBe("上限なしでは比較できません。上限を指定してください");
    fireEvent.click(runButton());
    expect(sentRequests(backend)).toHaveLength(0);
  });

  it("上限が最大値を超えると実行を無効にし、理由を示す", async () => {
    const { workspace, runButton } = await renderReady();

    act(() => workspace.setLimitText("2000"));

    expect(runButton().disabled).toBe(true);
    expect(reasonOf(runButton())).toBe("命令数の上限は 1000 以下で指定してください");
  });

  it("上限が不正な整数だと実行を無効にし、理由を示す", async () => {
    const { workspace, runButton } = await renderReady();

    act(() => workspace.setLimitText("abc"));

    expect(runButton().disabled).toBe(true);
    expect(reasonOf(runButton())).toBe("命令数の上限は 1 以上の整数で指定してください");
  });

  it("入力が不正だと実行を無効にし、理由を示す", async () => {
    const { workspace, runButton } = await renderReady();

    act(() => workspace.setInput({ mode: "hex", text: "zz" }));

    expect(runButton().disabled).toBe(true);
    expect(reasonOf(runButton())).toBe("入力が正しくありません。16 進の表記を確認してください");
  });

  it.each([
    ["0", "繰り返し回数は 1 以上 5 以下で指定してください"],
    ["6", "繰り返し回数は 1 以上 5 以下で指定してください"],
    ["", "繰り返し回数は 1 以上 5 以下で指定してください"],
  ])("繰り返し回数 %j は範囲外なので実行を無効にし、理由を示す", async (text, expected) => {
    const { runButton } = await renderReady();

    fireEvent.change(screen.getByLabelText("繰り返し回数(1〜5)"), { target: { value: text } });

    expect(runButton().disabled).toBe(true);
    expect(reasonOf(runButton())).toBe(expected);
  });

  it("比較の情報を取得できないと実行を無効にし、操作エラーを示す", async () => {
    const error: ApiError = { kind: "internal", message: "", field: null };
    const backend = createFakeComparisonBackend({
      getInfo: () => {
        throw error;
      },
    });
    render(<ComparisonScreen backend={backend} workspace={makeWorkspace()} />);

    expect(await screen.findByText(apiErrorMessageJa(error))).toBeTruthy();
    expect((screen.getByRole("button", { name: "比較を実行" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ComparisonScreen: 実行中・結果・失敗・解析エラー(要件 11.3・9.6・10.9)", () => {
  it("実行中は再実行を無効にし、実行中と結果の作業中を示し、完了後に結果を表示する", async () => {
    const pending = deferred<ComparisonResultDto>();
    const { runButton, backend } = await renderReady({ run: () => pending.promise });

    fireEvent.click(runButton());

    expect(runButton().disabled).toBe(true);
    expect(screen.getByText("実行中です…")).toBeTruthy();
    expect(screen.getByRole("region", { name: "方式ごとの結果" }).getAttribute("aria-busy")).toBe("true");
    fireEvent.click(runButton());
    expect(sentRequests(backend)).toHaveLength(1);

    await act(async () => pending.resolve(COMPLETED));

    expect(screen.queryByText("実行中です…")).toBeNull();
    expect(screen.getByText(/すべて一致/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "方式ごとの結果" }).getAttribute("aria-busy")).toBe("false");
  });

  it("完了した結果は総合結果・表・実行数の凡例・免責を示す", async () => {
    const { runButton } = await renderReady({ run: () => COMPLETED });

    fireEvent.click(runButton());

    expect(await screen.findByText(/すべて一致/)).toBeTruthy();
    expect(screen.getByRole("table")).toBeTruthy();
    expect(screen.getByRole("region", { name: "実行数の凡例" })).toBeTruthy();
    expect(screen.getByText(DISCLAIMER_TEXT)).toBeTruthy();
  });

  it("解析エラーでは表を出さず、種類・命令添字・行と列を示す", async () => {
    const { runButton } = await renderReady({
      run: () => ({
        kind: "parseError",
        error: {
          kind: "unmatchedOpeningBracket",
          instructionIndex: 2,
          position: { byteOffset: 5, utf16Offset: 5, line: 1, column: 6 },
        },
      }),
    });

    fireEvent.click(runButton());

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("対応する閉じ括弧がない開き括弧");
    expect(alert.textContent).toContain("命令添字: 2");
    expect(alert.textContent).toContain("位置: 行 1・列 6");
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByText(/すべて一致/)).toBeNull();
  });

  it("実行が失敗すると操作エラーを示し、結果の表は出さない", async () => {
    const error: ApiError = { kind: "sourceTooLarge", message: "", field: null };
    const { runButton } = await renderReady({
      run: () => {
        throw error;
      },
    });

    fireEvent.click(runButton());

    expect(await screen.findByText(apiErrorMessageJa(error))).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
  });
});

describe("ComparisonScreen: 古い結果(要件 11.8)", () => {
  it("結果の後に共有値を変えると、再実行を促す注意を示す(結果は残す)", async () => {
    const workspace = makeWorkspace();
    const { runButton } = await renderReady({ run: () => COMPLETED }, workspace);
    fireEvent.click(runButton());
    await screen.findByRole("table");
    const stale = "条件が変更されています(再実行してください)";
    expect(screen.queryByText(stale)).toBeNull();

    act(() => workspace.setSource("+"));

    expect(screen.getByText(stale)).toBeTruthy();
    expect(screen.getByRole("table")).toBeTruthy();
  });
});

describe("ComparisonScreen: 操作の構成(要件 11.10)", () => {
  it("操作要素はネイティブの button・input で、フォーカスを受け取れる", async () => {
    const { runButton } = await renderReady();

    expect(runButton().tagName).toBe("BUTTON");
    expect(screen.getByRole("checkbox", { name: "最適化インタプリタ" }).tagName).toBe("INPUT");
    runButton().focus();
    expect(document.activeElement).toBe(runButton());
  });
});
