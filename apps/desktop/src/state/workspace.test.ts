import { act, renderHook } from "@testing-library/react";
import { describe, expect, expectTypeOf, it, vi } from "vitest";
import { getHelloSource } from "../samples/hello";
import { DEFAULT_MAX_STEPS, DEFAULT_TAPE_RADIUS, MAX_TAPE_RADIUS } from "../domain/limits";
import { DEFAULT_STEP_LIMIT_TEXT } from "../domain/stepLimit";
import {
  createWorkspace,
  useWorkspace,
  type WorkspaceState,
  type WorkspaceStore,
  type WorkspaceValues,
} from "./workspace";

describe("createWorkspace: 初期状態", () => {
  it("起動時のソースは取り込んだ Hello World サンプルと一致し、未保存でもファイル名もない", () => {
    const ws = createWorkspace();
    const state = ws.getSnapshot();
    expect(state.source).toBe(getHelloSource().replace(/\r\n?/g, "\n"));
    expect(state.source).not.toContain("\r");
    expect(state.document).toEqual({ fileName: null, dirty: false, lossyNotice: false });
  });

  it("入力は空のテキスト、上限は既定値(10,000,000)、テープ半径と出力モードと BP は既定値", () => {
    const state = createWorkspace().getSnapshot();
    expect(state.input).toEqual({ mode: "text", text: "" });
    expect(state.limit).toEqual({
      text: DEFAULT_STEP_LIMIT_TEXT,
      unlimited: false,
      lastValid: { kind: "limited", maxSteps: DEFAULT_MAX_STEPS },
    });
    expect(state.tapeRadius).toBe(DEFAULT_TAPE_RADIUS);
    expect(state.outputMode).toBe("text");
    expect(state.breakpoints).toEqual([]);
  });

  it("WorkspaceValues はソース・入力・上限の 3 項目だけを持つ型である", () => {
    expectTypeOf<WorkspaceValues>().toEqualTypeOf<Pick<WorkspaceState, "source" | "input" | "limit">>();
    const values: WorkspaceValues = createWorkspace().getSnapshot();
    expect(Object.keys(values).sort()).toEqual(expect.arrayContaining(["source", "input", "limit"]));
  });
});

describe("編集と未保存表示", () => {
  it("ソースを編集すると未保存になり、保存済みにすると解除される", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    expect(ws.getSnapshot().source).toBe("+");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.markSaved(ws.getSnapshot().source);
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("同じ内容の設定では変更通知を出さない", () => {
    const ws = createWorkspace();
    const listener = vi.fn();
    ws.subscribe(listener);
    ws.setSource(ws.getSnapshot().source);
    expect(listener).not.toHaveBeenCalled();
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("名前を付けて保存では保存先のファイル名を紐づけて未保存を解除する", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.markSaved(ws.getSnapshot().source, "hello.bf");
    expect(ws.getSnapshot().document).toEqual({ fileName: "hello.bf", dirty: false, lossyNotice: false });
  });

  it("保存時点のソースと現在のソースが異なれば未保存のまま残し、ファイル名だけ更新する(保存中の編集)", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    const sent = ws.getSnapshot().source;
    ws.setSource("+.");
    ws.markSaved(sent, "new.bf");
    expect(ws.getSnapshot().document).toEqual({ fileName: "new.bf", dirty: true, lossyNotice: false });
  });

  it("保存時点のソースと現在のソースが一致すれば未保存を解除する", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.markSaved(ws.getSnapshot().source, "new.bf");
    expect(ws.getSnapshot().document).toEqual({ fileName: "new.bf", dirty: false, lossyNotice: false });
  });

  it("入力の変更はソースの未保存フラグを変えない", () => {
    const ws = createWorkspace();
    ws.setInput({ mode: "hex", text: "41" });
    expect(ws.getSnapshot().input).toEqual({ mode: "hex", text: "41" });
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });
});

