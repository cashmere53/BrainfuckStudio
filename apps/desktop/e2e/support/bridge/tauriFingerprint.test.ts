// @vitest-environment node
// ブリッジ(crates/brainfuck-e2e-bridge)が複製している src-tauri の区間の指紋を固定する。
// 区間の本文を空白を正規化して SHA-256 にし、テストに固定した値と比べる。違えば、変わった区間・複製先・手順を示して失敗する。
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const srcTauri = resolve(import.meta.dirname, "../../../src-tauri");

interface FingerprintTarget {
  /** 失敗の文面に出す区間の名前。 */
  name: string;
  /** src-tauri からの相対パス。 */
  file: string;
  /** 区間の宣言の行(行頭からの正規表現)。複数あれば区間を順に連結する。 */
  headers: readonly RegExp[];
  /** 固定の指紋(空白を正規化した本文の SHA-256)。 */
  sha256: string;
}

// 区間は宣言の行から、対応する閉じ波括弧までとする。
const TARGETS: readonly FingerprintTarget[] = [
  {
    name: "DocumentState(構造体・Default・impl)",
    file: "src/app_state.rs",
    headers: [
      /^\s*pub struct DocumentState\b/,
      /^\s*impl Default for DocumentState\b/,
      /^\s*impl DocumentState\b/,
    ],
    sha256: "30af922bcfec62df474ae1039f1fe015f792086958e6732d5d65ef343074102d",
  },
  {
    name: "io_failure",
    file: "src/commands/files.rs",
    headers: [/^\s*pub fn io_failure\b/],
    sha256: "9b173a8e4b10edfb5e630c020e3d4ce47e9ce9dd8df5a47e03c95791862e0ed8",
  },
  {
    name: "save_needs_dialog",
    file: "src/commands/files.rs",
    headers: [/^\s*pub fn save_needs_dialog\b/],
    sha256: "c12ba9b48f982f47f8201e17c4ed51d4cfa92b5cf46bcea60fa092532414ebcb",
  },
  {
    name: "read_source",
    file: "src/commands/files.rs",
    headers: [/^\s*pub fn read_source\b/],
    sha256: "ccfed7c9f1c1a4d64b3eebb08b6e800d7eff8317680bf3d1abf6f550ccd98bf1",
  },
  {
    name: "write_source",
    file: "src/commands/files.rs",
    headers: [/^\s*pub fn write_source\b/],
    sha256: "5a93b3aec4d443b9f7fbe83081c89ca6ff1d621cca3391fbbd2f4a111ce58614",
  },
  {
    name: "write_bytes",
    file: "src/commands/files.rs",
    headers: [/^\s*fn write_bytes\b/],
    sha256: "3543552b31646eb9b1130ac4f1a5010cbd7ab8d55585e87a5ba5cc6d8261f409",
  },
  {
    name: "to_path",
    file: "src/commands/files.rs",
    headers: [/^\s*fn to_path\b/],
    sha256: "b9e2096fde45816a849ba96ef847c22a08ed3f507de6e17658a50afa29d792ea",
  },
  {
    name: "new_document",
    file: "src/commands/files.rs",
    headers: [/^\s*pub async fn new_document\b/],
    sha256: "b6d3bcf5a687141a2256da27d10132aaeb4fe2076327ee79c393b17c09e462f0",
  },
  {
    name: "open_source_file",
    file: "src/commands/files.rs",
    headers: [/^\s*pub async fn open_source_file\b/],
    sha256: "b4b196ec4b04358298c503e80f9edbfcee48703677d0342fa04d9c9ec8e833e6",
  },
  {
    name: "save_source_file",
    file: "src/commands/files.rs",
    headers: [/^\s*pub async fn save_source_file\b/],
    sha256: "aae22f02530f4e1000e7b1bb6f3fed9c7f8f40acaceb861ec3f07009d4432f74",
  },
  {
    name: "set_document_dirty",
    file: "src/commands/files.rs",
    headers: [/^\s*pub async fn set_document_dirty\b/],
    sha256: "6de3d0e1ee173455b3b3944315175fa7bfb6419b46e154f94448d82f4d3563ab",
  },
  {
    name: "close_requires_confirmation",
    file: "src/window.rs",
    headers: [/^\s*pub fn close_requires_confirmation\b/],
    sha256: "a264601486045ca33209e709eaf0dd479613139b69f60869ef572451662852b3",
  },
];

