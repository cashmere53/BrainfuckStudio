// @vitest-environment node
// ブリッジの対応表(dispatch.rs の COMMANDS)と Tauri の登録の基準(names.rs の COMMAND_NAMES)が一致することを固定する。
// どちらも Rust のソースを読む方式(apps/desktop/src/ipc/contract.test.ts と同じ)。
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const repoRoot = resolve(import.meta.dirname, "../../../../..");
const namesPath = resolve(repoRoot, "apps/desktop/src-tauri/src/commands/names.rs");
const dispatchPath = resolve(repoRoot, "crates/brainfuck-e2e-bridge/src/dispatch.rs");

type ContractInput = {
  /** Tauri の登録の基準(names.rs の COMMAND_NAMES) */
  readonly registered: readonly string[];
  /** ブリッジの対応表(dispatch.rs の COMMANDS) */
  readonly bridged: readonly string[];
  /** ブリッジが扱わないコマンドの除外一覧(理由つき) */
  readonly excluded: readonly { readonly name: string; readonly reason: string }[];
};

/**
 * ブリッジが扱わないコマンドの除外一覧。除外するときは理由を書いて入れる。
 * 比較機能はブラウザ E2E の対象外。ブリッジに brainfuck-compare-api と wasmi の依存を持ち込まないため、比較の 2 件は除外する。
 */
const EXCLUDED_COMMANDS: ContractInput["excluded"] = [
  {
    name: "get_comparison_info",
    reason: "比較画面はブラウザ E2E の対象外。ブリッジに brainfuck-compare-api と wasmi の依存を持ち込まない",
  },
  {
    name: "run_comparison",
    reason: "比較画面はブラウザ E2E の対象外。ブリッジに brainfuck-compare-api と wasmi の依存を持ち込まない",
  },
];

const REMEDY_REGISTERED =
  "ブリッジで対応する(dispatch.rs の COMMANDS と dispatch の分岐に足す)、または理由つきで EXCLUDED_COMMANDS に入れる";

/** 差の一行。名前・問題・対処の順で、対処を必ず付ける。 */
function difference(name: string, problem: string, remedy: string): string {
  return `${name}: ${problem}。対処: ${remedy}`;
}

/**
 * 登録・ブリッジ・除外の三つを比べ、差の文面を返す。差が無ければ空配列。
 * 失敗の文面には差のあるコマンド名と、対処(ブリッジで対応する、または理由つきで EXCLUDED_COMMANDS に入れる)を示す。
 */
function findContractDifferences(input: ContractInput): string[] {
  const registered = new Set(input.registered);
  const bridged = new Set(input.bridged);
  const excludedNames = input.excluded.map((entry) => entry.name);
  const excluded = new Set(excludedNames);
  const differences: string[] = [];

  for (const name of duplicates(input.bridged)) {
    differences.push(difference(name, "COMMANDS に重複している", "1 件にする"));
  }
  for (const name of duplicates(excludedNames)) {
    differences.push(difference(name, "EXCLUDED_COMMANDS に重複している", "1 件にする"));
  }
  for (const name of bridged) {
    if (excluded.has(name)) {
      differences.push(
        difference(
          name,
          "ブリッジで対応するのに EXCLUDED_COMMANDS にも入っている",
          "EXCLUDED_COMMANDS から外す",
        ),
      );
    }
  }
  for (const name of excluded) {
    if (!registered.has(name)) {
      differences.push(
        difference(name, "EXCLUDED_COMMANDS にあるが COMMAND_NAMES に無い", "EXCLUDED_COMMANDS から消す"),
      );
    }
  }
  for (const name of registered) {
    if (!bridged.has(name) && !excluded.has(name)) {
      differences.push(
        difference(name, "COMMAND_NAMES にあるがブリッジにも除外一覧にも無い", REMEDY_REGISTERED),
      );
    }
  }
  for (const name of bridged) {
    if (!registered.has(name)) {
      differences.push(
        difference(
          name,
          "ブリッジの COMMANDS にあるが COMMAND_NAMES に無い",
          "names.rs の COMMAND_NAMES に足す、またはブリッジの COMMANDS から外す",
        ),
      );
    }
  }
  return differences;
}

function duplicates(names: readonly string[]): string[] {
  const seen = new Set<string>();
  const repeated = new Set<string>();
  for (const name of names) {
    if (seen.has(name)) {
      repeated.add(name);
    }
    seen.add(name);
  }
  return [...repeated];
}

