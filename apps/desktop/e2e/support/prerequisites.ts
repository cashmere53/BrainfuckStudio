// E2E の準備物(画面のビルド成果物・IPC シム・ブリッジ・Chromium)の存在確認。
// playwright.config.ts が読み込まれる時点(webServer の起動より前)に呼ばれ、不足があれば
// 配信サーバやブリッジを起動する前に、不足の一覧と準備のコマンドを示して失敗させる。
import { existsSync } from "node:fs";
import { chromium } from "@playwright/test";
import { DIST_INDEX_PATH, SHIM_BUNDLE_PATH, bridgeBinaryPath } from "./paths.ts";

export type PrerequisiteKind = "dist" | "shim" | "bridge" | "chromium";

export type PrerequisitePaths = {
  readonly dist: string;
  readonly shim: string;
  readonly bridge: string;
  /** Chromium の実行ファイル。取得できない場合は undefined(不足として扱う) */
  readonly chromium: string | undefined;
};

const LABELS: Record<PrerequisiteKind, string> = {
  dist: "画面のビルド成果物(apps/desktop/dist/index.html)",
  shim: "IPC シム(apps/desktop/e2e/.cache/ipc-shim.js)",
  bridge: "テスト専用ブリッジ(brainfuck-e2e-bridge)",
  chromium: "Chromium(Playwright のブラウザ)",
};

/** 環境変数 E2E_SKIP_PREREQ=1 のときだけ確認を省く(--list・show-report・エディタ拡張・下流の webview 用) */
export function shouldSkipPrerequisites(env: NodeJS.ProcessEnv): boolean {
  return env.E2E_SKIP_PREREQ === "1";
}

/** 存在しない準備物の種類を、確認の順に返す(純粋関数。存在の判定は exists に任せる) */
export function missingPrerequisites(
  paths: PrerequisitePaths,
  exists: (path: string) => boolean,
): PrerequisiteKind[] {
  const missing: PrerequisiteKind[] = [];
  if (!exists(paths.dist)) missing.push("dist");
  if (!exists(paths.shim)) missing.push("shim");
  if (!exists(paths.bridge)) missing.push("bridge");
  if (paths.chromium === undefined || !exists(paths.chromium)) missing.push("chromium");
  return missing;
}

/** 不足の種類に対応する準備のコマンド(重複なし、確認の順) */
export function prerequisiteCommands(missing: readonly PrerequisiteKind[]): string[] {
  const commands: string[] = [];
  if (missing.some((kind) => kind !== "chromium")) {
    commands.push("npm run e2e:prepare");
  }
  if (missing.includes("chromium")) {
    commands.push("npx playwright install chromium");
  }
  return commands;
}

/** 実行中の環境でのパス一覧 */
export function defaultPrerequisitePaths(): PrerequisitePaths {
  let chromiumPath: string | undefined;
  try {
    chromiumPath = chromium.executablePath();
  } catch {
    chromiumPath = undefined;
  }
  return {
    dist: DIST_INDEX_PATH,
    shim: SHIM_BUNDLE_PATH,
    bridge: bridgeBinaryPath(process.env),
    chromium: chromiumPath,
  };
}

/**
 * 準備物が揃っているかを確かめ、不足があれば不足の一覧と準備のコマンドを含む例外を投げる。
 * `E2E_SKIP_PREREQ=1` のときは何もしない。
 */
export function assertPrerequisites(
  env: NodeJS.ProcessEnv,
  paths: PrerequisitePaths = defaultPrerequisitePaths(),
  exists: (path: string) => boolean = existsSync,
): void {
  if (shouldSkipPrerequisites(env)) {
    return;
  }
  const missing = missingPrerequisites(paths, exists);
  if (missing.length === 0) {
    return;
  }
  const lines = [
    "E2E の準備物が不足しています:",
    ...missing.map((kind) => `  - ${LABELS[kind]}`),
    "",
    "次のコマンドで準備してから再実行してください:",
    ...prerequisiteCommands(missing).map((command) => `  ${command}`),
    "",
    "(準備の確認を省く場合は E2E_SKIP_PREREQ=1 を指定します。--list や show-report などの用途向けです)",
  ];
  throw new Error(lines.join("\n"));
}
