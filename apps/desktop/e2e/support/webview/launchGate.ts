// webview の実行の中で「CDP の口が開かない(AppLaunchError の reason が notReady)」ことを記録し、
// 以降のシナリオを起動せずに理由つきでスキップするための判定と記録。
// 最初の notReady で残りを待たずに終えるため、同じ実行の残りのシナリオは起動を試みない(実行時間を数分に抑える)。
// 記録は実行ごとのファイル。実行の開始時に setup プロジェクトが必ず消す(古い記録による誤判定を防ぐ)。
// 判定・記録の内容は純粋関数で、入出力(ファイル)は注入できる形にして単体テストする。
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { AppLaunchError } from "./appProcess.ts";
import { userDataRoot } from "./paths.ts";

/** 記録のファイル名(一時フォルダの webview の根の直下に置く。利用者データの掃除は `scenario-<uuid>` だけを消すため対象外) */
export const LAUNCH_GATE_FILE_NAME = "launch-gate.json";

/** 記録の中身。notReady の 1 件だけを残す */
export interface LaunchGateRecord {
  readonly reason: "notReady";
  /** 元の失敗メッセージの 1 行目(スキップの理由に含める) */
  readonly summary: string;
  readonly recordedAt: string;
}

/** 記録の読み書きと削除。ファイルの I/O を注入できるように分けている */
export interface LaunchGateStore {
  /** 無ければ null(読めなかった場合も null) */
  read(): string | null;
  write(content: string): void;
  /** 無くてもエラーにしない。消せなかった場合は投げる */
  remove(): void;
}

/** 記録のファイルのパス(root は webview の根。絶対パスに解決する) */
export function launchGateFilePath(root: string): string {
  return path.join(path.resolve(root), LAUNCH_GATE_FILE_NAME);
}

/** 実行の記録(一時フォルダの webview の根の下)の既定の入出力。tmpDir は一時フォルダ(os.tmpdir()) */
export function webviewLaunchGateStore(tmpDir: string): LaunchGateStore {
  return fileLaunchGateStore(launchGateFilePath(userDataRoot(tmpDir)));
}

/** 実ファイルに対する既定の記録の入出力 */
export function fileLaunchGateStore(filePath: string): LaunchGateStore {
  return {
    read() {
      try {
        return readFileSync(filePath, "utf8");
      } catch {
        return null;
      }
    },
    write(content) {
      mkdirSync(path.dirname(filePath), { recursive: true });
      writeFileSync(filePath, content, "utf8");
    },
    remove() {
      rmSync(filePath, { force: true });
    },
  };
}

/** 起動の失敗が CDP の口の未応答(notReady)かどうか */
export function isNotReadyLaunchError(error: unknown): error is AppLaunchError {
  return error instanceof AppLaunchError && error.reason === "notReady";
}

/** 例外メッセージの 1 行目 */
export function firstLine(message: string): string {
  return message.split("\n")[0] ?? "";
}

/**
 * 起動の失敗から記録を作る(純粋関数)。notReady 以外は null。
 * notReady 以外(早期終了・見つからない・アサーションの失敗)では記録しない(個別の失敗は全部実行する)。
 */
export function launchGateRecordFor(error: unknown, nowIso: string): LaunchGateRecord | null {
  if (!isNotReadyLaunchError(error)) {
    return null;
  }
  return { reason: "notReady", summary: firstLine(error.message), recordedAt: nowIso };
}

/**
 * 起動の失敗を記録する。notReady のときだけ書き、書けたかどうかを返す(投げない)。
 * 書けなかったときは、後続のシナリオはスキップされない(従来どおり全部起動する)。
 */
export function recordLaunchGate(store: LaunchGateStore, error: unknown, nowIso: string): boolean {
  const record = launchGateRecordFor(error, nowIso);
  if (record === null) {
    return false;
  }
  try {
    store.write(JSON.stringify(record));
    return true;
  } catch {
    return false;
  }
}

/** 記録の中身から、スキップの理由を作る(純粋関数)。記録が無い・壊れている・notReady でなければ null */
export function skipReasonFromRecord(raw: string | null): string | null {
  if (raw === null) {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) {
    return null;
  }
  const record = parsed as Record<string, unknown>;
  if (record.reason !== "notReady" || typeof record.summary !== "string") {
    return null;
  }
  return `最初のシナリオで CDP の口が開かなかったため(notReady)、この実行の残りのシナリオは起動せずスキップしました。元の失敗: ${record.summary}`;
}

/** 実行の開始時に記録を消す。消せなければ投げる(古い記録が残ったまま実行すると、誤ってスキップされるため) */
export function clearLaunchGate(store: LaunchGateStore): void {
  store.remove();
}