/**
 * Rust のソースから、行コメントと、複数行にまたがるブロックコメントを除く。
 * 文字列リテラルと文字リテラルの中身は保つ。
 */
function stripComments(source: string): string {
  let out = "";
  let i = 0;
  while (i < source.length) {
    const ch = source[i];
    const next = source[i + 1];
    if (ch === '"') {
      const end = stringLiteralEnd(source, i);
      out += source.slice(i, end);
      i = end;
    } else if (ch === "'") {
      const charLiteral = /^'(?:\\.|[^'\\\n])'/.exec(source.slice(i, i + 8));
      if (charLiteral) {
        out += charLiteral[0];
        i += charLiteral[0].length;
      } else {
        out += ch;
        i += 1;
      }
    } else if (ch === "/" && next === "/") {
      const newline = source.indexOf("\n", i);
      i = newline === -1 ? source.length : newline;
    } else if (ch === "/" && next === "*") {
      const close = source.indexOf("*/", i + 2);
      i = close === -1 ? source.length : close + 2;
      out += " ";
    } else {
      out += ch;
      i += 1;
    }
  }
  return out;
}

/** 開き引用符の位置から、対応する閉じ引用符の次の位置を返す(エスケープを考慮する)。 */
function stringLiteralEnd(source: string, open: number): number {
  let i = open + 1;
  while (i < source.length) {
    if (source[i] === "\\") {
      i += 2;
    } else if (source[i] === '"') {
      return i + 1;
    } else {
      i += 1;
    }
  }
  return source.length;
}

/**
 * Rust のソースから `constName: &[&str] = &[ ... ];` の要素(コマンド名)を順に取り出す。
 * コメントを先に除くので、コメントの中の名前や `];` は数えない。定数名は単語の途中に一致しない(E2E_COMMANDS と COMMANDS を区別する)。
 */
function parseCommandList(source: string, constName: string): string[] {
  const code = stripComments(source);
  const pattern = new RegExp(
    `(?<![A-Za-z0-9_])${constName}\\s*:\\s*&\\[&str\\]\\s*=\\s*&\\[([\\s\\S]*?)\\];`,
  );
  const block = pattern.exec(code)?.[1];
  if (block === undefined) {
    throw new Error(`${constName} が見つからない`);
  }
  return [...block.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1] ?? "");
}

function readCommandList(file: string, constName: string): string[] {
  return parseCommandList(readFileSync(file, "utf8"), constName);
}

function realInput(): ContractInput {
  return {
    registered: readCommandList(namesPath, "COMMAND_NAMES"),
    bridged: readCommandList(dispatchPath, "COMMANDS"),
    excluded: EXCLUDED_COMMANDS,
  };
}

