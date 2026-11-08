// @vitest-environment node
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_E2E_PORT,
  e2ePort,
  resolveBridgeBinary,
} from "./paths.ts";

const REPO = path.resolve("/repo");

describe("resolveBridgeBinary(ブリッジの実行ファイルの優先順位と拡張子)", () => {
  it("E2E_BRIDGE_BIN が最優先で、CARGO_TARGET_DIR より先に使われる", () => {
    const bin = resolveBridgeBinary(
      { E2E_BRIDGE_BIN: "/custom/bridge.exe", CARGO_TARGET_DIR: "/target-dir" },
      { repoRoot: REPO, platform: "win32" },
    );
    expect(bin).toBe(path.resolve("/custom/bridge.exe"));
  });

  it("E2E_BRIDGE_BIN が無ければ CARGO_TARGET_DIR の debug 配下を使う(Windows は .exe)", () => {
    const bin = resolveBridgeBinary(
      { CARGO_TARGET_DIR: "/target-dir" },
      { repoRoot: REPO, platform: "win32" },
    );
    expect(bin).toBe(path.join(path.resolve("/target-dir"), "debug", "brainfuck-e2e-bridge.exe"));
  });

  it("どちらも無ければリポジトリ直下の target/debug を使う(Windows は .exe)", () => {
    const bin = resolveBridgeBinary({}, { repoRoot: REPO, platform: "win32" });
    expect(bin).toBe(path.join(REPO, "target", "debug", "brainfuck-e2e-bridge.exe"));
  });

  it("Windows 以外は拡張子を付けない", () => {
    const bin = resolveBridgeBinary({}, { repoRoot: REPO, platform: "linux" });
    expect(bin).toBe(path.join(REPO, "target", "debug", "brainfuck-e2e-bridge"));
  });

  it("空文字の環境変数は未設定として扱う", () => {
    const bin = resolveBridgeBinary(
      { E2E_BRIDGE_BIN: "", CARGO_TARGET_DIR: "" },
      { repoRoot: REPO, platform: "linux" },
    );
    expect(bin).toBe(path.join(REPO, "target", "debug", "brainfuck-e2e-bridge"));
  });
});

describe("e2ePort(E2E_PORT の既定値と検証)", () => {
  it("未設定なら 14273(1420 とは別)", () => {
    expect(DEFAULT_E2E_PORT).toBe(14273);
    expect(e2ePort({})).toBe(14273);
    expect(e2ePort({ E2E_PORT: "" })).toBe(14273);
  });

  it("数字の指定をそのまま使う", () => {
    expect(e2ePort({ E2E_PORT: "15000" })).toBe(15000);
  });

  it("既定は開発サーバの 1420 と一致しない", () => {
    expect(DEFAULT_E2E_PORT).not.toBe(1420);
  });

  it.each(["abc", "0", "65536", "-1", "14273.5"])("不正な値 %s は例外にする", (value) => {
    expect(() => e2ePort({ E2E_PORT: value })).toThrow(/E2E_PORT/);
  });
});