describe("未保存表示: 基準のソースとの比較(要件 2.2)", () => {
  // 未保存は「編集領域の内容が最後に開いた/保存した内容と異なる間」だけ示す。取り消しで基準と同じ内容に戻れば解除する

  it("編集中は未保存、基準のソースと同じ内容に戻すと未保存が解除される", () => {
    const ws = createWorkspace();
    const original = ws.getSnapshot().source;
    ws.setSource(`${original}+`);
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource(original);
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("保存後に編集して元の保存内容へ戻すと未保存が解除される", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.markSaved(ws.getSnapshot().source, "a.bf");
    ws.setSource("+.");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("+");
    expect(ws.getSnapshot().document).toEqual({ fileName: "a.bf", dirty: false, lossyNotice: false });
  });

  it("保存で基準は保存時点のソースになり、その後の差分だけが未保存になる", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.markSaved("+", "a.bf");
    ws.setSource("++");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("+");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("保存の応答前の編集(保存中の編集)は、基準との差分として未保存のまま残る", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    const sent = ws.getSnapshot().source;
    ws.setSource("+.");
    ws.markSaved(sent, "a.bf");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    // 保存中の編集を元へ戻すと、保存時点と一致するので解除される
    ws.setSource("+");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("名前を付けて保存でも基準は保存時点のソースになる", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.markSaved("+", "b.bf");
    ws.setSource("+");
    expect(ws.getSnapshot().document.dirty).toBe(false);
    ws.setSource("-");
    expect(ws.getSnapshot().document.dirty).toBe(true);
  });

  it("開く(replaceDocument)で基準は置換後のソースになり、編集して元に戻すと未保存が解除される", () => {
    const ws = createWorkspace();
    ws.setSource("edited");
    ws.replaceDocument({ source: "opened", fileName: "a.bf", lossy: false });
    expect(ws.getSnapshot().document.dirty).toBe(false);
    ws.setSource("opened!");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("opened");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("新規(newDocument)で基準は空のソースになり、編集して空に戻すと未保存が解除される", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.newDocument();
    expect(ws.getSnapshot().document.dirty).toBe(false);
    ws.setSource("+");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("起動時のサンプルに編集して元に戻すと、無題のまま未保存は解除される", () => {
    const ws = createWorkspace();
    const sample = ws.getSnapshot().source;
    ws.setSource(`${sample}.`);
    expect(ws.getSnapshot().document).toEqual({ fileName: null, dirty: true, lossyNotice: false });
    ws.setSource(sample);
    expect(ws.getSnapshot().document).toEqual({ fileName: null, dirty: false, lossyNotice: false });
  });

  it("Shift_JIS などを開いたときの置換文字を含むソースが基準になる(lossyNotice は変えない)", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: "a�", fileName: "sjis.bf", lossy: true });
    expect(ws.getSnapshot().document).toEqual({ fileName: "sjis.bf", dirty: false, lossyNotice: true });
    ws.setSource("a");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("a�");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });
  it("CRLF を含む開いた文書も LF に正規化した基準と比べ、元に戻すと未保存が解除される(k2)", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: "a\r\nb", fileName: "x.bf", lossy: false });
    expect(ws.getSnapshot().baselineSource).toBe("a\nb");
    ws.setSource("a\nb!");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("a\nb");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("保存時点のソースが CRLF でも基準は LF に正規化され、同じ内容なら未保存は解除される(k1)", () => {
    const ws = createWorkspace();
    ws.setSource("a\nb");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.markSaved("a\r\nb", "x.bf");
    expect(ws.getSnapshot().baselineSource).toBe("a\nb");
    expect(ws.getSnapshot().document).toEqual({ fileName: "x.bf", dirty: false, lossyNotice: false });
  });

  it("基準が LF で編集内容が CRLF でも、正規化後に基準と一致すれば未保存は解除される(k4)", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: "a\nb", fileName: "x.bf", lossy: false });
    ws.setSource("a\nb!");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    ws.setSource("a\r\nb");
    expect(ws.getSnapshot().source).toBe("a\nb");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("基準だけが変わる保存(dirty と紐づけ先は同じ)でも基準が更新され、後の比較に反映される(l)", () => {
    const ws = createWorkspace();
    ws.setSource("+");
    ws.setSource("++");
    ws.markSaved("+", "a.bf");
    expect(ws.getSnapshot().document).toEqual({ fileName: "a.bf", dirty: true, lossyNotice: false });
    // dirty(true)と fileName("a.bf")は変わらず、基準だけ "+!" に変わる
    ws.markSaved("+!", "a.bf");
    expect(ws.getSnapshot().document).toEqual({ fileName: "a.bf", dirty: true, lossyNotice: false });
    expect(ws.getSnapshot().baselineSource).toBe("+!");
    ws.setSource("+!");
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });
});

