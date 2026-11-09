import "./ParseErrorNotice.css";
import type { ParseErrorDto } from "../../generated/ParseErrorDto";
import type { ParseErrorKind } from "../../generated/ParseErrorKind";

export interface ParseErrorNoticeProps {
  /** 解析エラー(デバッガ API と同一の規則)。読み取りのみ */
  error: ParseErrorDto;
}

const KIND_LABEL: Record<ParseErrorKind, string> = {
  unmatchedClosingBracket: "対応する開き括弧がない閉じ括弧",
  unmatchedOpeningBracket: "対応する閉じ括弧がない開き括弧",
};

/**
 * 解析エラーの表示(要件 9.6・10.9)。種類・命令添字・行と列を文言で示す。
 * 解析に失敗したため方式は実行されておらず、結果の表は出さない。`role="alert"` で表示時に伝える。
 */
export function ParseErrorNotice(props: ParseErrorNoticeProps) {
  const { kind, instructionIndex, position } = props.error;
  return (
    <div className="parse-error-notice" role="alert">
      <p className="parse-error-notice-heading">解析エラー</p>
      <p>{`種類: ${KIND_LABEL[kind]}`}</p>
      <p>{`命令添字: ${instructionIndex}`}</p>
      <p>{`位置: 行 ${position.line}・列 ${position.column}`}</p>
    </div>
  );
}
