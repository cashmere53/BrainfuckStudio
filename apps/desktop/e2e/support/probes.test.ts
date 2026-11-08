// 既定動作の観測(Probes)の単体テスト。jsdom の要素とイベントで、抑止の有無・自動リピート・編集可能の判定を確かめる。
// installProbeListeners は page.evaluate に渡して実ブラウザでも動くよう自己完結しているため、
// ここでは jsdom の window 上で直接呼んで同じコードを検証する。
import { afterEach, describe, expect, it } from "vitest";
import {
  installDefaultActionProbe,
  installProbeListeners,
  isEditableElement,
  readProbe,
  toProbeRecord,
  type RawProbeEvent,
} from "./probes.ts";

type ProbeWindow = Window & {
  __bfE2eProbe?: RawProbeEvent[];
  __bfE2eProbeHandler?: EventListener;
};

// 各テストの後に、記録・リスナー・登録の印を外す(リスナーはページ内に残るため)
afterEach(() => {
  const probeWindow = window as ProbeWindow;
  if (probeWindow.__bfE2eProbeHandler) {
    window.removeEventListener("keydown", probeWindow.__bfE2eProbeHandler);
    window.removeEventListener("contextmenu", probeWindow.__bfE2eProbeHandler);
  }
  delete probeWindow.__bfE2eProbeHandler;
  delete probeWindow.__bfE2eProbe;
  document.body.innerHTML = "";
});

function raw(overrides: Partial<RawProbeEvent>): RawProbeEvent {
  return {
    type: "keydown",
    key: "a",
    repeat: false,
    defaultPrevented: false,
    tagName: "DIV",
    inputType: null,
    editableAncestor: null,
    ...overrides,
  };
}

describe("isEditableElement(編集可能かの判定)", () => {
  it("textarea は編集可能", () => {
    expect(isEditableElement(raw({ tagName: "TEXTAREA" }))).toBe(true);
  });

  it("type を省いた input と text 系の input は編集可能", () => {
    expect(isEditableElement(raw({ tagName: "INPUT", inputType: null }))).toBe(true);
    expect(isEditableElement(raw({ tagName: "INPUT", inputType: "text" }))).toBe(true);
    expect(isEditableElement(raw({ tagName: "INPUT", inputType: "search" }))).toBe(true);
  });

  it("ボタン系・選択系の input は文字を編集しないので編集可能ではない", () => {
    for (const inputType of ["button", "checkbox", "radio", "range", "submit", "file"]) {
      expect(isEditableElement(raw({ tagName: "INPUT", inputType }))).toBe(false);
    }
  });

  it("contenteditable の祖先(値が空・true・plaintext-only)は編集可能", () => {
    for (const editableAncestor of ["", "true", "plaintext-only"]) {
      expect(isEditableElement(raw({ tagName: "SPAN", editableAncestor }))).toBe(true);
    }
  });

  it("contenteditable=false と祖先が無い要素は編集可能ではない", () => {
    expect(isEditableElement(raw({ tagName: "SPAN", editableAncestor: "false" }))).toBe(false);
    expect(isEditableElement(raw({ tagName: "DIV", editableAncestor: null }))).toBe(false);
  });
});

describe("toProbeRecord(イベントから記録への変換)", () => {
  it("keydown の記録は key・自動リピート・抑止の有無・編集可能の判定を持つ", () => {
    const record = toProbeRecord(
      raw({ type: "keydown", key: "F5", repeat: true, defaultPrevented: true, tagName: "DIV" }),
    );
    expect(record).toEqual({
      type: "keydown",
      key: "F5",
      repeat: true,
      defaultPrevented: true,
      targetEditable: false,
    });
  });

  it("contextmenu の記録は key を持たず、自動リピートは false", () => {
    const record = toProbeRecord(
      raw({ type: "contextmenu", key: null, repeat: false, defaultPrevented: false, tagName: "TEXTAREA" }),
    );
    expect(record).toEqual({
      type: "contextmenu",
      key: null,
      repeat: false,
      defaultPrevented: false,
      targetEditable: true,
    });
  });
});

