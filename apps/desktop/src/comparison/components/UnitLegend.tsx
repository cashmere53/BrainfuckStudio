import "./UnitLegend.css";
import { STEP_COUNT_LEGEND } from "../domain/definitions";

/**
 * 実行数の凡例(要件 10.4)。素朴実装換算の実行数と方式固有の単位の 2 つの定義を、用語と説明の組で示す。
 * 文言は domain/definitions の定数をそのまま使う。
 */
export function UnitLegend() {
  return (
    <section className="unit-legend" aria-label="実行数の凡例">
      <dl>
        {STEP_COUNT_LEGEND.map((entry) => (
          <div key={entry.term} className="unit-legend-entry">
            <dt>{entry.term}</dt>
            <dd>{entry.description}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
