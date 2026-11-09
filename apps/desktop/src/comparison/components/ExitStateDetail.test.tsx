import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { encodeBase64 } from "../../domain/base64";
import type { ExitStateDto } from "../../generated/ExitStateDto";
import type { TerminationDto } from "../../generated/TerminationDto";
import { ExitStateDetail } from "./ExitStateDetail";

const encoder = new TextEncoder();

/** 正常停止・出力 "Hi\n"(先頭から全保持)・入力 2/5 の終了状態 */
function haltedExit(overrides: Partial<ExitStateDto> = {}): ExitStateDto {
  return {
    termination: { kind: "halted" },
    pc: 10,
    dp: 2,
    currentCell: 65,
    stepsExecuted: 906,
    inputConsumed: 2,
    inputTotal: 5,
    outputTail: { dataStart: 0, totalLen: 3, dataBase64: encodeBase64(encoder.encode("Hi\n")) },
    tapeWindow: { start: 0, cells: [0, 0, 65], pointerOffset: 2 },
    tapeDigest: "00000000deadbeef",
    ...overrides,
  };
}

function exitWith(termination: TerminationDto, overrides: Partial<ExitStateDto> = {}): ExitStateDto {
  return haltedExit({ termination, ...overrides });
}

/** 説明一覧(dl)の用語に対応する値(dd)の文字列。用語がなければ null */
function valueOf(term: string): string | null {
  const dt = Array.from(document.querySelectorAll("dt")).find((item) => item.textContent === term);
  return dt?.nextElementSibling?.textContent ?? null;
}

describe("ExitStateDetail(要件 10.3)", () => {
  it("正常停止の最終状態(pc・dp・現在セル・実行数・入力読取位置・指紋)を表示する", () => {
    render(<ExitStateDetail exit={haltedExit()} />);

    expect(valueOf("終了の種類")).toBe("正常停止");
    expect(valueOf("最終の pc")).toBe("10");
    expect(valueOf("最終の dp")).toBe("2");
    expect(valueOf("現在セル")).toBe("65");
    expect(valueOf("実行数(素朴換算)")).toBe("906");
    expect(valueOf("入力読取位置")).toBe("2 / 5");
    expect(valueOf("テープの指紋")).toBe("00000000deadbeef");
  });

  it("正常停止ではエラーのフィールドを表示しない", () => {
    render(<ExitStateDetail exit={haltedExit()} />);
    expect(valueOf("移動方向")).toBeNull();
    expect(valueOf("上限")).toBeNull();
    expect(valueOf("エラーの pc")).toBeNull();
  });

  it("ポインタ範囲外のエラーフィールド(pc・dp・移動方向)を表示する", () => {
    render(
      <ExitStateDetail
        exit={exitWith({ kind: "error", error: { kind: "pointerOutOfBounds", pc: 3, dp: 0, direction: "left" } })}
      />,
    );

    expect(valueOf("終了の種類")).toBe("ポインタ範囲外(pc: 3, dp: 0, 左へ移動)");
    expect(valueOf("エラーの pc")).toBe("3");
    expect(valueOf("エラーの dp")).toBe("0");
    expect(valueOf("移動方向")).toBe("左");
  });

  it("ポインタ範囲外で右方向の移動は「右」と表示する", () => {
    render(
      <ExitStateDetail
        exit={exitWith({ kind: "error", error: { kind: "pointerOutOfBounds", pc: 1, dp: 4, direction: "right" } })}
      />,
    );
    expect(valueOf("移動方向")).toBe("右");
  });

  it("命令数上限超過のエラーフィールド(上限・pc)を表示する", () => {
    render(
      <ExitStateDetail exit={exitWith({ kind: "error", error: { kind: "stepLimitExceeded", limit: 1000, pc: 7 } })} />,
    );

    expect(valueOf("終了の種類")).toBe("命令数上限超過(上限: 1000, pc: 7)");
    expect(valueOf("上限")).toBe("1000");
    expect(valueOf("エラーの pc")).toBe("7");
    expect(valueOf("移動方向")).toBeNull();
  });

  it("出力は総バイト数と末尾を表示し、省略がある場合は省略の旨を示す", () => {
    const exit = haltedExit({
      outputTail: { dataStart: 10, totalLen: 13, dataBase64: encodeBase64(encoder.encode("Hi\n")) },
    });
    render(<ExitStateDetail exit={exit} />);

    expect(screen.getByText("総バイト数: 13 バイト")).toBeTruthy();
    expect(screen.getByText(/先頭 10 バイトを省略しています/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "出力" }).textContent).toContain("Hi");
  });

  it("出力の表示形式を 16 進に切り替えると、末尾の各バイトを 16 進で示す(desktop-gui の表示を再利用)", () => {
    render(<ExitStateDetail exit={haltedExit()} />);

    fireEvent.click(screen.getByRole("radio", { name: "16 進" }));

    const hex = screen.getByRole("list", { name: "出力の内容(16 進)" });
    expect(hex.textContent).toContain("48 69 0A");
  });

  it("ポインタ周辺のテープを強調つきで表示し、dp のセルを aria-current で示す", () => {
    render(<ExitStateDetail exit={haltedExit()} />);

    const pointer = screen.getByRole("listitem", { name: /^セル 2、/ });
    expect(pointer.getAttribute("aria-current")).toBe("true");
    expect(pointer.textContent).toContain("▼ dp");
  });
});
