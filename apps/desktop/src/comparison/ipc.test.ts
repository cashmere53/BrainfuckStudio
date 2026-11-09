import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ApiError } from "../generated/ApiError";
import type { ComparisonInfoDto } from "../generated/ComparisonInfoDto";
import type { ComparisonResultDto } from "../generated/ComparisonResultDto";
import type { RunComparisonRequest } from "../generated/RunComparisonRequest";
import { COMMANDS, invokeCommand } from "../ipc/backend";
import { COMPARISON_COMMANDS, createFakeComparisonBackend, tauriComparisonBackend } from "./ipc";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  invokeMock.mockReset();
});

const INFO: ComparisonInfoDto = {
  maxSteps: 1000,
  maxRepetitions: 5,
  defaultRepetitions: 1,
  methods: ["naive", "optimized", "wasm"],
};

const REQUEST: RunComparisonRequest = {
  source: "+.",
  inputBase64: "",
  maxSteps: 100,
  methods: ["optimized"],
  repetitions: 1,
};

const RESULT: ComparisonResultDto = { kind: "completed", limit: 100, overall: "allMatch", methods: [] };

describe("tauriComparisonBackend: 共有ヘルパ invokeCommand 経由の呼び出し", () => {
  it("コマンド名は get_comparison_info / run_comparison で、引数は { args } の 1 個にまとめる", async () => {
    expect(COMPARISON_COMMANDS).toEqual({ getInfo: "get_comparison_info", run: "run_comparison" });
    expect(COMMANDS.getComparisonInfo).toBe(COMPARISON_COMMANDS.getInfo);
    expect(COMMANDS.runComparison).toBe(COMPARISON_COMMANDS.run);

    invokeMock.mockResolvedValueOnce(INFO);
    await expect(tauriComparisonBackend.getInfo()).resolves.toEqual(INFO);
    expect(invokeMock).toHaveBeenLastCalledWith("get_comparison_info", undefined);

    invokeMock.mockResolvedValueOnce(RESULT);
    await expect(tauriComparisonBackend.run(REQUEST)).resolves.toEqual(RESULT);
    expect(invokeMock).toHaveBeenLastCalledWith("run_comparison", { args: REQUEST });
  });

  it("拒否値は共有ヘルパと同じく ApiError に正規化して reject する", async () => {
    const err: ApiError = { kind: "invalidArgument", message: "bad", field: "maxSteps" };
    invokeMock.mockRejectedValueOnce(err);
    await expect(tauriComparisonBackend.run(REQUEST)).rejects.toEqual(err);

    invokeMock.mockRejectedValueOnce("raw");
    await expect(tauriComparisonBackend.getInfo()).rejects.toMatchObject({ kind: "internal", field: null });
  });

  it("共有ヘルパ invokeCommand と同じ呼び出し形を使う(再実装しない)", async () => {
    invokeMock.mockResolvedValueOnce(RESULT);
    await invokeCommand("run_comparison", REQUEST);
    const direct = invokeMock.mock.calls[0];

    invokeMock.mockResolvedValueOnce(RESULT);
    await tauriComparisonBackend.run(REQUEST);
    expect(invokeMock.mock.calls[1]).toEqual(direct);
  });
});

describe("createFakeComparisonBackend: 台本どおりの応答", () => {
  it("台本の応答を返し、呼び出しを記録する", async () => {
    const fake = createFakeComparisonBackend({
      getInfo: () => INFO,
      run: () => RESULT,
    });
    await expect(fake.getInfo()).resolves.toEqual(INFO);
    await expect(fake.run(REQUEST)).resolves.toEqual(RESULT);
    expect(fake.calls).toEqual([
      { method: "getInfo", args: [] },
      { method: "run", args: [REQUEST] },
    ]);
  });

  it("台本に無いメソッドは internal で reject する", async () => {
    const fake = createFakeComparisonBackend({});
    await expect(fake.run(REQUEST)).rejects.toMatchObject({ kind: "internal" });
  });

  it("台本が投げた値は ApiError に正規化して reject する", async () => {
    const fake = createFakeComparisonBackend({
      run: () => {
        throw "not an error";
      },
    });
    await expect(fake.run(REQUEST)).rejects.toMatchObject({ kind: "internal", field: null });
  });
});
