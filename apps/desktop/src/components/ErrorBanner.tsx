import "./ErrorBanner.css";
import type { ParseErrorKind } from "../generated/ParseErrorKind";
import type { PointerDirectionDto } from "../generated/PointerDirectionDto";
import type { RuntimeErrorDto } from "../generated/RuntimeErrorDto";
import type { DisplayError } from "../state/debuggerStore";

export interface ErrorBannerProps {
  /** 表示中の解析エラーまたは実行エラー。無ければ null(何も描画しない) */
  error: DisplayError | null;
}

const PARSE_KIND_LABEL: Record<ParseErrorKind, string> = {
  unmatchedClosingBracket: "対応する開き括弧がない閉じ括弧",
  unmatchedOpeningBracket: "対応する閉じ括弧がない開き括弧",
};

const DIRECTION_LABEL: Record<PointerDirectionDto, string> = { left: "左", right: "右" };

/** 表示する見出しと詳細行。ソース全文・入力バイト列・絶対パスは含めない(要件 10.9) */
function describeError(error: DisplayError): { heading: string; details: string[] } {
  if (error.category === "parse") {
    const { kind, instructionIndex, position } = error.error;
    return {
      heading: "解析エラー",
      details: [
        `種類: ${PARSE_KIND_LABEL[kind]}`,
        `命令添字: ${instructionIndex}`,
        `位置: 行 ${position.line}・列 ${position.column}`,
      ],
    };
  }
  return { heading: "実行エラー", details: describeRuntime(error.error) };
}

function describeRuntime(error: RuntimeErrorDto): string[] {
  if (error.kind === "pointerOutOfBounds") {
    return [
      "ポインタ範囲外: データポインタが範囲外へ移動しようとしました",
      `pc: ${error.pc}`,
      `dp: ${error.dp}`,
      `方向: ${DIRECTION_LABEL[error.direction]}`,
    ];
  }
  return [`命令数の上限(${error.limit} 命令)を超えました`, `pc: ${error.pc}`];
}

/**
 * 実行結果のエラー表示(要件 10.4・10.5・10.6)。解析エラーと実行エラーを見出しとテキストで区別する。
 * 操作の失敗・通知は `NoticeList` が扱い、ここには出さない(要件 10.7)。
 * `role="alert"` で、表示された時点で支援技術へ伝える。
 */
export function ErrorBanner(props: ErrorBannerProps) {
  const { error } = props;
  if (error === null) {
    return null;
  }
  const { heading, details } = describeError(error);

  return (
    <div className="error-banner" role="alert">
      <p className="error-banner-heading">{heading}</p>
      <ul className="error-banner-details">
        {details.map((detail) => (
          <li key={detail}>{detail}</li>
        ))}
      </ul>
    </div>
  );
}
