import "./NoticeList.css";
import { noticeSeverityOf, type Notice } from "../domain/notice";
import { noticeMessageJa } from "../ipc/errors";

export interface NoticeListProps {
  /** 操作の通知(ApiError または ClientNotice)。無ければ空配列(何も描画しない) */
  notices: readonly Notice[];
}

/**
 * 操作エラー・通知の表示(要件 10.7・2.11・2.12・3.7・6.3)。
 * ファイル操作・通信・内部エラーなど、プログラムの実行結果ではないエラーを `ErrorBanner` とは別の領域に出す。
 * 文面は `ipc/errors.ts` の `noticeMessageJa` が種類とフィールドから組み立てた日本語を使う(ここでは再構成しない)。
 * 見出しは重要度で分ける: 失敗が含まれれば「操作エラー」、成功・情報だけなら「通知」(成功を失敗と誤認させない)。
 */
export function NoticeList(props: NoticeListProps) {
  const { notices } = props;
  if (notices.length === 0) {
    return null;
  }

  const heading = notices.every((notice) => noticeSeverityOf(notice) === "info") ? "通知" : "操作エラー";
  return (
    <div className="notice-list" role="alert" aria-label="通知">
      <p className="notice-list-heading">{heading}</p>
      <ul className="notice-list-items">
        {notices.map((notice, index) => (
          <li key={index}>{noticeMessageJa(notice)}</li>
        ))}
      </ul>
    </div>
  );
}
