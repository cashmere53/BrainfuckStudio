import { describe, expect, it } from "vitest";
import { availability, type UiPhase, type UiState } from "./controls";

type Row = { run: boolean; pause: boolean; step: boolean; reset: boolean; edit: boolean };

const OFF: Row = { run: false, pause: false, step: false, reset: false, edit: false };

/** 設計書の真理値表(design.md「操作の有効/無効」)と 1 対 1 で対応させる */
const TABLE: Array<{ ui: UiState; expected: Row; label: string }> = [
  {
    ui: { phase: "noSession", inputValid: true },
    expected: { run: true, pause: false, step: true, reset: false, edit: true },
    label: "NoSession(入力が有効)",
  },
  {
    ui: { phase: "noSession", inputValid: false },
    expected: { run: false, pause: false, step: false, reset: false, edit: true },
    label: "NoSession(16 進入力が不正)",
  },
  {
    ui: { phase: "idle", inputValid: true },
    expected: { run: true, pause: false, step: true, reset: true, edit: true },
    label: "Idle",
  },
  {
    ui: { phase: "running", inputValid: true },
    expected: { run: false, pause: true, step: false, reset: false, edit: false },
    label: "Running",
  },
  {
    ui: { phase: "pausing", inputValid: true },
    expected: OFF,
    label: "Pausing",
  },
  {
    ui: { phase: "halted", inputValid: true },
    expected: { run: false, pause: false, step: false, reset: true, edit: true },
    label: "Halted",
  },
  {
    ui: { phase: "failed", inputValid: true },
    expected: { run: false, pause: false, step: false, reset: true, edit: true },
    label: "Failed",
  },
];

describe("availability: 状態 × 操作の真理値表", () => {
  for (const { ui, expected, label } of TABLE) {
    it(`${label}: 操作の有効/無効が表どおり`, () => {
      expect(availability(ui)).toEqual(expected);
    });
  }

  it("全フェーズ × 入力有効/無効の全組合せで、Pause は Running 以外で無効", () => {
    const phases: UiPhase[] = ["noSession", "idle", "running", "pausing", "halted", "failed"];
    for (const phase of phases) {
      for (const inputValid of [true, false]) {
        const c = availability({ phase, inputValid });
        expect(c.pause).toBe(phase === "running");
      }
    }
  });

  it("実行中(running/pausing)は編集・ファイル操作・入力変更を受け付けない", () => {
    for (const phase of ["running", "pausing"] as const) {
      expect(availability({ phase, inputValid: true }).edit).toBe(false);
    }
  });

  it("停止状態(halted/failed)では Run と Step が無効で Reset のみ再開に使える", () => {
    for (const phase of ["halted", "failed"] as const) {
      const c = availability({ phase, inputValid: true });
      expect(c.run).toBe(false);
      expect(c.step).toBe(false);
      expect(c.reset).toBe(true);
    }
  });

  it("入力が不正なら、セッション未開始でも Run/Step は無効", () => {
    const c = availability({ phase: "noSession", inputValid: false });
    expect(c.run).toBe(false);
    expect(c.step).toBe(false);
  });
});
