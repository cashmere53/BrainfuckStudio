// @vitest-environment node
import { describe, expect, it } from "vitest";
import { createDialogQueue } from "./dialogQueue.ts";

const PATH_A = { kind: "path", path: "/work/a.bf" } as const;
const PATH_B = { kind: "path", path: "/work/b.bf" } as const;
const CANCEL = { kind: "cancel" } as const;

/** Promise がまだ解決していないことを、マイクロタスクを回して確かめる */
async function isPending(promise: Promise<unknown>): Promise<boolean> {
  let settled = false;
  promise.then(
    () => {
      settled = true;
    },
    () => {
      settled = true;
    },
  );
  for (let i = 0; i < 5; i += 1) {
    await Promise.resolve();
  }
  return !settled;
}

describe("DialogQueue(先入れ先出し)", () => {
  it("同じ種類の指定を登録順に返す", async () => {
    const queue = createDialogQueue();
    queue.enqueue("open", PATH_A);
    queue.enqueue("open", PATH_B);
    expect(await queue.take("open")).toEqual(PATH_A);
    expect(await queue.take("open")).toEqual(PATH_B);
    expect(queue.unexpected).toEqual([]);
    expect(queue.remaining).toBe(0);
  });

  it("使われなかった指定の数を remaining で返す", async () => {
    const queue = createDialogQueue();
    queue.enqueue("save", PATH_A);
    queue.enqueue("save", CANCEL);
    expect(queue.remaining).toBe(2);
    expect(await queue.take("save")).toEqual(PATH_A);
    expect(queue.remaining).toBe(1);
  });
});

describe("DialogQueue(指定が無い・種類が違う問い合わせ)", () => {
  it("空の待ち行列への問い合わせは unexpected に記録し、取消を返す", async () => {
    const queue = createDialogQueue();
    expect(await queue.take("open")).toEqual(CANCEL);
    expect(queue.unexpected).toEqual(["open"]);
    expect(queue.remaining).toBe(0);
  });

  it("先頭と種類が違う問い合わせは記録して取消を返し、先頭の指定は消費しない", async () => {
    const queue = createDialogQueue();
    queue.enqueue("save", PATH_A);
    expect(await queue.take("open")).toEqual(CANCEL);
    expect(queue.unexpected).toEqual(["open"]);
    expect(queue.remaining).toBe(1);
    expect(await queue.take("save")).toEqual(PATH_A);
    expect(queue.remaining).toBe(0);
  });

  it("先頭と種類が違う問い合わせは、後ろに同じ種類の指定があっても消費しない", async () => {
    const queue = createDialogQueue();
    queue.enqueue("save", PATH_A);
    queue.enqueue("open", PATH_B);
    expect(await queue.take("open")).toEqual(CANCEL);
    expect(queue.unexpected).toEqual(["open"]);
    expect(queue.remaining).toBe(2);
    expect(await queue.take("save")).toEqual(PATH_A);
    expect(await queue.take("open")).toEqual(PATH_B);
    expect(queue.remaining).toBe(0);
  });

  it("記録した順に unexpected を並べ、読み取り専用の配列として見せる", async () => {
    const queue = createDialogQueue();
    await queue.take("save");
    await queue.take("open");
    expect(queue.unexpected).toEqual(["save", "open"]);
  });
});

describe("DialogQueue(保留と解放)", () => {
  it("hold の問い合わせは release まで解決しず、requested で到着を知らせる", async () => {
    const queue = createDialogQueue();
    const held = queue.hold("open");
    let requestedSeen = false;
    void held.requested.then(() => {
      requestedSeen = true;
    });

    const answer = queue.take("open");
    await held.requested;
    expect(requestedSeen).toBe(true);
    expect(await isPending(answer)).toBe(true);

    held.release(PATH_A);
    expect(await answer).toEqual(PATH_A);
    expect(queue.remaining).toBe(0);
  });

  it("到着の前に release しても、問い合わせにその結果を返す", async () => {
    const queue = createDialogQueue();
    const held = queue.hold("save");
    held.release(CANCEL);
    expect(await queue.take("save")).toEqual(CANCEL);
    expect(queue.unexpected).toEqual([]);
  });

  it("保留は次の 1 件だけに効き、後続の指定は待たずに順序どおり返る", async () => {
    const queue = createDialogQueue();
    const held = queue.hold("open");
    queue.enqueue("open", PATH_B);

    const first = queue.take("open");
    await held.requested;
    const second = queue.take("open");

    expect(await second).toEqual(PATH_B);
    expect(await isPending(first)).toBe(true);
    held.release(PATH_A);
    expect(await first).toEqual(PATH_A);
  });

  it("requested は問い合わせを take したときに解決し、hold や release だけでは解決しない", async () => {
    const queue = createDialogQueue();
    const held = queue.hold("open");
    expect(await isPending(held.requested)).toBe(true);
    held.release(PATH_A);
    expect(await isPending(held.requested)).toBe(true);

    const answer = queue.take("open");
    expect(await isPending(held.requested)).toBe(false);
    expect(await answer).toEqual(PATH_A);
  });

  it("release は自分の保留だけを解放し、後続の保留は解放しない", async () => {
    const queue = createDialogQueue();
    const h1 = queue.hold("open");
    const h2 = queue.hold("open");
    const first = queue.take("open");
    const second = queue.take("open");

    h1.release(PATH_A);
    expect(await first).toEqual(PATH_A);
    expect(await isPending(second)).toBe(true);

    h2.release(PATH_B);
    expect(await second).toEqual(PATH_B);
    expect(queue.unexpected).toEqual([]);
  });

  it("保留の指定は remaining に数えられ、使われるまで残る", async () => {
    const queue = createDialogQueue();
    queue.hold("open").release(PATH_A);
    expect(queue.remaining).toBe(1);
    await queue.take("open");
    expect(queue.remaining).toBe(0);
  });
});
