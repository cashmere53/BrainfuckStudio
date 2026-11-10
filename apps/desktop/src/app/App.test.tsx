import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ComparisonInfoDto } from "../generated/ComparisonInfoDto";
import type { ComparisonResultDto } from "../generated/ComparisonResultDto";
import { createComparisonController } from "../comparison/state/comparisonStore";
import { createFakeComparisonBackend, type ComparisonScript } from "../comparison/ipc";
import { useWorkspace, workspaceStore } from "../state/workspace";
import { App } from "./App";
import { createScreens, screens, type ScreenDefinition } from "./screens";

describe("画面レジストリ(要件 1.1・12.14・10.1)", () => {
  it("既定の登録はデバッガ画面と比較画面の 2 件で、比較は「比較」の名前で登録される", () => {
    expect(screens.map((entry) => [entry.id, entry.title])).toEqual([
      ["debugger", "デバッガ"],
      ["comparison", "比較"],
    ]);
  });

  it("比較タブを選ぶと比較画面を表示し、デバッガ画面へ戻っても操作が残る", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("tab", { name: "比較" }));

    expect(screen.getByRole("heading", { name: "比較" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "デバッガ" }));
    expect(screen.getByRole("toolbar", { name: "操作" })).toBeTruthy();
  });
});

describe("App シェル", () => {
  it("画面が 1 つの間はタブを出さず、デバッガ画面を表示する", () => {
    render(<App screens={[screens[0]!]} />);

    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByRole("toolbar", { name: "操作" })).toBeTruthy();
  });

  it("画面が複数あるときだけタブを出し、選んだ画面へ切り替える", () => {
    const extra: readonly ScreenDefinition[] = [
      { id: "first", title: "最初", component: () => <p>最初の画面</p> },
      { id: "second", title: "次", component: () => <p>次の画面</p> },
    ];
    render(<App screens={extra} />);

    expect(screen.getByRole("tablist")).toBeTruthy();
    expect(screen.getByText("最初の画面")).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "次" }));

    expect(screen.getByText("次の画面")).toBeTruthy();
    expect(screen.queryByText("最初の画面")).toBeNull();
  });

  it("後続の画面はデバッガと同じ Workspace を共有する(別の画面で変えた半径がデバッガ画面に反映される)", () => {
    const initialRadius = workspaceStore.getSnapshot().tapeRadius;
    function Sibling() {
      const snapshot = useWorkspace(workspaceStore);
      return (
        <div>
          <p>後続の半径 {snapshot.tapeRadius}</p>
          <button type="button" onClick={() => workspaceStore.setTapeRadius(3)}>
            半径を 3 にする
          </button>
        </div>
      );
    }
    try {
      render(<App screens={[...screens, { id: "sibling", title: "後続", component: Sibling }]} />);

      fireEvent.click(screen.getByRole("tab", { name: "後続" }));
      fireEvent.click(screen.getByRole("button", { name: "半径を 3 にする" }));
      expect(screen.getByText("後続の半径 3")).toBeTruthy();

      fireEvent.click(screen.getByRole("tab", { name: "デバッガ" }));
      expect((screen.getByLabelText("表示半径") as HTMLSelectElement).value).toBe("3");
    } finally {
      act(() => workspaceStore.setTapeRadius(initialRadius));
    }
  });
});

const COMPARISON_INFO: ComparisonInfoDto = {
  maxSteps: 1000,
  maxRepetitions: 5,
  defaultRepetitions: 1,
  methods: ["naive", "optimized", "wasm"],
};
const COMPARISON_DONE: ComparisonResultDto = { kind: "completed", limit: 100, overall: "allMatch", methods: [] };
const STALE_NOTICE = "条件が変更されています(再実行してください)";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/**
 * 登録の画面一覧を、偽バックエンドの比較コントローラで組んで描画する。
 * コントローラは画面の外(ここ)で作り、タブ切替でアンマウントされても保持される(要件 11.4・11.8)。
 */