describe("文書の置換(新規・開く)", () => {
  it("置換でソース・ファイル名・置換通知が更新され、未保存は解除される", () => {
    const ws = createWorkspace();
    ws.setSource("edited");
    ws.replaceDocument({ source: "opened", fileName: "a.bf", lossy: true });
    expect(ws.getSnapshot().source).toBe("opened");
    expect(ws.getSnapshot().document).toEqual({ fileName: "a.bf", dirty: false, lossyNotice: true });
  });

  it("置換でブレークポイントが空になる", () => {
    const ws = createWorkspace();
    ws.setBreakpoints([3, 1]);
    ws.replaceDocument({ source: "+", fileName: null, lossy: false });
    expect(ws.getSnapshot().breakpoints).toEqual([]);
  });

  it("新規文書は空のソースでファイル名なしになり、ブレークポイントも空になる", () => {
    const ws = createWorkspace();
    ws.setBreakpoints([0]);
    ws.newDocument();
    expect(ws.getSnapshot().source).toBe("");
    expect(ws.getSnapshot().document).toEqual({ fileName: null, dirty: false, lossyNotice: false });
    expect(ws.getSnapshot().breakpoints).toEqual([]);
  });
});

describe("ブレークポイント・テープ半径・出力モード", () => {
  it("ブレークポイント位置は昇順・重複なしで保持し、未保存フラグを変えない", () => {
    const ws = createWorkspace();
    ws.setBreakpoints([5, 2, 5, 0]);
    expect(ws.getSnapshot().breakpoints).toEqual([0, 2, 5]);
    expect(ws.getSnapshot().document.dirty).toBe(false);
  });

  it("テープ半径は 0〜64 の整数のみ受け付け、範囲外は RangeError", () => {
    const ws = createWorkspace();
    ws.setTapeRadius(0);
    expect(ws.getSnapshot().tapeRadius).toBe(0);
    ws.setTapeRadius(MAX_TAPE_RADIUS);
    expect(ws.getSnapshot().tapeRadius).toBe(MAX_TAPE_RADIUS);
    expect(() => ws.setTapeRadius(MAX_TAPE_RADIUS + 1)).toThrow(RangeError);
    expect(() => ws.setTapeRadius(-1)).toThrow(RangeError);
    expect(() => ws.setTapeRadius(1.5)).toThrow(RangeError);
    expect(ws.getSnapshot().tapeRadius).toBe(MAX_TAPE_RADIUS);
  });

  it("出力表示モードを切り替えられる", () => {
    const ws = createWorkspace();
    ws.setOutputMode("hex");
    expect(ws.getSnapshot().outputMode).toBe("hex");
  });
});

describe("命令数上限の入力", () => {
  it("有効な値は入力欄と直前の有効な上限の両方を更新する", () => {
    const ws = createWorkspace();
    ws.setLimitText("500");
    expect(ws.getSnapshot().limit).toEqual({
      text: "500",
      unlimited: false,
      lastValid: { kind: "limited", maxSteps: 500 },
    });
  });

  it("不正な値は入力欄にだけ反映し、直前の有効な上限を保持する", () => {
    const ws = createWorkspace();
    ws.setLimitText("500");
    ws.setLimitText("0");
    expect(ws.getSnapshot().limit.text).toBe("0");
    expect(ws.getSnapshot().limit.lastValid).toEqual({ kind: "limited", maxSteps: 500 });
    ws.setLimitText("1.5");
    expect(ws.getSnapshot().limit.lastValid).toEqual({ kind: "limited", maxSteps: 500 });
  });

  it("「上限なし」の切替は直前の有効な上限を消さない", () => {
    const ws = createWorkspace();
    ws.setLimitText("500");
    ws.setUnlimited(true);
    expect(ws.getSnapshot().limit.unlimited).toBe(true);
    expect(ws.getSnapshot().limit.lastValid).toEqual({ kind: "limited", maxSteps: 500 });
    ws.setUnlimited(false);
    expect(ws.getSnapshot().limit.unlimited).toBe(false);
  });
});

