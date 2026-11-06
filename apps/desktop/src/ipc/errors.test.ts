import { describe, expect, it } from "vitest";
import type { ApiError } from "../generated/ApiError";
import type { ApiErrorKind } from "../generated/ApiErrorKind";
import {
  API_ERROR_KINDS,
  apiErrorMessageJa,
  isApiError,
  isSessionInvalidating,
  normalizeApiError,
} from "./errors";

const JAPANESE = /[\u3040-\u30ff\u4e00-\u9fff]/;

describe("isApiError: 拒否値の判別", () => {
  it("kind・message・field を持つオブジェクトを ApiError として判別する", () => {
    expect(isApiError({ kind: "io", message: "failed to read file: denied", field: null })).toBe(true);
    expect(isApiError({ kind: "invalidArgument", message: "x", field: "batchSize" })).toBe(true);
  });

  it("未知の kind・欠けた項目・プリミティブは ApiError ではない", () => {
    expect(isApiError(null)).toBe(false);
    expect(isApiError("boom")).toBe(false);
    expect(isApiError(42)).toBe(false);
    expect(isApiError({ kind: "bogus", message: "x", field: null })).toBe(false);
    expect(isApiError({ kind: "internal", field: null })).toBe(false);
    expect(isApiError({ kind: "internal", message: "x" })).toBe(false);
  });
});

describe("normalizeApiError: 未知の拒否値は internal に正規化する", () => {
  it("ApiError はそのままの内容で返す", () => {
    const err: ApiError = { kind: "sessionNotFound", message: "session not found", field: null };
    expect(normalizeApiError(err)).toEqual(err);
  });

  it("文字列・Error・未知の kind は internal にし、元の値の内容を含めない", () => {
    const fromString = normalizeApiError("secret C:\\Users\\me\\a.bf");
    expect(fromString.kind).toBe("internal");
    expect(fromString.field).toBeNull();
    expect(fromString.message).not.toContain("secret");

    const fromError = normalizeApiError(new Error("/home/me/source"));
    expect(fromError.kind).toBe("internal");
    expect(fromError.message).not.toContain("/home/me");

    const unknownKind = normalizeApiError({ kind: "bogus", message: "leak", field: null });
    expect(unknownKind.kind).toBe("internal");
    expect(unknownKind.message).not.toContain("leak");
  });
});

describe("apiErrorMessageJa: 種類とフィールドから日本語の通知文を組み立てる", () => {
  it("全種類について日本語の文面を返し、英語の固定文面をそのまま表示しない", () => {
    for (const kind of API_ERROR_KINDS) {
      const text = apiErrorMessageJa({ kind, message: "English fixed text", field: null });
      expect(text, kind).toMatch(JAPANESE);
      expect(text, kind).not.toContain("English fixed text");
    }
  });

  it("種類ごとの文面が互いに異なる(io を除く)", () => {
    const texts = API_ERROR_KINDS.filter((k) => k !== "io").map((kind) =>
      apiErrorMessageJa({ kind, message: "", field: null }),
    );
    expect(new Set(texts).size).toBe(texts.length);
  });

  it("サイズ超過は上限の大きさを示す", () => {
    expect(apiErrorMessageJa({ kind: "sourceTooLarge", message: "", field: null })).toContain("1 MiB");
    expect(apiErrorMessageJa({ kind: "inputTooLarge", message: "", field: null })).toContain("1 MiB");
    expect(apiErrorMessageJa({ kind: "fileTooLarge", message: "", field: null })).toContain("1 MiB");
  });

  it("invalidArgument は不正だった項目名を示す(項目が無ければ示さない)", () => {
    expect(apiErrorMessageJa({ kind: "invalidArgument", message: "", field: "batchSize" })).toContain(
      "batchSize",
    );
    expect(apiErrorMessageJa({ kind: "invalidArgument", message: "", field: null })).not.toContain("項目");
  });

  it("io は読み込み/書き込みの別と OS の原因文を示す", () => {
    const read = apiErrorMessageJa({
      kind: "io",
      message: "failed to read file: Access is denied. (os error 5)",
      field: null,
    });
    expect(read).toContain("読み込み");
    expect(read).toContain("Access is denied. (os error 5)");

    const write = apiErrorMessageJa({
      kind: "io",
      message: "failed to write file: The disk is full.",
      field: null,
    });
    expect(write).toContain("書き込み");
    expect(write).toContain("The disk is full.");
  });

  it("io の文面が想定の形でなければ原因文を出さず一般的な文面にする", () => {
    const text = apiErrorMessageJa({ kind: "io", message: "C:\\secret\\path", field: null });
    expect(text).toMatch(JAPANESE);
    expect(text).not.toContain("C:\\secret");
  });
});

describe("isSessionInvalidating: セッションの無効化を要する種類", () => {
  it("sessionNotFound と staleProgram だけが真", () => {
    const all: ApiErrorKind[] = [...API_ERROR_KINDS];
    const invalidating = all.filter((kind) =>
      isSessionInvalidating({ kind, message: "", field: null }),
    );
    expect(invalidating.sort()).toEqual(["sessionNotFound", "staleProgram"]);
  });
});
