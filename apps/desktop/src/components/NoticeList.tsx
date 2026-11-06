import "./NoticeList.css";
import type { Notice } from "../domain/notice";
import { noticeMessageJa } from "../ipc/errors";

export interface NoticeListProps {
  /** 操作の通知(ApiError または ClientNotice)。無ければ空配列(何も描画しない) */
  notices: readonly Notice[];
}

/**
 * 操作エラー・通知の表示(要件 10.7・2.11・3.7・6.3)。
 * ファイル操作・通信・内部エラーなど、プログラムの実行結果ではないエラーを `ErrorBanner` とは別の領域に出す。
 * 文面は `ipc/errors.ts` の `noticeMessageJa` が種類とフィールドから組み立てた日本語を使う(ここでは再構成しない)。
 */
export function NoticeList(props: NoticeListProps) {
  const { notices } = props;
  if (notices.length === 0) {
    return null;
  }

  return (
    <div className="notice-list" role="alert" aria-label="通知">
      <p className="notice-list-heading">操作エラー</p>
      <ul className="notice-list-items">
        {notices.map((notice, index) => (
          <li key={index}>{noticeMessageJa(notice)}</li>
        ))}
      </ul>
    </div>
  );
}
