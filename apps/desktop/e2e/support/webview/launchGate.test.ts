// @vitest-environment node
import { mkdtemp, rm as fsRm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { AppLaunchError } from "./appProcess.ts";
import {
  clearLaunchGate,
  fileLaunchGateStore,
  launchGateFilePath,
  launchGateRecordFor,
  LAUNCH_GATE_FILE_NAME,
  recordLaunchGate,
  skipReasonFromRecord,
  type LaunchGateStore,
} from "./launchGate.ts";

/** 読み書きを配列の中に持つ偽の記録の入出力 */
function memoryStore(initial: string | null = null): LaunchGateStore & { content: string | null; removed: number } {
  const state = { content: initial as string | null, removed: 0 };
  return {
    get content() {
      return state.content;
    },
    get removed() {
      return state.removed;
    },
    read: () => state.content,
    write: (content: string) => {
      state.content = content;
    },
    remove: () => {
      state.removed += 1;
      state.content = null;
    },
  };
}

const NOW = "2026-11-10T00:00:00.000Z";
const NOT_READY = new AppLaunchError(
  "notReady",
  "アプリの CDP の口が 60000 ms 以内に応答しませんでした\n次を確かめてください:\n- …",
  "",
);

describe("launchGateRecordFor(notReady のときだけ記録を作る)", () => {
  it("notReady のときは、元のメッセージの 1 行目を含む記録を作る", () => {
    expect(launchGateRecordFor(NOT_READY, NOW)).toEqual({
      reason: "notReady",
      summary: "アプリの CDP の口が 60000 ms 以内に応答しませんでした",
      recordedAt: NOW,
    });
  });

  it.each(["exitedEarly", "notFound"] as const)("%s では記録を作らない", (reason) => {
    expect(launchGateRecordFor(new AppLaunchError(reason, "別の失敗", ""), NOW)).toBeNull();
  });

  it("AppLaunchError 以外(アサーションなど)では記録を作らない", () => {
    expect(launchGateRecordFor(new Error("expect failed"), NOW)).toBeNull();
    expect(launchGateRecordFor("notReady", NOW)).toBeNull();
  });
});

describe("recordLaunchGate(書き込み)", () => {
  it("(1) notReady でマーカーを作る", () => {
    const store = memoryStore();
    expect(recordLaunchGate(store, NOT_READY, NOW)).toBe(true);
    expect(store.content).not.toBeNull();
    expect(skipReasonFromRecord(store.content)).toContain("CDP の口が開かなかった");
  });

  it("(2) notReady 以外ではマーカーを作らない", () => {
    const store = memoryStore();
    expect(recordLaunchGate(store, new AppLaunchError("exitedEarly", "早期終了", ""), NOW)).toBe(false);
    expect(recordLaunchGate(store, new Error("アサーション"), NOW)).toBe(false);
    expect(store.content).toBeNull();
  });

  it("書き込みに失敗しても投げず、false を返す(後続は従来どおり起動する)", () => {
    const store: LaunchGateStore = {
      read: () => null,
      write: () => {
        throw new Error("書けない");
      },
      remove: () => {},
    };
    expect(recordLaunchGate(store, NOT_READY, NOW)).toBe(false);
  });
});

describe("skipReasonFromRecord(判定)", () => {
  it("(3) マーカーがあればスキップの理由を返す(理由に元の失敗の要約を含む)", () => {
    const store = memoryStore();
    recordLaunchGate(store, NOT_READY, NOW);
    const reason = skipReasonFromRecord(store.content);
    expect(reason).not.toBeNull();
    expect(reason).toContain("最初のシナリオで CDP の口が開かなかったため");
    expect(reason).toContain("アプリの CDP の口が 60000 ms 以内に応答しませんでした");
    expect(reason).not.toContain("次を確かめてください");
  });

  it("マーカーが無ければ null(起動する)", () => {
    expect(skipReasonFromRecord(null)).toBeNull();
  });

  it("壊れた内容・notReady 以外の内容では、スキップしない", () => {
    expect(skipReasonFromRecord("{not json")).toBeNull();
    expect(skipReasonFromRecord("null")).toBeNull();
    expect(skipReasonFromRecord(JSON.stringify({ reason: "exitedEarly", summary: "x" }))).toBeNull();
    expect(skipReasonFromRecord(JSON.stringify({ reason: "notReady" }))).toBeNull();
  });
});

describe("clearLaunchGate(実行の開始時の削除)", () => {
  it("(4) 開始時に記録を消す", () => {
    const store = memoryStore();
    recordLaunchGate(store, NOT_READY, NOW);
    clearLaunchGate(store);
    expect(store.removed).toBe(1);
    expect(skipReasonFromRecord(store.read())).toBeNull();
  });

  it("(5) 古い記録が残っていても、開始時の削除の後は誤ってスキップされない", () => {
    // 前の実行が残した記録
    const previous = memoryStore(JSON.stringify({ reason: "notReady", summary: "前の実行", recordedAt: NOW }));
    expect(skipReasonFromRecord(previous.read())).not.toBeNull();
    // 次の実行の開始時の削除
    clearLaunchGate(previous);
    expect(skipReasonFromRecord(previous.read())).toBeNull();
  });

  it("消せなかったときは投げる(古い記録のまま実行させない)", () => {
    const store: LaunchGateStore = {
      read: () => JSON.stringify({ reason: "notReady", summary: "x", recordedAt: NOW }),
      write: () => {},
      remove: () => {
        throw new Error("消せない");
      },
    };
    expect(() => clearLaunchGate(store)).toThrow("消せない");
  });
});

describe("実ファイルの入出力", () => {
  let dir: string;

  beforeEach(async () => {
    dir = await mkdtemp(path.join(tmpdir(), "launch-gate-test-"));
  });

  afterEach(async () => {
    await fsRm(dir, { recursive: true, force: true });
  });

  it("書いた記録を読み直せ、消すと読めなくなる。無いファイルの削除はエラーにならない", () => {
    const file = launchGateFilePath(dir);
    expect(path.basename(file)).toBe(LAUNCH_GATE_FILE_NAME);
    const store = fileLaunchGateStore(file);
    expect(store.read()).toBeNull();

    recordLaunchGate(store, NOT_READY, NOW);
    expect(skipReasonFromRecord(store.read())).not.toBeNull();

    clearLaunchGate(store);
    expect(store.read()).toBeNull();
    expect(() => clearLaunchGate(store)).not.toThrow();
  });
});
