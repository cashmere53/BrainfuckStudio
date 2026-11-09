import { describe, expect, it } from "vitest";
import type { ComparisonInfoDto } from "../../generated/ComparisonInfoDto";
import type { WorkspaceValues } from "../../state/workspace";
import { MAX_INPUT_BYTES, MAX_SOURCE_BYTES } from "../../domain/limits";
import { deriveRequest, type Selection } from "./comparisonRequest";

const info: ComparisonInfoDto = {
  maxSteps: 50_000_000,
  maxRepetitions: 5,
  defaultRepetitions: 3,
  methods: ["naive", "optimized", "wasm"],
};

const bothMethods: Selection = { optimized: true, wasm: true, repetitions: 3 };

function values(overrides: Partial<WorkspaceValues> = {}): WorkspaceValues {
  return {
    source: "+.",
    input: { mode: "text", text: "" },
    limit: { text: "1000", unlimited: false, lastValid: { kind: "limited", maxSteps: 1000 } },
    ...overrides,
  };
}

function limitedWith(text: string, unlimited = false): WorkspaceValues {
  return values({ limit: { text, unlimited, lastValid: { kind: "limited", maxSteps: 1000 } } });
}

function reasonOf(result: ReturnType<typeof deriveRequest>): unknown {
  if (result.ok) {
    throw new Error("expected an invalid reason but the request was accepted");
  }
  return result.reason;
}

describe("deriveRequest: 命令数上限", () => {
  it("上限なしは unlimited で無効になる", () => {
    expect(reasonOf(deriveRequest(limitedWith("1000", true), bothMethods, info))).toEqual({ kind: "unlimited" });
  });

  it("上限なしのとき入力欄が不正でも unlimited を返す(上限なしを優先)", () => {
    expect(reasonOf(deriveRequest(limitedWith("abc", true), bothMethods, info))).toEqual({ kind: "unlimited" });
  });

  it.each(["", "0", "-1", "1.5", "abc", " 1", "+1", "000"])("入力 %j は invalidLimit で無効になる", (text) => {
    expect(reasonOf(deriveRequest(limitedWith(text), bothMethods, info))).toEqual({ kind: "invalidLimit" });
  });

  it("下限 1 は受理し maxSteps に 1 を入れる", () => {
    const result = deriveRequest(limitedWith("1"), bothMethods, info);
    expect(result.ok && result.request.maxSteps).toBe(1);
  });

  it("最大値 50000000 は受理する", () => {
    const result = deriveRequest(limitedWith("50000000"), bothMethods, info);
    expect(result.ok && result.request.maxSteps).toBe(50_000_000);
  });

  it("最大 + 1 の 50000001 は limitTooLarge(max = 50000000)", () => {
    expect(reasonOf(deriveRequest(limitedWith("50000001"), bothMethods, info))).toEqual({
      kind: "limitTooLarge",
      max: 50_000_000,
    });
  });

  it("2^53 を含む桁数の大きい値も精度を失わず limitTooLarge になる", () => {
    expect(reasonOf(deriveRequest(limitedWith("99999999999999999999"), bothMethods, info))).toEqual({
      kind: "limitTooLarge",
      max: 50_000_000,
    });
  });
});

describe("deriveRequest: 入力", () => {
  it("空のテキスト入力は空のバイト列として Base64 は空文字列になる", () => {
    const result = deriveRequest(values(), bothMethods, info);
    expect(result.ok && result.request.inputBase64).toBe("");
  });

  it("テキスト入力 Hi は Base64 SGk= になる", () => {
    const result = deriveRequest(values({ input: { mode: "text", text: "Hi" } }), bothMethods, info);
    expect(result.ok && result.request.inputBase64).toBe("SGk=");
  });

  it("テキスト入力は UTF-8 のバイト列で Base64 化する(あ = E3 81 82 → 44GC)", () => {
    const result = deriveRequest(values({ input: { mode: "text", text: "あ" } }), bothMethods, info);
    expect(result.ok && result.request.inputBase64).toBe("44GC");
  });

  it("16 進入力 48 69 は Base64 SGk= になる", () => {
    const result = deriveRequest(values({ input: { mode: "hex", text: "48 69" } }), bothMethods, info);
    expect(result.ok && result.request.inputBase64).toBe("SGk=");
  });

  it("16 進の桁数が奇数なら invalidInput", () => {
    expect(reasonOf(deriveRequest(values({ input: { mode: "hex", text: "4" } }), bothMethods, info))).toEqual({
      kind: "invalidInput",
    });
  });

  it("16 進に不正な文字があれば invalidInput", () => {
    expect(reasonOf(deriveRequest(values({ input: { mode: "hex", text: "zz" } }), bothMethods, info))).toEqual({
      kind: "invalidInput",
    });
  });

  it("テキスト入力が 1 MiB ちょうどは受理する", () => {
    const text = "a".repeat(MAX_INPUT_BYTES);
    expect(deriveRequest(values({ input: { mode: "text", text } }), bothMethods, info).ok).toBe(true);
  });

  it("テキスト入力が 1 MiB + 1 バイトは inputTooLarge", () => {
    const text = "a".repeat(MAX_INPUT_BYTES + 1);
    expect(reasonOf(deriveRequest(values({ input: { mode: "text", text } }), bothMethods, info))).toEqual({
      kind: "inputTooLarge",
    });
  });

  it("16 進入力が 1 MiB + 1 バイトは inputTooLarge", () => {
    const text = "00".repeat(MAX_INPUT_BYTES + 1);
    expect(reasonOf(deriveRequest(values({ input: { mode: "hex", text } }), bothMethods, info))).toEqual({
      kind: "inputTooLarge",
    });
  });

  it("上限の不正は入力の不正より先に判定する", () => {
    expect(reasonOf(deriveRequest(
      values({ input: { mode: "hex", text: "zz" }, limit: { text: "0", unlimited: false, lastValid: { kind: "limited", maxSteps: 1000 } } }),
      bothMethods,
      info,
    ))).toEqual({ kind: "invalidLimit" });
  });
});

