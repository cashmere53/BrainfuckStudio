import "./StatusPanel.css";
import type { SessionStateDto } from "../generated/SessionStateDto";
import { deriveRunLabel, type UiState } from "../domain/runLabel";
import { displayOf } from "../state/debuggerStore";

export interface StatusPanelProps {
  /** 画面状態(セッションの有無と実行の段階)と、局所 Pause の有無。状態ラベルの導出に使う */
  ui: UiState;
  /** 最新のセッション状態 DTO。セッションが無ければ null(未開始の値を表示する) */
  state: SessionStateDto | null;
}

/**
 * 状態領域(要件 10.1・10.2・10.3・10.10・3.9)。
 * `pc`・`dp`・現在セル・実行数・入力の読み取り済み/総数を 10 進で常に表示し、
 * 状態ラベルを `aria-live="polite"` の領域に出す(頻繁な読み上げを避けるため assertive にしない)。
 * ラベルは `deriveRunLabel`、値は `displayOf` に委譲し、ここでは写すだけにする。
 */
export function StatusPanel(props: StatusPanelProps) {
  const { ui, state } = props;
  const display = displayOf(state, 0);
  const label = deriveRunLabel(ui, state);
  const steps = display.snapshot.stepsExecuted;

  return (
    <section className="status-panel" aria-label="状態">
      <p className="status-panel-label" aria-live="polite" aria-atomic="true" data-testid="status-label">
        {label}
      </p>
      {label === "正常終了" ? <p className="status-panel-final">{`最終実行数: ${steps} 命令`}</p> : null}
      <dl className="status-panel-values">
        <div className="status-panel-item">
          <dt>pc</dt>
          <dd>{display.snapshot.pc}</dd>
        </div>
        <div className="status-panel-item">
          <dt>dp</dt>
          <dd>{display.snapshot.dp}</dd>
        </div>
        <div className="status-panel-item">
          <dt>現在のセル</dt>
          <dd>{display.snapshot.currentCell}</dd>
        </div>
        <div className="status-panel-item">
          <dt>実行数</dt>
          <dd>{steps}</dd>
        </div>
        <div className="status-panel-item">
          <dt>入力の読み取り済み</dt>
          <dd>{display.inputConsumed}</dd>
        </div>
        <div className="status-panel-item">
          <dt>入力の総数</dt>
          <dd>{display.inputTotal}</dd>
        </div>
      </dl>
    </section>
  );
}