describe("installProbeListeners(ページ内の記録)", () => {
  it("アプリより後に登録されるため、アプリが既定動作を抑止したあとの defaultPrevented を記録する", () => {
    const appListener = (event: Event): void => {
      event.preventDefault();
    };
    window.addEventListener("keydown", appListener);
    installProbeListeners();

    const target = document.createElement("div");
    document.body.append(target);
    target.dispatchEvent(new KeyboardEvent("keydown", { key: "r", ctrlKey: true, cancelable: true, bubbles: true }));

    const records = (window as ProbeWindow).__bfE2eProbe ?? [];
    expect(records).toHaveLength(1);
    expect(records[0]?.defaultPrevented).toBe(true);
    window.removeEventListener("keydown", appListener);
  });

  it("抑止されなければ defaultPrevented は false のまま記録される", () => {
    installProbeListeners();
    const target = document.createElement("div");
    document.body.append(target);
    target.dispatchEvent(new KeyboardEvent("keydown", { key: "F5", cancelable: true, bubbles: true }));
    expect((window as ProbeWindow).__bfE2eProbe?.[0]?.defaultPrevented).toBe(false);
  });

  it("自動リピートの keydown を repeat として記録する", () => {
    installProbeListeners();
    const target = document.createElement("div");
    document.body.append(target);
    target.dispatchEvent(new KeyboardEvent("keydown", { key: "F5", repeat: true, bubbles: true }));
    expect((window as ProbeWindow).__bfE2eProbe?.[0]?.repeat).toBe(true);
  });

  it("入力欄の上での keydown は編集可能として記録する", () => {
    installProbeListeners();
    const input = document.createElement("input");
    document.body.append(input);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
    expect((window as ProbeWindow).__bfE2eProbe?.[0]?.tagName).toBe("INPUT");
    expect(readRecord(0).targetEditable).toBe(true);
  });

  it("contenteditable の子要素の上での右クリックは編集可能として記録する", () => {
    installProbeListeners();
    const host = document.createElement("div");
    host.setAttribute("contenteditable", "true");
    const child = document.createElement("span");
    host.append(child);
    document.body.append(host);
    child.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    const record = readRecord(0);
    expect(record.type).toBe("contextmenu");
    expect(record.targetEditable).toBe(true);
  });

  it("編集可能でない要素の上での右クリックは編集可能ではないと記録する", () => {
    installProbeListeners();
    const div = document.createElement("div");
    document.body.append(div);
    div.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    expect(readRecord(0).targetEditable).toBe(false);
  });

  it("二重に登録されても記録は増えない", () => {
    installProbeListeners();
    installProbeListeners();
    const div = document.createElement("div");
    document.body.append(div);
    div.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
    expect((window as ProbeWindow).__bfE2eProbe).toHaveLength(1);
  });
});

describe("installDefaultActionProbe / readProbe(ページの操作口)", () => {
  // Playwright の Page は page.evaluate の関数を実行するので、jsdom では window で直接動かす
  const fakePage = {
    evaluate: async <T>(fn: () => T): Promise<T> => fn(),
  };

  it("インストール後に読み出すと、記録を組み立てた結果が返る", async () => {
    await installDefaultActionProbe(fakePage as never);
    const div = document.createElement("div");
    document.body.append(div);
    div.dispatchEvent(new KeyboardEvent("keydown", { key: "F5", repeat: true, bubbles: true }));
    const records = await readProbe(fakePage as never);
    expect(records).toEqual([
      { type: "keydown", key: "F5", repeat: true, defaultPrevented: false, targetEditable: false },
    ]);
  });

  it("記録が無いときは空の配列を返す", async () => {
    const records = await readProbe(fakePage as never);
    expect(records).toEqual([]);
  });
});

function readRecord(index: number) {
  const record = toProbeRecord((window as ProbeWindow).__bfE2eProbe?.[index] ?? raw({}));
  return record;
}
