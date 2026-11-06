// フロントのコマンド名・イベント名が Rust 側の定義と一致することを固定する。
// 権限生成の一覧(names.rs)・ケイパビリティの許可(default.json)・イベント名(window.rs)を読んで比べる。
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { CLOSE_REQUESTED_EVENT, COMMANDS } from "./backend";

const srcTauri = `${resolve(import.meta.dirname, "../../src-tauri")}/`;

function frontendCommandNames(): string[] {
  return Object.values(COMMANDS).sort();
}

describe("コマンド名の契約", () => {
  it("names.rs の COMMAND_NAMES とフロントのコマンド名定数が一致する", () => {
    const rust = readFileSync(`${srcTauri}src/commands/names.rs`, "utf8");
    const block = /COMMAND_NAMES: &\[&str\] = &\[([\s\S]*?)\];/.exec(rust)?.[1];
    expect(block).toBeDefined();
    const names = [...(block ?? "").matchAll(/"([a-z_]+)"/g)].map((m) => m[1] ?? "").sort();
    expect(names).toEqual(frontendCommandNames());
  });

  it("ケイパビリティの許可一覧(allow-*)がフロントのコマンド名と一致する", () => {
    const capability = JSON.parse(readFileSync(`${srcTauri}capabilities/default.json`, "utf8")) as {
      permissions: string[];
    };
    const allowed = capability.permissions
      .filter((p) => p.startsWith("allow-"))
      .map((p) => p.slice("allow-".length).replaceAll("-", "_"))
      .sort();
    expect(allowed).toEqual(frontendCommandNames());
  });
});

describe("イベント名の契約", () => {
  it("CLOSE_REQUESTED_EVENT は Rust の window.rs の定数と一致する", () => {
    const rust = readFileSync(`${srcTauri}src/window.rs`, "utf8");
    const value = /CLOSE_REQUESTED_EVENT: &str = "([^"]+)"/.exec(rust)?.[1];
    expect(value).toBe("close-requested");
    expect(CLOSE_REQUESTED_EVENT).toBe(value);
  });
});
