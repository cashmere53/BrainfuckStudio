import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type { OutputChunkDto } from "../generated/OutputChunkDto";
import { encodeBase64 } from "../domain/base64";
import { OUTPUT_TAIL_LIMIT } from "../domain/limits";
import { applyChunk, emptyOutput, type OutputBuffer } from "../domain/outputBuffer";
import type { OutputMode } from "../state/workspace";
import { OutputView } from "./OutputView";

function chunk(dataStart: number, totalLen: number, bytes: Uint8Array): OutputChunkDto {
  return { dataStart, totalLen, dataBase64: encodeBase64(bytes) };
}

/** 先頭から全バイトが保持されている出力 */
function bufferOf(bytes: number[]): OutputBuffer {
  return applyChunk(emptyOutput(), chunk(0, bytes.length, new Uint8Array(bytes)));
}

/**
 * 表示モードを親が保持する想定の包装。切替クリックで親の状態を更新し、その値を mode として渡す。
 * (OutputView 自体は表示モードを内部に持たないため、切替の結果は親の再描画で反映される)
 */
function ControlledOutputView(props: { output: OutputBuffer; initialMode: OutputMode }) {
  const [mode, setMode] = useState<OutputMode>(props.initialMode);
  return <OutputView output={props.output} mode={mode} onModeChange={setMode} />;
}

function renderView(output: OutputBuffer, initialMode: OutputMode = "text") {
  render(<ControlledOutputView output={output} initialMode={initialMode} />);
}

function switchTo(label: "テキスト" | "16 進") {
  fireEvent.click(screen.getByRole("radio", { name: label }));
}

function radioChecked(label: "テキスト" | "16 進"): boolean {
  return (screen.getByRole("radio", { name: label }) as HTMLInputElement).checked;
}

