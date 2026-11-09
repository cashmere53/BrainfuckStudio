import "./MethodOptions.css";
import { useId } from "react";

export interface MethodOptionsProps {
  /** 最適化インタプリタを比較に含めるか */
  optimized: boolean;
  /** WASM コンパイル実行を比較に含めるか */
  wasm: boolean;
  /** 繰り返し回数の入力欄の文字列(不正値も含む)。検証は要求の導出が担う */
  repetitionsText: string;
  /** 繰り返し回数の最大値(範囲の表示に使う) */
  maxRepetitions: number;
  /** 操作を止める(実行中・情報の未取得など)。省略時は操作可能 */
  disabled?: boolean;
  onOptimizedChange: (checked: boolean) => void;
  onWasmChange: (checked: boolean) => void;
  onRepetitionsTextChange: (text: string) => void;
}

/**
 * 比較の方式選択(最適化・WASM のチェックボックス)と繰り返し回数の入力(要件 11.9・11.10)。
 * 素朴は基準として常に実行されるため選択肢に出さず、その旨だけを示す。すべてネイティブ要素でラベルを付ける。
 */
export function MethodOptions(props: MethodOptionsProps) {
  const { optimized, wasm, repetitionsText, maxRepetitions, disabled = false } = props;
  const repetitionsId = useId();

  return (
    <fieldset className="method-options" disabled={disabled}>
      <legend className="method-options-legend">比較する方式</legend>
      <p className="method-options-note">素朴インタプリタは基準として常に実行されます</p>
      <label className="method-options-check">
        <input
          type="checkbox"
          checked={optimized}
          disabled={disabled}
          onChange={(event) => props.onOptimizedChange(event.target.checked)}
        />
        最適化インタプリタ
      </label>
      <label className="method-options-check">
        <input type="checkbox" checked={wasm} disabled={disabled} onChange={(event) => props.onWasmChange(event.target.checked)} />
        WASM コンパイル実行
      </label>
      <div className="method-options-repetitions">
        <label htmlFor={repetitionsId}>{`繰り返し回数(1〜${maxRepetitions})`}</label>
        <input
          id={repetitionsId}
          className="method-options-repetitions-input"
          type="text"
          inputMode="numeric"
          autoComplete="off"
          spellCheck={false}
          value={repetitionsText}
          disabled={disabled}
          onChange={(event) => props.onRepetitionsTextChange(event.target.value)}
        />
      </div>
    </fieldset>
  );
}
