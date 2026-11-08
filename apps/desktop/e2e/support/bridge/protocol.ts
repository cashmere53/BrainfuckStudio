// ブリッジ(crates/brainfuck-e2e-bridge/src/protocol.rs)と同じ行の型と、検証つきの読み書き。
// 1 行 = 1 つの JSON オブジェクト。形の違う行は ProtocolError にする。
// 純粋な関数だけを置く(プロセスや転送路には触れない)。

export type DialogKind = "open" | "save";
export type DialogResult = { kind: "path"; path: string } | { kind: "cancel" };

export type ToBridge =
  | { type: "request"; id: number; cmd: string; payload: unknown }
  | { type: "dialogResult"; id: number; result: DialogResult };

export type FromBridge =
  | { type: "response"; id: number; ok: true; value: unknown }
  | { type: "response"; id: number; ok: false; error: unknown }
  | { type: "dialog"; id: number; kind: DialogKind };

export class ProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ProtocolError";
  }
}

type Obj = Record<string, unknown>;

function isObject(value: unknown): value is Obj {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isId(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** 許すキーだけを持つか。余分なキーは形の違いとして扱う */
function onlyKeys(obj: Obj, allowed: readonly string[], context: string): void {
  for (const key of Object.keys(obj)) {
    if (!allowed.includes(key)) {
      throw new ProtocolError(`${context}: 想定外のキー "${key}" があります`);
    }
  }
}

function hasKey(obj: Obj, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

function checkDialogResult(value: unknown, context: string): void {
  if (!isObject(value)) {
    throw new ProtocolError(`${context}: result はオブジェクトである必要があります`);
  }
  if (value.kind === "cancel") {
    onlyKeys(value, ["kind"], `${context}.result`);
    return;
  }
  if (value.kind === "path") {
    onlyKeys(value, ["kind", "path"], `${context}.result`);
    if (typeof value.path !== "string") {
      throw new ProtocolError(`${context}: result.path は文字列である必要があります`);
    }
    return;
  }
  throw new ProtocolError(`${context}: result.kind が不正です`);
}

function checkDialogKind(value: unknown, context: string): asserts value is DialogKind {
  if (value !== "open" && value !== "save") {
    throw new ProtocolError(`${context}: kind は "open" か "save" である必要があります`);
  }
}

/** Node → ブリッジの 1 行(末尾に改行を付ける)。形が違えば書かずに ProtocolError を投げる */
export function encodeLine(message: ToBridge): string {
  const value: unknown = message;
  if (!isObject(value)) {
    throw new ProtocolError("送信する行はオブジェクトである必要があります");
  }
  if (!isId(value.id)) {
    throw new ProtocolError("送信する行の id は 0 以上の整数である必要があります");
  }
  if (value.type === "request") {
    onlyKeys(value, ["type", "id", "cmd", "payload"], "request");
    if (typeof value.cmd !== "string") {
      throw new ProtocolError("request: cmd は文字列である必要があります");
    }
    if (!hasKey(value, "payload")) {
      throw new ProtocolError("request: payload が必要です(無い場合は null)");
    }
    // JSON にすると undefined の値は消え、ブリッジは payload の無い行を受け取って応答しないため、書かずに拒否する
    if (value.payload === undefined) {
      throw new ProtocolError("request: payload は undefined ではなく null を指定してください");
    }
    return `${JSON.stringify(value)}\n`;
  }
  if (value.type === "dialogResult") {
    onlyKeys(value, ["type", "id", "result"], "dialogResult");
    checkDialogResult(value.result, "dialogResult");
    return `${JSON.stringify(value)}\n`;
  }
  throw new ProtocolError(`送信する行の type が不正です: ${String(value.type)}`);
}

/** ブリッジ → Node の 1 行(改行を除いた文字列)を解釈する。形が違えば ProtocolError を投げる */
export function parseLine(line: string): FromBridge {
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch {
    throw new ProtocolError("行が JSON として読めません");
  }
  if (!isObject(parsed)) {
    throw new ProtocolError("行はオブジェクトである必要があります");
  }
  if (parsed.type === "response") {
    if (!isId(parsed.id)) {
      throw new ProtocolError("response: id は 0 以上の整数である必要があります");
    }
    if (parsed.ok === true) {
      onlyKeys(parsed, ["type", "id", "ok", "value"], "response");
      if (!hasKey(parsed, "value")) {
        throw new ProtocolError("response: 成功には value が必要です");
      }
      return { type: "response", id: parsed.id, ok: true, value: parsed.value };
    }
    if (parsed.ok === false) {
      onlyKeys(parsed, ["type", "id", "ok", "error"], "response");
      if (!hasKey(parsed, "error")) {
        throw new ProtocolError("response: 失敗には error が必要です");
      }
      return { type: "response", id: parsed.id, ok: false, error: parsed.error };
    }
    throw new ProtocolError("response: ok は真偽値である必要があります");
  }
  if (parsed.type === "dialog") {
    onlyKeys(parsed, ["type", "id", "kind"], "dialog");
    if (!isId(parsed.id)) {
      throw new ProtocolError("dialog: id は 0 以上の整数である必要があります");
    }
    checkDialogKind(parsed.kind, "dialog");
    return { type: "dialog", id: parsed.id, kind: parsed.kind };
  }
  throw new ProtocolError(`行の type が不正です: ${String(parsed.type)}`);
}