describe("ブリッジのコマンド一覧の契約", () => {
  it("dispatch.rs の COMMANDS と除外一覧の和が names.rs の COMMAND_NAMES に等しい", () => {
    const input = realInput();
    expect(input.registered.length).toBeGreaterThan(0);
    expect(input.bridged.length).toBeGreaterThan(0);
    expect(findContractDifferences(input)).toEqual([]);
  });

  it("有効な除外(登録にあり、ブリッジに無く、除外一覧にある)は差にしない", () => {
    expect(
      findContractDifferences({
        registered: ["a", "b"],
        bridged: ["a"],
        excluded: [{ name: "b", reason: "ブリッジの対象外" }],
      }),
    ).toEqual([]);
  });

  it("COMMAND_NAMES にだけコマンドを足した入力は失敗し、足した名前と対処(ブリッジで対応する・理由つきで EXCLUDED_COMMANDS)を示す", () => {
    const input = realInput();
    const differences = findContractDifferences({
      ...input,
      registered: [...input.registered, "new_only_command"],
    });
    expect(differences).toEqual([
      `new_only_command: COMMAND_NAMES にあるがブリッジにも除外一覧にも無い。対処: ${REMEDY_REGISTERED}`,
    ]);
    expect(differences[0]).toContain("ブリッジで対応する");
    expect(differences[0]).toContain("理由つきで");
    expect(differences[0]).toContain("EXCLUDED_COMMANDS");
  });

  it("ブリッジにだけコマンドを足した入力は失敗し、足した名前と対処(COMMAND_NAMES に足す)を示す", () => {
    const input = realInput();
    const differences = findContractDifferences({
      ...input,
      bridged: [...input.bridged, "bridge_only_command"],
    });
    expect(differences).toEqual([
      "bridge_only_command: ブリッジの COMMANDS にあるが COMMAND_NAMES に無い。対処: names.rs の COMMAND_NAMES に足す、またはブリッジの COMMANDS から外す",
    ]);
  });

  it("除外一覧の名前がブリッジと重なる入力は失敗し、外す対処を示す", () => {
    const input = realInput();
    const differences = findContractDifferences({
      ...input,
      excluded: [...input.excluded, { name: "load_source", reason: "古い除外" }],
    });
    expect(differences).toEqual([
      "load_source: ブリッジで対応するのに EXCLUDED_COMMANDS にも入っている。対処: EXCLUDED_COMMANDS から外す",
    ]);
  });

  it("登録に無い名前を除外している入力は失敗し、消す対処を示す", () => {
    const input = realInput();
    const differences = findContractDifferences({
      ...input,
      excluded: [...input.excluded, { name: "removed_command", reason: "古い除外" }],
    });
    expect(differences).toEqual([
      "removed_command: EXCLUDED_COMMANDS にあるが COMMAND_NAMES に無い。対処: EXCLUDED_COMMANDS から消す",
    ]);
  });

  it("COMMANDS に同じ名前が重複する入力は失敗する", () => {
    const input = realInput();
    const first = input.bridged[0] ?? "";
    const differences = findContractDifferences({
      ...input,
      bridged: [...input.bridged, first],
    });
    expect(differences).toEqual([`${first}: COMMANDS に重複している。対処: 1 件にする`]);
  });

  it("除外一覧に同じ名前が重複する入力は失敗する", () => {
    const differences = findContractDifferences({
      registered: ["a", "b"],
      bridged: ["a"],
      excluded: [
        { name: "b", reason: "理由 1" },
        { name: "b", reason: "理由 2" },
      ],
    });
    expect(differences).toEqual(["b: EXCLUDED_COMMANDS に重複している。対処: 1 件にする"]);
  });
});

describe("コマンド名の一覧の読み取り", () => {
  it("一行の書式と複数行の書式で同じ名前の一覧になる", () => {
    const oneLine = `pub const COMMANDS: &[&str] = &["a", "b"];`;
    const multiLine = `pub const COMMANDS: &[&str] = &[\n    "a",\n    "b",\n];`;
    expect(parseCommandList(oneLine, "COMMANDS")).toEqual(["a", "b"]);
    expect(parseCommandList(multiLine, "COMMANDS")).toEqual(["a", "b"]);
  });

  it("行コメントの中の名前は数えない", () => {
    const source = `pub const COMMANDS: &[&str] = &[\n    "a",\n    // "step",\n    "b",\n];`;
    expect(parseCommandList(source, "COMMANDS")).toEqual(["a", "b"]);
  });

  it("ブロックコメントの中の名前は数えない", () => {
    const source = `pub const COMMANDS: &[&str] = &[\n    "a", /* "ghost" */\n    "b",\n];`;
    expect(parseCommandList(source, "COMMANDS")).toEqual(["a", "b"]);
  });

  it("コメントの中の `];` で一覧が切れない", () => {
    const source = `pub const COMMANDS: &[&str] = &[\n    "a", // ];\n    "b",\n];`;
    expect(parseCommandList(source, "COMMANDS")).toEqual(["a", "b"]);
  });

  it("E2E_COMMANDS が前にあっても COMMANDS を取る", () => {
    const source = `pub const E2E_COMMANDS: &[&str] = &["x"];\npub const COMMANDS: &[&str] = &["a"];`;
    expect(parseCommandList(source, "COMMANDS")).toEqual(["a"]);
  });

  it("文字列の中の `//` はコメントにしない", () => {
    const source = `pub const COMMANDS: &[&str] = &["a//b", "c"];`;
    expect(parseCommandList(source, "COMMANDS")).toEqual(["a//b", "c"]);
  });

  it("文字リテラルの引用符で文字列の扱いを崩さない", () => {
    const source = `const Q: char = '"';\npub const COMMANDS: &[&str] = &["a", "b"];`;
    expect(parseCommandList(source, "COMMANDS")).toEqual(["a", "b"]);
  });

  it("定数が無いときは例外にする", () => {
    expect(() => parseCommandList(`pub const OTHER: &[&str] = &["a"];`, "COMMANDS")).toThrow(
      /COMMANDS が見つからない/,
    );
  });
});
