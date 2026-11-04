import "./InputPanel.css";
import { useId } from "react";
import { byteCount, type InputError, type InputMode, type InputModel, parseInput, switchMode } from "../domain/inputBytes";
import { MAX_INPUT_BYTES } from "../domain/limits";

export interface InputPanelProps {
  input: InputModel;
  /** 入力モデルが変わったとき(文字入力、モード切替の成功)に呼ばれる */
  onChange: (input: InputModel) => void;
  /** モード切替が拒否されたとき(16 進 → テキストで UTF-8 として不正など)に理由とともに呼ばれる。モードは変わらない */
  onSwitchRejected: (reason: InputError["kind"] | "notUtf8") => void;
  /** 入力の編集を止める(実行中など)。省略時は編集可能 */
  disabled?: boolean;
}

const MODE_LABELS: Record<InputMode, string> = { text: "テキスト", hex: "16 進" };

/**
 * 入力設定(要件 3.1〜3.8)。テキスト/16 進のモード切替、入力欄、総バイト数、入力エラーの種類と位置を表示する。
 * 入力の正当性の判定とモード切替の規則は `domain/inputBytes` が担い、このコンポーネントは結果を表示して通知するだけ。
 */
export function InputPanel(props: InputPanelProps) {
  const { input, disabled = false } = props;
  const groupName = useId();
  const errorId = useId();
  const parsed = parseInput(input);
  const total = byteCount(input);

  function selectMode(to: InputMode) {
    if (to === input.mode) {
      return;
    }
    const result = switchMode(input, to);
    if (result.ok) {
      props.onChange(result.model);
    } else {
      props.onSwitchRejected(result.reason);
    }
  }

  return (
    <div className="input-panel">
      <div className="input-panel-modes" role="radiogroup" aria-label="入力の形式">
        {(Object.keys(MODE_LABELS) as InputMode[]).map((mode) => (
          <label key={mode} className="input-panel-mode">
            <input
              type="radio"
              name={groupName}
              value={mode}
              checked={input.mode === mode}
              disabled={disabled}
              onChange={() => selectMode(mode)}
            />
            {MODE_LABELS[mode]}
          </label>
        ))}
      </div>
      <textarea
        className="input-panel-text"
        aria-label="入力"
        aria-invalid={parsed.ok ? "false" : "true"}
        aria-describedby={parsed.ok ? undefined : errorId}
        rows={3}
        spellCheck={false}
        value={input.text}
        disabled={disabled}
        onChange={(event) => props.onChange({ mode: input.mode, text: event.target.value })}
      />
      {parsed.ok ? (
        <p className="input-panel-bytes">総バイト数: {total} バイト</p>
      ) : (
        <p className="input-panel-error" id={errorId} role="alert">
          {describeError(parsed.error)}
        </p>
      )}
    </div>
  );
}

/** 入力エラーの日本語表示。位置は元の入力文字列上の 1 始まりの文字位置 */
function describeError(error: InputError): string {
  switch (error.kind) {
    case "invalidChar":
      return `入力エラー: 16 進数字・空白以外の文字が含まれています(${error.index + 1} 文字目)`;
    case "oddDigits":
      return "入力エラー: 16 進数字の総数が奇数です(2 桁ずつ区切ってください)";
    case "tooLarge":
      return `入力エラー: 入力が 1 MiB(${MAX_INPUT_BYTES.toLocaleString("en-US")} バイト)を超えているため受け付けません`;
  }
}