function renderAppWithComparison(script: ComparisonScript) {
  const backend = createFakeComparisonBackend({ getInfo: () => COMPARISON_INFO, ...script });
  const controller = createComparisonController({ backend, workspace: workspaceStore });
  render(<App screens={createScreens({ comparisonController: controller, comparisonBackend: backend })} />);
  return { backend, controller };
}

function selectTab(name: string) {
  fireEvent.click(screen.getByRole("tab", { name }));
}

async function startComparisonRun() {
  selectTab("比較");
  await screen.findByText("比較できる命令数の上限は 1000 です");
  fireEvent.click(screen.getByRole("button", { name: "比較を実行" }));
}

describe("比較画面の状態の保持(要件 11.4・11.8)", () => {
  // 共有値は既定(上限が偽バックエンドの最大値を超える)のままだと実行できないため、比較可能な値に固定して元に戻す
  let saved: { source: string; limitText: string } | null = null;
  beforeEach(() => {
    const snapshot = workspaceStore.getSnapshot();
    saved = { source: snapshot.source, limitText: snapshot.limit.text };
    act(() => {
      workspaceStore.setSource("+.");
      workspaceStore.setLimitText("100");
    });
  });
  afterEach(() => {
    if (saved !== null) {
      const restore = saved;
      act(() => {
        workspaceStore.setSource(restore.source);
        workspaceStore.setLimitText(restore.limitText);
      });
    }
  });

  it("比較で実行した結果は、デバッガへ切り替えて戻っても残る", async () => {
    renderAppWithComparison({ run: () => COMPARISON_DONE });
    await startComparisonRun();
    await screen.findByRole("table");

    selectTab("デバッガ");
    selectTab("比較");

    expect(screen.getByRole("table")).toBeTruthy();
    expect(screen.getByText(/すべて一致/)).toBeTruthy();
  });

  it("実行中に別のタブへ行っても実行は続き、応答が届いてから戻ると結果が表示される", async () => {
    const pending = deferred<ComparisonResultDto>();
    renderAppWithComparison({ run: () => pending.promise });
    await startComparisonRun();
    expect(screen.getByText("実行中です…")).toBeTruthy();

    selectTab("デバッガ");
    await act(async () => pending.resolve(COMPARISON_DONE));
    selectTab("比較");

    expect(screen.queryByText("実行中です…")).toBeNull();
    expect(screen.getByRole("table")).toBeTruthy();
    expect(screen.getByText(/すべて一致/)).toBeTruthy();
  });

  it("デバッガで共有値(ソース)を変えて比較へ戻ると、古い結果の注意を示す", async () => {
    renderAppWithComparison({ run: () => COMPARISON_DONE });
    await startComparisonRun();
    await screen.findByRole("table");
    expect(screen.queryByText(STALE_NOTICE)).toBeNull();

    selectTab("デバッガ");
    act(() => workspaceStore.setSource("+"));
    selectTab("比較");

    expect(screen.getByText(STALE_NOTICE)).toBeTruthy();
    expect(screen.getByRole("table")).toBeTruthy();
  });
});

