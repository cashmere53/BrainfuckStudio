import "./StepLimitField.css";
import { useId } from "react";
import { type StepLimitReason, parseStepLimit } from "../domain/stepLimit";
import { MAX_STEP_LIMIT } from "../domain/limits";

export interface StepLimitFieldProps {
  /** 入力欄の文字列(不正値も含む) */
  text: string;
  /** 「上限なし」が選ばれているか */
  unlimited: boolean;
  onTextChange: (text: string) => void;
  onUnlimitedChange: (unlimited: boolean) => void;
  /** 編集を止める(実行中など)。省略時は編集可能 */
  disabled?: boolean;
}

const REASON_MESSAGES: Record<StepLimitReason, string> = {
  empty: "命令数上限を入力してください",
  notInteger: "命令数上限は 10 進整数で入力してください",
  belowMin: "命令数上限は 1 以上で入力してください",
  aboveMax: `命令数上限は ${MAX_STEP_LIMIT.toLocaleString("en-US")} 以下で入力してください`,
};

/**
 * 命令数上限の入力欄と「上限なし」(要件 5.1〜5.4)。
 * 不正値の理由を欄の近くに表示するだけで、直前の有効な値の保持と採用はワークスペースと実行制御が担う。
 * 「上限なし」を選ぶと入力欄は無効になり、出力やメモリ使用量が増え続けうることを警告する。
 */
export function StepLimitField(props: StepLimitFieldProps) {
  const { text, unlimited, disabled = false } = props;
  const inputId = useId();
  const errorId = useId();
  const result = unlimited ? null : parseStepLimit(text);
  const invalidReason = result !== null && !result.ok ? result.reason : null;

  return (
    <div className="step-limit-field">
      <label className="step-limit-label" htmlFor={inputId}>
        命令数上限
      </label>
      <input
        id={inputId}
        className="step-limit-input"
        type="text"
        inputMode="numeric"
        autoComplete="off"
        spellCheck={false}
        value={text}
        disabled={disabled || unlimited}
        aria-invalid={invalidReason !== null ? "true" : "false"}
        aria-describedby={invalidReason !== null ? errorId : undefined}
        onChange={(event) => props.onTextChange(event.target.value)}
      />
      <label className="step-limit-unlimited">
        <input
          type="checkbox"
          checked={unlimited}
          disabled={disabled}
          onChange={(event) => props.onUnlimitedChange(event.target.checked)}
        />
        上限なし
      </label>
      {invalidReason !== null ? (
        <p className="step-limit-error" id={errorId} role="alert">
          入力エラー: {REASON_MESSAGES[invalidReason]}
        </p>
      ) : null}
      {unlimited ? (
        <p className="step-limit-warning" role="status">
          ⚠ 上限なしで実行します。出力やメモリ使用量が増え続けるおそれがあります。
        </p>
      ) : null}
    </div>
  );
}
