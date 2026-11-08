// 一時的な利用者データのフォルダの根の掃除。削除の対象の選択は純粋関数で行い、削除の I/O は呼び出し側が担う(1.8)。
import path from "node:path";

/** AppLauncher が実行ごとに作るフォルダ名の接頭辞(`scenario-<uuid>`) */
export const SCENARIO_FOLDER_PREFIX = "scenario-";

/** これより新しいフォルダは、他の実行が使っているかもしれないため残す(既定の 1 時間) */
export const DEFAULT_STALE_AGE_MS = 60 * 60 * 1000;

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** 根の直下の 1 エントリ(readdir と lstat の結果) */
export type UserDataEntry = { name: string; isDirectory: boolean; mtimeMs: number };

/** 名前が `scenario-<uuid>`(接頭辞と 36 文字のハイフン区切りの 16 進)かどうか */
export function isScenarioFolderName(name: string): boolean {
  if (!name.startsWith(SCENARIO_FOLDER_PREFIX)) {
    return false;
  }
  return UUID_PATTERN.test(name.slice(SCENARIO_FOLDER_PREFIX.length));
}

/**
 * 削除すべき古い `scenario-<uuid>` のディレクトリの絶対パスを返す。対象の条件は、
 * (a) 名前が `scenario-<uuid>` のディレクトリ、(b) ファイルや他の名前は対象外、
 * (c) 古さが `minAgeMs` 未満(新しい)のものは対象外、(d) 区切り文字や `..` を含む名前は対象外、
 * (e) 結果が根の下にあることを確かめる。
 */
export function selectStaleUserDataEntries(
  root: string,
  entries: UserDataEntry[],
  nowMs: number,
  minAgeMs: number = DEFAULT_STALE_AGE_MS,
): string[] {
  const base = path.resolve(root);
  const selected: string[] = [];
  for (const entry of entries) {
    if (!entry.isDirectory) {
      continue;
    }
    if (/[/\\]/.test(entry.name) || entry.name.includes("..")) {
      continue;
    }
    if (!isScenarioFolderName(entry.name)) {
      continue;
    }
    if (nowMs - entry.mtimeMs < minAgeMs) {
      continue;
    }
    const target = path.join(base, entry.name);
    if (!target.startsWith(base + path.sep)) {
      continue;
    }
    selected.push(target);
  }
  return selected;
}