describe("比較画面の方式と繰り返し回数の保持(タブ切替で既定に戻らない)", () => {
  const REPETITIONS_LABEL = "繰り返し回数(1〜5)";
  const REPETITIONS_RANGE_REASON = "繰り返し回数は 1 以上 5 以下で指定してください";

  function repetitionsInput() {
    return screen.getByLabelText(REPETITIONS_LABEL) as HTMLInputElement;
  }

  async function openComparison() {
    selectTab("比較");
    await screen.findByText("比較できる命令数の上限は 1000 です");
  }

  let saved: { source: string; limitText: string } | null = null;
  beforeEach(() => {
    const snapshot = workspaceStore.getSnapshot();
    saved = { source: snapshot.source, limitText: snapshot.limit.text };
    act(() => {
      workspaceStore.setSource("+.");
      workspaceStore.setLimitText("100");
    });
  });
  afterEach(() => {
    if (saved !== null) {
      const restore = saved;
      act(() => {
        workspaceStore.setSource(restore.source);
        workspaceStore.setLimitText(restore.limitText);
      });
    }
  });

  it("方式のチェックと繰り返し回数を変えて、デバッガへ行って戻っても保持される", async () => {
    renderAppWithComparison({ run: () => COMPARISON_DONE });
    await openComparison();
    fireEvent.click(screen.getByLabelText("最適化インタプリタ"));
    fireEvent.change(repetitionsInput(), { target: { value: "4" } });

    selectTab("デバッガ");
    await openComparison();

    // 片方だけ外して、チェックの対応が取り違えられていないことも確かめる
    expect((screen.getByLabelText("最適化インタプリタ") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByLabelText("WASM コンパイル実行") as HTMLInputElement).checked).toBe(true);
    expect(repetitionsInput().value).toBe("4");
  });

  it("不正な回数(0・6・空欄)を入力したまま往復しても、入力と無効理由が残る", async () => {
    renderAppWithComparison({ run: () => COMPARISON_DONE });
    await openComparison();

    for (const invalid of ["0", "6", ""]) {
      fireEvent.change(repetitionsInput(), { target: { value: invalid } });
      selectTab("デバッガ");
      await openComparison();

      expect(repetitionsInput().value).toBe(invalid);
      expect(screen.getByRole("button", { name: "比較を実行" }).hasAttribute("disabled")).toBe(true);
      expect(screen.getByText(REPETITIONS_RANGE_REASON)).toBeTruthy();
    }
  });

  it("利用者が触っていないときは、取得した既定の繰り返し回数を表示する", async () => {
    renderAppWithComparison({ run: () => COMPARISON_DONE, getInfo: () => ({ ...COMPARISON_INFO, defaultRepetitions: 3 }) });
    await openComparison();

    expect(repetitionsInput().value).toBe("3");
    expect((screen.getByLabelText("最適化インタプリタ") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("WASM コンパイル実行") as HTMLInputElement).checked).toBe(true);
  });

  it("利用者が編集した回数は、タブ往復で情報を再取得しても既定値で上書きされない", async () => {
    let fetches = 0;
    renderAppWithComparison({
      run: () => COMPARISON_DONE,
      getInfo: () => {
        fetches += 1;
        return { ...COMPARISON_INFO, defaultRepetitions: fetches === 1 ? 1 : 3 };
      },
    });
    await openComparison();
    fireEvent.change(repetitionsInput(), { target: { value: "4" } });

    selectTab("デバッガ");
    await openComparison();

    expect(fetches).toBe(2);
    expect(repetitionsInput().value).toBe("4");
  });

  it("既定の回数のままなら、再取得で既定値の変更が反映される", async () => {
    let fetches = 0;
    renderAppWithComparison({
      run: () => COMPARISON_DONE,
      getInfo: () => {
        fetches += 1;
        return { ...COMPARISON_INFO, defaultRepetitions: fetches === 1 ? 1 : 3 };
      },
    });
    await openComparison();
    expect(repetitionsInput().value).toBe("1");

    selectTab("デバッガ");
    await openComparison();

    expect(repetitionsInput().value).toBe("3");
  });

  it("保持された方式と回数で比較を実行すると、その選択で要求が送られる", async () => {
    const requests: unknown[] = [];
    renderAppWithComparison({
      run: (request) => {
        requests.push(request);
        return COMPARISON_DONE;
      },
    });
    await openComparison();
    fireEvent.click(screen.getByLabelText("WASM コンパイル実行"));
    fireEvent.change(repetitionsInput(), { target: { value: "2" } });
    selectTab("デバッガ");
    await openComparison();

    fireEvent.click(screen.getByRole("button", { name: "比較を実行" }));
    await screen.findByRole("table");

    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatchObject({ repetitions: 2, methods: ["optimized"] });
  });
});
