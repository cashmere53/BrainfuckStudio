import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StopReasonDto } from "../generated/StopReasonDto";
import type { UiPhase } from "../domain/controls";
import type { UiState } from "../domain/runLabel";
import { StatusPanel } from "./StatusPanel";

function stateOf(overrides: {
  pc?: number;
  dp?: number;
  currentCell?: number;
  steps?: number;
  inputConsumed?: number;
  inputTotal?: number;
  lastStop?: StopReasonDto | null;
}): SessionStateDto {
  return {
    sessionId: 1,
    snapshot: {
      pc: overrides.pc ?? 0,
      dp: overrides.dp ?? 0,
      currentCell: overrides.currentCell ?? 0,
      stepsExecuted: overrides.steps ?? 0,
      halted: overrides.lastStop === "halted",
    },
    tapeWindow: { start: 0, cells: [0], pointerOffset: 0 },
    output: { dataStart: 0, totalLen: 0, dataBase64: "" },
    inputConsumed: overrides.inputConsumed ?? 0,
    inputTotal: overrides.inputTotal ?? 0,
    lastStop: overrides.lastStop ?? null,
    currentRange: null,
  };
}

function ui(phase: UiPhase, stoppedByPause = false): UiState {
  return { phase, inputValid: true, stoppedByPause };
}

/** 状態領域の項目名(dt)に対応する値(dd)の表示文字列 */
function valueOf(label: string): string {
  const term = screen.getByText(label, { selector: "dt" });
  const group = term.parentElement;
  if (group === null) throw new Error(`no group for ${label}`);
  return within(group).getByText((_, el) => el?.tagName === "DD").textContent ?? "";
}

function labelElement(): HTMLElement {
  const live = screen.getByTestId("status-label");
  return live;
}

describe("StatusPanel(要件 10.1・10.3・10.10・3.9)", () => {
  it("pc・dp・現在セル・実行数・入力の読み取り済み/総数を 10 進で常に表示する(要件 10.1・3.9)", () => {
    render(
      <StatusPanel
        ui={ui("halted")}
        state={stateOf({ pc: 106, dp: 6, currentCell: 72, steps: 906, lastStop: "halted", inputConsumed: 2, inputTotal: 5 })}
      />,
    );
    expect(valueOf("pc")).toBe("106");
    expect(valueOf("dp")).toBe("6");
    expect(valueOf("現在のセル")).toBe("72");
    expect(valueOf("実行数")).toBe("906");
    expect(valueOf("入力の読み取り済み")).toBe("2");
    expect(valueOf("入力の総数")).toBe("5");
  });

  it("セッションが無いときは未開始の値(すべて 0)を表示する", () => {
    render(<StatusPanel ui={ui("noSession")} state={null} />);
    expect(valueOf("pc")).toBe("0");
    expect(valueOf("dp")).toBe("0");
    expect(valueOf("現在のセル")).toBe("0");
    expect(valueOf("実行数")).toBe("0");
    expect(labelElement().textContent).toBe("未開始");
  });

  it("状態ラベルを aria-live=polite の領域に表示する(要件 10.10)", () => {
    render(<StatusPanel ui={ui("noSession")} state={null} />);
    const live = labelElement();
    expect(live.getAttribute("aria-live")).toBe("polite");
    expect(live.textContent).toBe("未開始");
  });

  it("ラベルの変化は同じ aria-live 領域の内容として反映される(要件 10.10)", () => {
    const { rerender } = render(<StatusPanel ui={ui("idle")} state={stateOf({ steps: 0 })} />);
    expect(labelElement().textContent).toBe("未開始");
    rerender(<StatusPanel ui={ui("running")} state={stateOf({ steps: 10 })} />);
    expect(labelElement().textContent).toBe("実行中");
    expect(labelElement().getAttribute("aria-live")).toBe("polite");
    rerender(<StatusPanel ui={ui("failed")} state={stateOf({ steps: 10 })} />);
    expect(labelElement().textContent).toBe("実行エラー");
  });

  it("正常終了のとき「正常終了」と最終的な実行済み命令数を表示する(要件 10.3)", () => {
    render(<StatusPanel ui={ui("halted")} state={stateOf({ steps: 906, lastStop: "halted" })} />);
    expect(labelElement().textContent).toBe("正常終了");
    expect(screen.getByText("最終実行数: 906 命令")).toBeTruthy();
  });

  it("正常終了でないときは最終実行数の文を出さない", () => {
    render(<StatusPanel ui={ui("halted")} state={stateOf({ steps: 3, lastStop: "breakpoint" })} />);
    expect(labelElement().textContent).toBe("ブレークポイントで停止");
    expect(screen.queryByText(/最終実行数/)).toBeNull();
  });

  it("局所 Pause で止まったときは「一時停止」と表示する(stoppedByPause)", () => {
    render(<StatusPanel ui={ui("idle", true)} state={stateOf({ steps: 4, lastStop: null })} />);
    expect(labelElement().textContent).toBe("一時停止");
  });

  it("停止の確定で一時停止処理中の場合は「実行中」と表示する(Pausing は実行中に写す)", () => {
    render(<StatusPanel ui={ui("pausing")} state={stateOf({ steps: 4 })} />);
    expect(labelElement().textContent).toBe("実行中");
  });
});