describe("購読", () => {
  it("変更のたびに購読者へ通知し、解除後は通知しない", () => {
    const ws = createWorkspace();
    const listener = vi.fn();
    const unsubscribe = ws.subscribe(listener);
    ws.setSource("+");
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    ws.setSource("-");
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("スナップショットは変更時にだけ新しい参照になる", () => {
    const ws = createWorkspace();
    const before = ws.getSnapshot();
    ws.setSource(before.source);
    expect(ws.getSnapshot()).toBe(before);
    ws.setTapeRadius(3);
    expect(ws.getSnapshot()).not.toBe(before);
  });
});

describe("useWorkspace(読み取り用フック)", () => {
  it("ストアの変更で再描画され、最新のスナップショットを返す", () => {
    const ws = createWorkspace();
    const { result } = renderHook(() => useWorkspace(ws));
    expect(result.current.source).toBe(getHelloSource().replace(/\r\n?/g, "\n"));
    act(() => {
      ws.setSource("+");
    });
    expect(result.current.source).toBe("+");
    expect(result.current.document.dirty).toBe(true);
  });
});

describe("無変更ガード: 同じ値の設定は通知もスナップショット更新もしない", () => {
  type GuardCase = {
    name: string;
    /** 比較の基準となる状態を作る(購読前に呼ぶ) */
    prepare?: (ws: WorkspaceStore) => void;
    /** 現在値と同じ値を設定する */
    same: (ws: WorkspaceStore) => void;
    /** 現在値と異なる値を設定する */
    change: (ws: WorkspaceStore) => void;
  };

  const guardCases: GuardCase[] = [
    {
      name: "setSource",
      same: (ws) => ws.setSource(ws.getSnapshot().source),
      change: (ws) => ws.setSource("+"),
    },
    {
      name: "markSaved",
      same: (ws) => ws.markSaved(ws.getSnapshot().source),
      change: (ws) => ws.markSaved(ws.getSnapshot().source, "a.bf"),
    },
    {
      name: "setInput",
      same: (ws) => ws.setInput({ mode: "text", text: "" }),
      change: (ws) => ws.setInput({ mode: "hex", text: "41" }),
    },
    {
      name: "setLimitText",
      same: (ws) => ws.setLimitText(DEFAULT_STEP_LIMIT_TEXT),
      change: (ws) => ws.setLimitText("500"),
    },
    {
      name: "setUnlimited",
      same: (ws) => ws.setUnlimited(false),
      change: (ws) => ws.setUnlimited(true),
    },
    {
      name: "setTapeRadius",
      same: (ws) => ws.setTapeRadius(DEFAULT_TAPE_RADIUS),
      change: (ws) => ws.setTapeRadius(DEFAULT_TAPE_RADIUS + 1),
    },
    {
      name: "setOutputMode",
      same: (ws) => ws.setOutputMode("text"),
      change: (ws) => ws.setOutputMode("hex"),
    },
    {
      name: "setBreakpoints",
      prepare: (ws) => ws.setBreakpoints([1, 3]),
      // 正規化(ソート・重複排除)後に現在値と等しくなる入力も無変更
      same: (ws) => ws.setBreakpoints([3, 1, 3]),
      change: (ws) => ws.setBreakpoints([1, 2, 3]),
    },
  ];

  it.each(guardCases)("$name: 同じ値では通知されず、スナップショットの参照も変わらない", ({ prepare, same }) => {
    const ws = createWorkspace();
    prepare?.(ws);
    const before = ws.getSnapshot();
    const listener = vi.fn();
    ws.subscribe(listener);
    same(ws);
    expect(listener).not.toHaveBeenCalled();
    expect(ws.getSnapshot()).toBe(before);
  });

  it.each(guardCases)("$name: 値が変わるときは通知がちょうど 1 回で、スナップショットの参照も変わる", ({ prepare, change }) => {
    const ws = createWorkspace();
    prepare?.(ws);
    const before = ws.getSnapshot();
    const listener = vi.fn();
    ws.subscribe(listener);
    change(ws);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(ws.getSnapshot()).not.toBe(before);
  });
});

describe("置換イベントは同じ内容でも必ず 1 回通知する", () => {
  it("replaceDocument は同じ内容・同じファイル名でも通知をちょうど 1 回行う", () => {
    const ws = createWorkspace();
    const current = ws.getSnapshot();
    const listener = vi.fn();
    ws.subscribe(listener);
    ws.replaceDocument({ source: current.source, fileName: null, lossy: false });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(ws.getSnapshot()).not.toBe(current);
  });

  it("newDocument は既に空の新規文書でも通知をちょうど 1 回行う", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: "", fileName: null, lossy: false });
    const before = ws.getSnapshot();
    const listener = vi.fn();
    ws.subscribe(listener);
    ws.newDocument();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(ws.getSnapshot()).not.toBe(before);
  });
});