/** 空白を正規化する(連続する空白・改行を 1 つの空白にし、前後を取り除く)。 */
function normalize(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/** 空白を正規化した本文の SHA-256(16 進)。 */
function sha256Of(text: string): string {
  return createHash("sha256").update(normalize(text), "utf8").digest("hex");
}

/**
 * 宣言の行から、対応する閉じ波括弧の行までを取り出す。
 * 行コメント(`//` 以降)の中の波括弧は数えない。宣言が無い、または閉じが無い場合は失敗にする。
 *
 * 制約: 文字列リテラル・char リテラル・ブロックコメント(`/* ... *\/`)の中の波括弧は扱わない。
 * `//` より後ろはすべて行コメントとみなすので、文字列中の `//`(例: "http://...")より後ろも数えない。
 * こうした箇所に波括弧があると、取り出す範囲が本来とずれる。その場合は範囲が変わって指紋が一致せず、
 * 誤検出で失敗する側に倒れる(範囲のずれを見逃して合格することはない)。区間を書き換えたら固定値を確かめること。
 */
function extractSection(source: string, header: RegExp): string {
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex((line) => header.test(line));
  if (start < 0) {
    throw new Error(`区間の宣言が見つからない: ${header.source}`);
  }
  let depth = 0;
  let opened = false;
  for (let i = start; i < lines.length; i++) {
    const code = (lines[i] ?? "").replace(/\/\/.*$/, "");
    for (const ch of code) {
      if (ch === "{") {
        depth++;
        opened = true;
      } else if (ch === "}") {
        depth--;
      }
    }
    if (opened && depth === 0) {
      return lines.slice(start, i + 1).join("\n");
    }
  }
  throw new Error(`区間の閉じ波括弧が見つからない: ${header.source}`);
}

/** 目標の区間を連結した本文。 */
function sectionText(source: string, headers: readonly RegExp[]): string {
  return headers.map((header) => extractSection(source, header)).join("\n");
}

/** 指紋が固定値と一致することを確かめ、違えば変わった区間・複製先・手順を示して失敗する。 */
function checkFingerprint(target: FingerprintTarget, actual: string): void {
  if (actual === target.sha256) {
    return;
  }
  throw new Error(
    [
      `Tauri 層の区間「${target.name}」(${target.file})が変わり、指紋が固定値と一致しない。`,
      `  固定値: ${target.sha256}`,
      `  現在値: ${actual}`,
      "この区間はブリッジが複製している(複製先: crates/brainfuck-e2e-bridge/src/document.rs / crates/brainfuck-e2e-bridge/src/dispatch.rs)。次の順で対応する:",
      "  1. 複製先を、変更後の Tauri 層の振る舞いに合わせる(ブリッジで対応する、または差を理由つきで残す)",
      "  2. ブリッジの単体テストを更新し、cargo test -p brainfuck-e2e-bridge が通ることを確認する",
      "  3. apps/desktop/e2e/support/bridge/tauriFingerprint.test.ts の固定値を、上の現在値に置き換える",
    ].join("\n"),
  );
}

describe("区間の取り出し", () => {
  it("入れ子の波括弧を含む関数の、宣言の行から対応する閉じ波括弧までを取り出す", () => {
    const source = [
      "fn before() {}",
      "pub fn target() {",
      "    if true {",
      "        inner();",
      "    }",
      "}",
      "fn after() {}",
    ].join("\n");
    expect(extractSection(source, /^\s*pub fn target\b/)).toBe(
      ["pub fn target() {", "    if true {", "        inner();", "    }", "}"].join("\n"),
    );
  });

  it("行コメントの中の波括弧は数えない", () => {
    const source = ["fn f() {", "    // } は数えない", "}", "fn g() {}"].join("\n");
    expect(extractSection(source, /^\s*fn f\b/)).toBe(["fn f() {", "    // } は数えない", "}"].join("\n"));
  });

  it("宣言が無いときは区間の名前を示して失敗する", () => {
    expect(() => extractSection("fn a() {}", /^\s*fn missing\b/)).toThrow("区間の宣言が見つからない");
  });

  it("閉じ波括弧が無いときは失敗する", () => {
    expect(() => extractSection("fn a() {\n    let x = 1;\n", /^\s*fn a\b/)).toThrow(
      "区間の閉じ波括弧が見つからない",
    );
  });
});

describe("指紋の比較", () => {
  it("空白の違いは指紋に影響せず、1 文字の違いは指紋を変える", () => {
    expect(sha256Of("fn a() {\n  x\n}")).toBe(sha256Of("fn a() { x }"));
    expect(sha256Of("fn a() { x }")).not.toBe(sha256Of("fn a() { y }"));
  });

  it("区間を 1 文字変えた本文を渡すと、区間の名前・現在値・複製先・手順を示して失敗する", () => {
    const target = TARGETS.find((t) => t.name === "close_requires_confirmation");
    expect(target).toBeDefined();
    if (!target) return;
    const source = readFileSync(resolve(srcTauri, target.file), "utf8");
    const body = sectionText(source, target.headers);
    const altered = body.replace("guard.dirty", "guard.dirtx");
    expect(altered).not.toBe(body);
    const actual = sha256Of(altered);

    const fixed = sha256Of(body);
    let message = "";
    try {
      checkFingerprint({ ...target, sha256: fixed }, actual);
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
    expect(message).toContain("close_requires_confirmation");
    expect(message).toContain(target.file);
    // 固定値の行・現在値の行
    expect(message).toContain(`固定値: ${fixed}`);
    expect(message).toContain(`現在値: ${actual}`);
    // 複製先
    expect(message).toContain("crates/brainfuck-e2e-bridge/src/document.rs");
    expect(message).toContain("crates/brainfuck-e2e-bridge/src/dispatch.rs");
    // 手順 1(複製先を合わせる)・2・3
    expect(message).toContain("1. 複製先を、変更後の Tauri 層の振る舞いに合わせる");
    expect(message).toContain("cargo test -p brainfuck-e2e-bridge");
    expect(message).toContain("tauriFingerprint.test.ts");
  });

  it("固定値と先頭 63 桁だけが同じで最後の 1 桁が違う値は、一致しないとして失敗する", () => {
    const target = TARGETS.find((t) => t.name === "close_requires_confirmation");
    expect(target).toBeDefined();
    if (!target) return;
    const fixed = `${"a".repeat(63)}0`;
    const near = `${"a".repeat(63)}1`;
    expect(() => checkFingerprint({ ...target, sha256: fixed }, near)).toThrow("指紋が固定値と一致しない");
    expect(() => checkFingerprint({ ...target, sha256: fixed }, fixed)).not.toThrow();
  });
});

describe("区間の一覧", () => {
  it("12 区間を design の順序どおりに持つ", () => {
    expect(TARGETS.map((t) => t.name)).toEqual([
      "DocumentState(構造体・Default・impl)",
      "io_failure",
      "save_needs_dialog",
      "read_source",
      "write_source",
      "write_bytes",
      "to_path",
      "new_document",
      "open_source_file",
      "save_source_file",
      "set_document_dirty",
      "close_requires_confirmation",
    ]);
  });

  it("すべての固定値は 64 桁の 16 進数である", () => {
    for (const target of TARGETS) {
      expect(target.sha256, target.name).toMatch(/^[0-9a-f]{64}$/);
    }
  });

  it("DocumentState の宣言の行は 3 つ(構造体・Default・impl)である", () => {
    const target = TARGETS.find((t) => t.name.startsWith("DocumentState"));
    expect(target?.headers).toHaveLength(3);
  });
});

describe("複製元の区間の指紋", () => {
  for (const target of TARGETS) {
    it(`${target.name}(${target.file}) の指紋が固定値と一致する`, () => {
      const source = readFileSync(resolve(srcTauri, target.file), "utf8");
      checkFingerprint(target, sha256Of(sectionText(source, target.headers)));
    });
  }
});
