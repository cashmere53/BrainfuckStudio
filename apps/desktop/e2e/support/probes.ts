// ブラウザの既定動作の観測(Probes)。アプリの描画後に window へリスナーを足し、
// キー入力(F5 の自動リピート・Ctrl+R など)と右クリックについて、既定動作が抑止されたか、
// 自動リピートか、対象が編集可能かを記録して読み出せるようにする(要件 8.6・8.7)。
// アプリのリスナーは window より前(対象の要素や document)で動くため、window の bubble に後から登録すると
// アプリが既定動作を抑止した後の defaultPrevented を読める。
import type { Page } from "@playwright/test";

export type ProbeRecord = {
  type: "keydown" | "contextmenu";
  key: string | null;
  repeat: boolean;
  defaultPrevented: boolean;
  targetEditable: boolean;
};

/**
 * ページ内で採取した生の記録。DOM への参照を持たず、判定だけを Node 側の純粋関数で行う。
 * editableAncestor は対象から祖先をたどった最初の contenteditable 属性の値(無ければ null)。
 */
export type RawProbeEvent = {
  type: ProbeRecord["type"];
  key: string | null;
  repeat: boolean;
  defaultPrevented: boolean;
  tagName: string;
  inputType: string | null;
  editableAncestor: string | null;
};

/** 文字を入力しない input の種類(これ以外の input は文字を入力できる扱い) */
const NON_TEXT_INPUT_TYPES: ReadonlySet<string> = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

/** 対象が編集可能か(textarea・文字の input・contenteditable の内側)。純粋関数 */
export function isEditableElement(
  raw: Pick<RawProbeEvent, "tagName" | "inputType" | "editableAncestor">,
): boolean {
  if (raw.tagName === "TEXTAREA") {
    return true;
  }
  if (raw.tagName === "INPUT") {
    return !NON_TEXT_INPUT_TYPES.has((raw.inputType ?? "text").toLowerCase());
  }
  const editable = raw.editableAncestor?.toLowerCase();
  return editable === "" || editable === "true" || editable === "plaintext-only";
}

/** 生の記録を、テストの記録へ変換する(キー以外は右クリックでは key も自動リピートも持たない)。純粋関数 */
export function toProbeRecord(raw: RawProbeEvent): ProbeRecord {
  const isKeydown = raw.type === "keydown";
  return {
    type: raw.type,
    key: isKeydown ? raw.key : null,
    repeat: isKeydown && raw.repeat,
    defaultPrevented: raw.defaultPrevented,
    targetEditable: isEditableElement(raw),
  };
}

/**
 * ページ内で実行する。page.evaluate に渡されるため自己完結させ、モジュールのスコープの値を参照しない。
 * 二重に呼ばれても登録は 1 回だけにする。
 */
export function installProbeListeners(): void {
  const probeWindow = window as unknown as {
    __bfE2eProbe?: RawProbeEvent[];
    __bfE2eProbeHandler?: EventListener;
  };
  if (probeWindow.__bfE2eProbe) {
    return;
  }
  const records: RawProbeEvent[] = [];
  const handler = (event: Event): void => {
    const element = event.target instanceof Element ? event.target : null;
    const isKeydown = event.type === "keydown";
    records.push({
      type: event.type as "keydown" | "contextmenu",
      key: isKeydown ? (event as KeyboardEvent).key : null,
      repeat: isKeydown ? (event as KeyboardEvent).repeat : false,
      defaultPrevented: event.defaultPrevented,
      tagName: element ? element.tagName : "",
      inputType: element instanceof HTMLInputElement ? element.type : null,
      editableAncestor: element
        ? (element.closest("[contenteditable]")?.getAttribute("contenteditable") ?? null)
        : null,
    });
  };
  probeWindow.__bfE2eProbe = records;
  probeWindow.__bfE2eProbeHandler = handler;
  window.addEventListener("keydown", handler);
  window.addEventListener("contextmenu", handler);
}

/** アプリの描画後に呼ぶ。アプリのリスナーより後に window へ登録される */
export async function installDefaultActionProbe(page: Page): Promise<void> {
  await page.evaluate(installProbeListeners);
}

/** 採取された記録を読み出す。記録が無ければ空の配列 */
export async function readProbe(page: Page): Promise<readonly ProbeRecord[]> {
  const raw = await page.evaluate(
    () => (window as unknown as { __bfE2eProbe?: RawProbeEvent[] }).__bfE2eProbe ?? [],
  );
  return raw.map(toProbeRecord);
}