describe("改行の正規化(ソースは LF に統一する)", () => {
  it("replaceDocument は CRLF と CR を LF に正規化して保持する", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: "+\r\n-\r.", fileName: "a.bf", lossy: false });
    expect(ws.getSnapshot().source).toBe("+\n-\n.");
  });

  it("newDocument は空のソースのまま正規化の対象外で、置換回数は進む", () => {
    const ws = createWorkspace();
    ws.newDocument();
    expect(ws.getSnapshot().source).toBe("");
    expect(ws.getSnapshot().documentRevision).toBe(1);
  });

  it("setSource も同じ正規化を通し、正規化後に同じ内容なら変更通知を出さない", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: "+\n-", fileName: null, lossy: false });
    const listener = vi.fn();
    ws.subscribe(listener);
    ws.setSource("+\r\n-");
    expect(listener).not.toHaveBeenCalled();
    expect(ws.getSnapshot().document.dirty).toBe(false);
    ws.setSource("+\r\n-+");
    expect(ws.getSnapshot().source).toBe("+\n-+");
    expect(ws.getSnapshot().document.dirty).toBe(true);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("useWorkspace のスナップショットの安定性", () => {
  it("ストアが変わっていない再描画では同じ参照を返す", () => {
    const ws = createWorkspace();
    const { result, rerender } = renderHook(() => useWorkspace(ws));
    const first = result.current;
    rerender();
    expect(result.current).toBe(first);
  });

  it("無変更の設定では再描画の参照も変わらず、変更時にだけ新しい参照になる", () => {
    const ws = createWorkspace();
    const { result, rerender } = renderHook(() => useWorkspace(ws));
    const first = result.current;
    act(() => {
      ws.setSource(first.source);
    });
    expect(result.current).toBe(first);
    act(() => {
      ws.setTapeRadius(3);
    });
    const second = result.current;
    expect(second).not.toBe(first);
    rerender();
    expect(result.current).toBe(second);
  });
});

describe("文書の置換回数 documentRevision(置換でだけ進む)", () => {
  it("初期値は 0", () => {
    expect(createWorkspace().getSnapshot().documentRevision).toBe(0);
  });

  it("replaceDocument と newDocument は同じ内容でも回数を 1 ずつ進める", () => {
    const ws = createWorkspace();
    ws.replaceDocument({ source: ws.getSnapshot().source, fileName: null, lossy: false });
    expect(ws.getSnapshot().documentRevision).toBe(1);
    ws.newDocument();
    expect(ws.getSnapshot().documentRevision).toBe(2);
  });

  const otherSetters: readonly [string, (ws: WorkspaceStore) => void][] = [
    ["setSource", (ws) => ws.setSource("+")],
    ["markSaved", (ws) => ws.markSaved(ws.getSnapshot().source, "a.bf")],
    ["setInput", (ws) => ws.setInput({ mode: "hex", text: "41" })],
    ["setLimitText", (ws) => ws.setLimitText("500")],
    ["setUnlimited", (ws) => ws.setUnlimited(true)],
    ["setTapeRadius", (ws) => ws.setTapeRadius(3)],
    ["setOutputMode", (ws) => ws.setOutputMode("hex")],
    ["setBreakpoints", (ws) => ws.setBreakpoints([1])],
  ];

  it.each(otherSetters)("%s は回数を進めない", (_name, change) => {
    const ws = createWorkspace();
    change(ws);
    expect(ws.getSnapshot().documentRevision).toBe(0);
  });
});
