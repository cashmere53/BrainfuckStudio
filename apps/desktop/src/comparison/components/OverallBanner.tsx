import "./OverallBanner.css";
import type { OverallDto } from "../../generated/OverallDto";
import { formatOverall } from "../domain/comparisonFormat";

export interface OverallBannerProps {
  /** 総合結果 */
  overall: OverallDto;
}

/** 総合結果の記号。色だけに頼らず、記号と文言の両方で示す(要件 10.6 と同じ方針) */
const SYMBOLS: Record<OverallDto, string> = {
  allMatch: "✓",
  mismatch: "✗",
  incomplete: "△",
};

/**
 * 総合結果の帯(要件 10.8)。全方式一致・不一致あり・未対応または失敗あり を記号と文言で示す。
 * 状態を通知する role="status" の要素として出し、記号は支援技術に読ませない。
 */
export function OverallBanner(props: OverallBannerProps) {
  const { overall } = props;
  return (
    <div className={`overall-banner overall-banner-${overall}`} role="status">
      <span className="overall-banner-symbol" aria-hidden="true">
        {SYMBOLS[overall]}
      </span>{" "}
      {formatOverall(overall)}
    </div>
  );
}
