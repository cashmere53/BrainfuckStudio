// @vitest-environment node
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  assertPrerequisites,
  missingPrerequisites,
  prerequisiteCommands,
  shouldSkipPrerequisites,
  type PrerequisitePaths,
} from "./prerequisites.ts";

const PATHS: PrerequisitePaths = {
  dist: path.join("/desk", "dist", "index.html"),
  shim: path.join("/desk", "e2e", ".cache", "ipc-shim.js"),
  bridge: path.join("/repo", "target", "debug", "brainfuck-e2e-bridge.exe"),
  chromium: path.join("/ms-playwright", "chromium", "chrome.exe"),
};

/** 指定したパスだけ存在するとみなす判定関数 */
function existingOnly(...present: string[]): (p: string) => boolean {
  const set = new Set(present);
  return (p) => set.has(p);
}

const ALL_PRESENT = existingOnly(PATHS.dist, PATHS.shim, PATHS.bridge, PATHS.chromium as string);

describe("missingPrerequisites(不足している準備物の判定)", () => {
  it("すべて揃っていれば空の一覧", () => {
    expect(missingPrerequisites(PATHS, ALL_PRESENT)).toEqual([]);
  });

  it("dist だけ欠けていれば dist だけを返す", () => {
    const exists = existingOnly(PATHS.shim, PATHS.bridge, PATHS.chromium as string);
    expect(missingPrerequisites(PATHS, exists)).toEqual(["dist"]);
  });

  it("シム・ブリッジ・Chromium のそれぞれが欠けた場合を個別に返す", () => {
    const withoutShim = existingOnly(PATHS.dist, PATHS.bridge, PATHS.chromium as string);
    expect(missingPrerequisites(PATHS, withoutShim)).toEqual(["shim"]);

    const withoutBridge = existingOnly(PATHS.dist, PATHS.shim, PATHS.chromium as string);
    expect(missingPrerequisites(PATHS, withoutBridge)).toEqual(["bridge"]);

    const withoutChromium = existingOnly(PATHS.dist, PATHS.shim, PATHS.bridge);
    expect(missingPrerequisites(PATHS, withoutChromium)).toEqual(["chromium"]);
  });

  it("Chromium の実行ファイルのパスが得られない(undefined)場合は chromium として不足", () => {
    const noChromiumPath: PrerequisitePaths = { ...PATHS, chromium: undefined };
    expect(missingPrerequisites(noChromiumPath, ALL_PRESENT)).toEqual(["chromium"]);
  });

  it("全部欠けていれば全部を順番どおりに返す", () => {
    expect(missingPrerequisites(PATHS, () => false)).toEqual(["dist", "shim", "bridge", "chromium"]);
  });
});

describe("prerequisiteCommands(案内の準備コマンド)", () => {
  it("dist・シム・ブリッジのどれかが欠けたら npm run e2e:prepare を案内する", () => {
    expect(prerequisiteCommands(["dist"])).toEqual(["npm run e2e:prepare"]);
    expect(prerequisiteCommands(["shim", "bridge"])).toEqual(["npm run e2e:prepare"]);
  });

  it("Chromium だけが欠けたら npx playwright install chromium を案内する", () => {
    expect(prerequisiteCommands(["chromium"])).toEqual(["npx playwright install chromium"]);
  });

  it("両方が欠けたら両方のコマンドを順に案内する", () => {
    expect(prerequisiteCommands(["bridge", "chromium"])).toEqual([
      "npm run e2e:prepare",
      "npx playwright install chromium",
    ]);
  });

  it("欠けたものが無ければ案内も無い", () => {
    expect(prerequisiteCommands([])).toEqual([]);
  });
});

describe("shouldSkipPrerequisites(E2E_SKIP_PREREQ の省略条件)", () => {
  it("E2E_SKIP_PREREQ=1 のときだけ省く", () => {
    expect(shouldSkipPrerequisites({ E2E_SKIP_PREREQ: "1" })).toBe(true);
  });

  it.each([undefined, "", "0", "true", "yes"])("値 %s では省かない", (value) => {
    expect(shouldSkipPrerequisites({ E2E_SKIP_PREREQ: value })).toBe(false);
  });
});

describe("assertPrerequisites(設定の読み込み時の確認)", () => {
  it("省略の指定があれば判定せず例外も出さない", () => {
    expect(() =>
      assertPrerequisites({ E2E_SKIP_PREREQ: "1" }, PATHS, () => false),
    ).not.toThrow();
  });

  it("準備物が揃っていれば例外を出さない", () => {
    expect(() => assertPrerequisites({}, PATHS, ALL_PRESENT)).not.toThrow();
  });

  it("不足があれば、不足の一覧と準備のコマンドを含む例外を投げる", () => {
    const exists = existingOnly(PATHS.shim, PATHS.bridge, PATHS.chromium as string);
    let message = "";
    try {
      assertPrerequisites({}, PATHS, exists);
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
    expect(message).toContain("dist/index.html");
    expect(message).toContain("npm run e2e:prepare");
    expect(message).not.toContain("npx playwright install chromium");
    expect(message).toContain("E2E_SKIP_PREREQ=1");
  });
});
