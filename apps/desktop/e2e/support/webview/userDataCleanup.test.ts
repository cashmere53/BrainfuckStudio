// @vitest-environment node
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_STALE_AGE_MS,
  SCENARIO_FOLDER_PREFIX,
  isScenarioFolderName,
  selectStaleUserDataEntries,
  type UserDataEntry,
} from "./userDataCleanup.ts";

const ROOT = path.resolve("bfstudio-e2e-webview");
const NOW = 1_800_000_000_000;
const HOUR = 60 * 60 * 1000;
const UUID = "3f2a9c1e-7b4d-4e8a-9f10-2c5d6e7f8a9b";
const SCENARIO = `scenario-${UUID}`;

/** 既定の古さ(2 時間前)のフォルダ */
function dir(name: string, ageMs: number = 2 * HOUR): UserDataEntry {
  return { name, isDirectory: true, mtimeMs: NOW - ageMs };
}

/** 既定の古さ(2 時間前)のファイル */
function file(name: string, ageMs: number = 2 * HOUR): UserDataEntry {
  return { name, isDirectory: false, mtimeMs: NOW - ageMs };
}

describe("SCENARIO_FOLDER_PREFIX", () => {
  it("AppLauncher が作るフォルダ名の接頭辞と一致する", () => {
    expect(SCENARIO_FOLDER_PREFIX).toBe("scenario-");
  });
});

describe("isScenarioFolderName", () => {
  it("接頭辞と 36 文字の UUID の名前を受け入れる", () => {
    expect(isScenarioFolderName(SCENARIO)).toBe(true);
    expect(isScenarioFolderName(`scenario-${UUID.toUpperCase()}`)).toBe(true);
  });

  it("接頭辞が無い・UUID でない・余分な文字がある名前を拒否する", () => {
    expect(isScenarioFolderName(UUID)).toBe(false);
    expect(isScenarioFolderName("other-thing")).toBe(false);
    expect(isScenarioFolderName("scenario-abc")).toBe(false);
    expect(isScenarioFolderName(`${SCENARIO}-x`)).toBe(false);
    expect(isScenarioFolderName(`x${SCENARIO}`)).toBe(false);
    expect(isScenarioFolderName("scenario-3f2a9c1e7b4d4e8a9f102c5d6e7f8a9b")).toBe(false);
    expect(isScenarioFolderName("scenario-3f2a9c1e-7b4d-4e8a-9f10-2c5d6e7f8a9g")).toBe(false);
  });

  it("区切り文字や .. を含む名前を拒否する", () => {
    expect(isScenarioFolderName(`${SCENARIO}/..`)).toBe(false);
    expect(isScenarioFolderName(`${SCENARIO}\\..`)).toBe(false);
    expect(isScenarioFolderName("scenario-/../../outside")).toBe(false);
    expect(isScenarioFolderName("..")).toBe(false);
  });
});

describe("selectStaleUserDataEntries", () => {
  it("古い scenario-<uuid> のディレクトリだけを絶対パスで返す", () => {
    expect(selectStaleUserDataEntries(ROOT, [dir(SCENARIO)], NOW)).toEqual([path.join(ROOT, SCENARIO)]);
  });

  it("空の一覧は空の結果を返す", () => {
    expect(selectStaleUserDataEntries(ROOT, [], NOW)).toEqual([]);
  });

  it("名前だけが scenario- でも UUID でなければ対象外にする", () => {
    expect(selectStaleUserDataEntries(ROOT, [dir("scenario-abc")], NOW)).toEqual([]);
  });

  it("対象外の名前のディレクトリ(他のフォルダ)は古くても残す", () => {
    expect(selectStaleUserDataEntries(ROOT, [dir("other-thing"), dir(UUID)], NOW)).toEqual([]);
  });

  it("ファイルは scenario-<uuid> の名前でも古くても対象外にする", () => {
    expect(selectStaleUserDataEntries(ROOT, [file("foreign.txt"), file(SCENARIO)], NOW)).toEqual([]);
  });

  it("ちょうど既定の古さ(1 時間)のものは対象にし、それより新しいものは対象外にする", () => {
    expect(DEFAULT_STALE_AGE_MS).toBe(HOUR);
    expect(selectStaleUserDataEntries(ROOT, [dir(SCENARIO, HOUR)], NOW)).toEqual([path.join(ROOT, SCENARIO)]);
    expect(selectStaleUserDataEntries(ROOT, [dir(SCENARIO, HOUR - 1)], NOW)).toEqual([]);
    expect(selectStaleUserDataEntries(ROOT, [dir(SCENARIO, 0)], NOW)).toEqual([]);
  });

  it("引数で指定した古さの下限を使う", () => {
    const entries = [dir(SCENARIO, 10 * 60 * 1000)];
    expect(selectStaleUserDataEntries(ROOT, entries, NOW, 5 * 60 * 1000)).toEqual([path.join(ROOT, SCENARIO)]);
    expect(selectStaleUserDataEntries(ROOT, entries, NOW, 20 * 60 * 1000)).toEqual([]);
  });

  it("区切り文字や .. を含む名前は、接頭辞と古さが合っても対象外にする", () => {
    const hostile = [
      dir(`${SCENARIO}/..`),
      dir(`${SCENARIO}\\..`),
      dir("scenario-/../../outside"),
      dir(".."),
      dir(`${SCENARIO}-x`),
    ];
    expect(selectStaleUserDataEntries(ROOT, hostile, NOW)).toEqual([]);
  });

  it("返す絶対パスは必ず根の直下にある", () => {
    const entries = [dir(SCENARIO), dir("scenario-/../../outside"), dir(`${SCENARIO}/..`)];
    const selected = selectStaleUserDataEntries(ROOT, entries, NOW);
    expect(selected.length).toBeGreaterThan(0);
    for (const target of selected) {
      expect(target.startsWith(path.resolve(ROOT) + path.sep)).toBe(true);
    }
  });

  it("古いものだけを一覧の順に、対象の組み合わせどおりに返す", () => {
    const otherStale = "scenario-11111111-1111-4111-8111-111111111111";
    const fresh = "scenario-22222222-2222-4222-8222-222222222222";
    const staleFile = "scenario-33333333-3333-4333-8333-333333333333";
    const entries = [
      dir(SCENARIO),
      file("foreign.txt"),
      dir("other-thing"),
      dir(fresh, 5 * 60 * 1000),
      dir(otherStale),
      file(staleFile),
    ];
    expect(selectStaleUserDataEntries(ROOT, entries, NOW)).toEqual([
      path.join(ROOT, SCENARIO),
      path.join(ROOT, otherStale),
    ]);
  });
});
