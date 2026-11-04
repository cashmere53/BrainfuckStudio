import "./Toolbar.css";
import type { Controls } from "../domain/controls";

export interface ToolbarProps {
  /** 操作の有効/無効(`domain/controls` の `availability` の導出結果をそのまま渡す) */
  controls: Controls;
  onRun: () => void;
  onPause: () => void;
  onStep: () => void;
  onReset: () => void;
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onClearBreakpoints: () => void;
}

/**
 * 操作ツールバー(要件 6.9 / 14.1 / 14.2 / 14.3)。
 * ボタンは文字ラベルとショートカットをアクセシブルな名前に含め、色以外で操作を伝える。
 * 無効なボタンは `disabled` で表示し、クリックしても何も呼ばれない。
 */
export function Toolbar(props: ToolbarProps) {
  const { controls } = props;
  return (
    <div className="toolbar" role="toolbar" aria-label="操作">
      <div className="toolbar-group">
        <button type="button" onClick={props.onRun} disabled={!controls.run}>
          Run (F5)
        </button>
        <button type="button" onClick={props.onPause} disabled={!controls.pause}>
          Pause (F6)
        </button>
        <button type="button" onClick={props.onStep} disabled={!controls.step}>
          Step (F11)
        </button>
        <button type="button" onClick={props.onReset} disabled={!controls.reset}>
          Reset (Shift+F5)
        </button>
      </div>
      <div className="toolbar-group">
        <button type="button" onClick={props.onNew} disabled={!controls.edit}>
          新規 (Ctrl+N)
        </button>
        <button type="button" onClick={props.onOpen} disabled={!controls.edit}>
          開く (Ctrl+O)
        </button>
        <button type="button" onClick={props.onSave} disabled={!controls.edit}>
          保存 (Ctrl+S)
        </button>
        <button type="button" onClick={props.onSaveAs} disabled={!controls.edit}>
          名前を付けて保存 (Ctrl+Shift+S)
        </button>
        <button type="button" onClick={props.onClearBreakpoints} disabled={!controls.edit}>
          全ブレークポイント解除
        </button>
      </div>
    </div>
  );
}
