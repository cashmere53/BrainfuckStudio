// @vitest-environment node
// 封筒を開く関数(unwrapOutcome)の単体テスト。成功は値で解決し、失敗はエラーの値そのもので reject する(2.1)。
import { describe, expect, it } from "vitest";
import { unwrapOutcome, type InvokeOutcome } from "./envelope.ts";

describe("unwrapOutcome", () => {
  it("成功の封筒は value で解決する", async () => {
    const outcome: InvokeOutcome = { ok: true, value: { pc: 106, dp: 6 } };

    await expect(unwrapOutcome(outcome)).resolves.toEqual({ pc: 106, dp: 6 });
  });

  it("成功で value が undefined なら undefined で解決する", async () => {
    await expect(unwrapOutcome({ ok: true, value: undefined })).resolves.toBeUndefined();
  });

  it("失敗の封筒は error の値そのもので reject する(同一の参照)", async () => {
    const error = { code: "InvalidArgument", message: "bad" };

    const rejection = await unwrapOutcome({ ok: false, error }).then(
      () => undefined,
      (reason: unknown) => reason,
    );

    expect(rejection).toBe(error);
  });

  it("文字列のエラーもそのまま reject する", async () => {
    const rejection = await unwrapOutcome({ ok: false, error: "bridge exited" }).then(
      () => undefined,
      (reason: unknown) => reason,
    );

    expect(rejection).toBe("bridge exited");
  });
});
