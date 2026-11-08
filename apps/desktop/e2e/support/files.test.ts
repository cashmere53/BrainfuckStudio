// @vitest-environment node
import { appendFileSync, mkdtempSync, existsSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  missingDirectoryPath,
  writeCrlfSource,
  writeOversizedSource,
  writeReadOnlyFile,
  writeShiftJisSource,
} from "./files.ts";

// 書き込み可否の判定(accessSync)だけを差し替えて、属性が効かない環境(書けてしまう環境)を模擬する
const forceWritable = vi.hoisted(() => ({ on: false }));
vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs")>();
  return {
    ...actual,
    accessSync: (p: string, m?: number) => {
      if (forceWritable.on) return;
      return actual.accessSync(p, m);
    },
  };
});

const ONE_MIB = 1024 * 1024;

let dir: string;

beforeEach(() => {
  dir = mkdtempSync(path.join(tmpdir(), "e2e-files-test-"));
});

afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

describe("writeCrlfSource(改行コードが CRLF のソース)", () => {
  it("CRLF の改行だけを含むバイト列を書き、そのパスを返す", () => {
    const file = writeCrlfSource(dir);
    expect(path.dirname(file)).toBe(dir);
    expect(readFileSync(file)).toEqual(Buffer.from("+\r\n+\r\n.", "latin1"));
    expect(statSync(file).size).toBe(7);
  });
});

describe("writeShiftJisSource(UTF-8 として不正な Shift_JIS のソース)", () => {
  it("「テスト」の Shift_JIS のバイト列と命令を書く", () => {
    const file = writeShiftJisSource(dir);
    const bytes = readFileSync(file);
    expect(bytes.subarray(0, 6)).toEqual(Buffer.from([0x83, 0x65, 0x83, 0x58, 0x83, 0x67]));
    expect(bytes.subarray(6)).toEqual(Buffer.from("\n+.", "latin1"));
    expect(bytes.length).toBe(9);
  });

  it("UTF-8 として復号すると置換文字が現れる(不正なバイト列であること)", () => {
    const file = writeShiftJisSource(dir);
    const text = new TextDecoder("utf-8").decode(readFileSync(file));
    expect(text).toContain("�");
  });
});

describe("writeOversizedSource(1 MiB を 1 バイト超えるファイル)", () => {
  it("ちょうど 1 MiB + 1 バイトの大きさで書く", () => {
    const file = writeOversizedSource(dir);
    expect(statSync(file).size).toBe(ONE_MIB + 1);
    expect(readFileSync(file).length).toBe(ONE_MIB + 1);
  });
});

describe("writeReadOnlyFile(読み取り専用のファイル)", () => {
  it("書き込めない環境では、書き込めるファイルを残さずに null を返す(root の判定)", () => {
    const originalGetuid = process.getuid;
    // root で動く環境を模擬する: getuid が 0 を返すとき null を返し、ファイルを作らない
    Object.defineProperty(process, "getuid", { value: () => 0, configurable: true });
    try {
      expect(writeReadOnlyFile(dir)).toBeNull();
      expect(existsSync(path.join(dir, "read-only.bf"))).toBe(false);
    } finally {
      if (originalGetuid === undefined) {
        delete (process as { getuid?: unknown }).getuid;
      } else {
        Object.defineProperty(process, "getuid", { value: originalGetuid, configurable: true });
      }
    }
  });

  it("属性が効かず書き込めてしまう環境では、ファイルを残さずに null を返す", () => {
    forceWritable.on = true;
    try {
      expect(writeReadOnlyFile(dir)).toBeNull();
      expect(existsSync(path.join(dir, "read-only.bf"))).toBe(false);
    } finally {
      forceWritable.on = false;
    }
  });

  it("書き込めない環境では、読み取り専用の属性のファイルのパスを返す", () => {
    const file = writeReadOnlyFile(dir);
    if (file === null) {
      // root で動く環境では null が返る(呼び出し側が skip する)。この場合は属性の検証を行わない
      expect(process.getuid?.()).toBe(0);
      return;
    }
    expect(path.dirname(file)).toBe(dir);
    expect(existsSync(file)).toBe(true);
    expect(readFileSync(file).length).toBeGreaterThan(0);
    // 読み取り専用の属性で、書き込みの試行が失敗すること
    expect(() => appendFileSync(file, "x")).toThrow();
  });
});

describe("missingDirectoryPath(存在しないディレクトリの下のパス)", () => {
  it("親ディレクトリが存在しないパスを返し、そのパスは dir の下にある", () => {
    const missing = missingDirectoryPath(dir);
    expect(path.dirname(path.dirname(missing))).toBe(dir);
    expect(existsSync(path.dirname(missing))).toBe(false);
    expect(existsSync(missing)).toBe(false);
  });
});