describe("OutputView(要件 9.1・9.2・9.7〜9.9)", () => {
  it("mode=text のときテキストを表示し、UTF-8 として解釈する(要件 9.1・9.2)", () => {
    // 「あ」= E3 81 82 と ASCII "Hi"
    renderView(bufferOf([0x48, 0x69, 0xe3, 0x81, 0x82]), "text");
    expect(radioChecked("テキスト")).toBe(true);
    expect(screen.getByLabelText("出力の内容(テキスト)").textContent).toBe("Hiあ");
    expect(screen.queryByLabelText("出力の内容(16 進)")).toBeNull();
  });

  it("不正な UTF-8 は置換文字で表示し、改行は保持し、制御文字は記号で表示する(要件 9.3・9.5)", () => {
    renderView(bufferOf([0x41, 0xff, 0x0a, 0x01]), "text");
    expect(screen.getByLabelText("出力の内容(テキスト)").textContent).toBe("A�\n␁");
  });

  it("mode=hex のとき、先頭からのバイト位置つきの 2 桁 16 進を表示し、テキストは表示しない(要件 9.2・9.6)", () => {
    renderView(bufferOf([0x48, 0x69]), "hex");
    expect(radioChecked("16 進")).toBe(true);
    expect(screen.queryByLabelText("出力の内容(テキスト)")).toBeNull();
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(1);
    expect(rows[0]?.textContent).toContain("00000000");
    expect(rows[0]?.textContent).toContain("48 69");
  });

  it("切替で表示内容が変わっても出力のバイト列と総数は変わらない(要件 9.2)", () => {
    const output = bufferOf([0x48, 0x69, 0x0a]);
    const before = Array.from(output.bytes);
    renderView(output, "text");
    switchTo("16 進");
    switchTo("テキスト");
    switchTo("16 進");
    expect(Array.from(output.bytes)).toEqual(before);
    expect(output.totalLen).toBe(3);
    expect(screen.getByText("総バイト数: 3 バイト")).toBeTruthy();
  });

  it("テキスト・16 進のどちらでも正確な総バイト数を表示する(要件 9.7)", () => {
    renderView(bufferOf([0x00, 0x01, 0x02]), "text");
    expect(screen.getByText("総バイト数: 3 バイト")).toBeTruthy();
    switchTo("16 進");
    expect(screen.getByText("総バイト数: 3 バイト")).toBeTruthy();
  });

  it("65,536 バイトを超える出力は末尾だけを表示し、先頭の省略と正確な総バイト数を示す(要件 9.8)", () => {
    const omitted = 10;
    const kept = new Uint8Array(OUTPUT_TAIL_LIMIT).fill(0x41);
    const output = applyChunk(emptyOutput(), chunk(omitted, OUTPUT_TAIL_LIMIT + omitted, kept));
    renderView(output, "text");

    expect(screen.getByText(`総バイト数: ${OUTPUT_TAIL_LIMIT + omitted} バイト`)).toBeTruthy();
    expect(screen.getByText(/先頭 10 バイトを省略/)).toBeTruthy();
    expect(screen.getByLabelText("出力の内容(テキスト)").textContent).toBe("A".repeat(OUTPUT_TAIL_LIMIT));

    switchTo("16 進");
    // 16 進の行頭は出力全体における絶対位置(省略分を含む)
    expect(screen.getAllByRole("listitem")[0]?.textContent).toContain("0000000A");
  });

  it("省略がない出力では省略メッセージを表示しない(要件 9.8)", () => {
    renderView(bufferOf([0x41]), "text");
    expect(screen.queryByText(/省略/)).toBeNull();
  });

  it("出力がまだ空の場合は空であることを表示する(要件 9.9)", () => {
    renderView(emptyOutput(), "text");
    expect(screen.getByText("出力はまだありません")).toBeTruthy();
    expect(screen.getByText("総バイト数: 0 バイト")).toBeTruthy();
    expect(screen.queryByLabelText("出力の内容(テキスト)")).toBeNull();

    switchTo("16 進");
    expect(screen.getByText("出力はまだありません")).toBeTruthy();
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });

  it("ラジオをクリックすると onModeChange を選んだモードで 1 回呼ぶ(要件 9.2)", () => {
    const onModeChange = vi.fn();
    const output = bufferOf([0x41]);
    const { rerender } = render(<OutputView output={output} mode="text" onModeChange={onModeChange} />);

    switchTo("16 進");
    expect(onModeChange).toHaveBeenCalledTimes(1);
    expect(onModeChange).toHaveBeenLastCalledWith("hex");

    // 選択中のモード(hex)のラジオは change を発火しないため、親が mode=hex に更新した後に戻す
    rerender(<OutputView output={output} mode="hex" onModeChange={onModeChange} />);
    switchTo("テキスト");
    expect(onModeChange).toHaveBeenCalledTimes(2);
    expect(onModeChange).toHaveBeenLastCalledWith("text");
  });

  it("checked は内部状態ではなく mode プロップに従う(要件 9.2)", () => {
    // 親が mode を更新しないので、クリック後も checked は mode=text のままになる
    const onModeChange = vi.fn();
    const { rerender } = render(<OutputView output={bufferOf([0x41])} mode="text" onModeChange={onModeChange} />);
    switchTo("16 進");
    expect(onModeChange).toHaveBeenCalledWith("hex");
    expect(radioChecked("テキスト")).toBe(true);
    expect(radioChecked("16 進")).toBe(false);
    expect(screen.getByLabelText("出力の内容(テキスト)").textContent).toBe("A");

    // 親が mode=hex を渡せば、表示と checked がそれに従う
    rerender(<OutputView output={bufferOf([0x41])} mode="hex" onModeChange={onModeChange} />);
    expect(radioChecked("16 進")).toBe(true);
    expect(radioChecked("テキスト")).toBe(false);
    expect(screen.queryByLabelText("出力の内容(テキスト)")).toBeNull();
    expect(screen.getAllByRole("listitem")[0]?.textContent).toContain("41");
  });
});