describe("deriveRequest: ソース", () => {
  it("ソースは内容をそのまま要求に入れる(改行を含む)", () => {
    const result = deriveRequest(values({ source: "+\n." }), bothMethods, info);
    expect(result.ok && result.request.source).toBe("+\n.");
  });

  it("ソースが 1 MiB ちょうどは受理する", () => {
    const source = "+".repeat(MAX_SOURCE_BYTES);
    expect(deriveRequest(values({ source }), bothMethods, info).ok).toBe(true);
  });

  it("ソースが 1 MiB + 1 バイトは sourceTooLarge", () => {
    const source = "+".repeat(MAX_SOURCE_BYTES + 1);
    expect(reasonOf(deriveRequest(values({ source }), bothMethods, info))).toEqual({ kind: "sourceTooLarge" });
  });

  it("ソースのサイズは UTF-8 のバイト数で判定する(文字数は上限以下でも超過)", () => {
    // あ は 3 バイト。文字数は上限より少ないがバイト数は上限を超える
    const source = "あ".repeat(Math.floor(MAX_SOURCE_BYTES / 3) + 1);
    expect(source.length).toBeLessThan(MAX_SOURCE_BYTES);
    expect(reasonOf(deriveRequest(values({ source }), bothMethods, info))).toEqual({ kind: "sourceTooLarge" });
  });
});

describe("deriveRequest: 方式と繰り返し", () => {
  it("naive は常に含めず、選択された最適化・WASM を順に並べる(両方)", () => {
    const result = deriveRequest(values(), bothMethods, info);
    expect(result.ok && result.request.methods).toEqual(["optimized", "wasm"]);
  });

  it("最適化だけを選ぶと methods は optimized のみ", () => {
    const result = deriveRequest(values(), { optimized: true, wasm: false, repetitions: 1 }, info);
    expect(result.ok && result.request.methods).toEqual(["optimized"]);
  });

  it("WASM だけを選ぶと methods は wasm のみ", () => {
    const result = deriveRequest(values(), { optimized: false, wasm: true, repetitions: 1 }, info);
    expect(result.ok && result.request.methods).toEqual(["wasm"]);
  });

  it("何も選ばなければ methods は空(素朴だけの比較)", () => {
    const result = deriveRequest(values(), { optimized: false, wasm: false, repetitions: 1 }, info);
    expect(result.ok && result.request.methods).toEqual([]);
  });

  it.each([1, 5])("繰り返し %i は受理し repetitions に入れる", (repetitions) => {
    const result = deriveRequest(values(), { optimized: false, wasm: false, repetitions }, info);
    expect(result.ok && result.request.repetitions).toBe(repetitions);
  });

  it.each([0, 6, 2.5, -1, Number.NaN])("繰り返し %p は invalidRepetitions(max = 5)", (repetitions) => {
    expect(reasonOf(deriveRequest(values(), { optimized: false, wasm: false, repetitions }, info))).toEqual({
      kind: "invalidRepetitions",
      max: 5,
    });
  });

  it("要求の maxSteps は上限の数値を入れる", () => {
    const result = deriveRequest(limitedWith("123"), bothMethods, info);
    expect(result.ok && result.request.maxSteps).toBe(123);
  });
});
