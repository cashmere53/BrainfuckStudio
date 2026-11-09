import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { STEP_COUNT_LEGEND } from "../domain/definitions";
import { UnitLegend } from "./UnitLegend";

describe("UnitLegend(要件 10.4)", () => {
  it("実行数の 2 つの定義を凡例として表示する", () => {
    render(<UnitLegend />);
    const legend = screen.getByRole("region", { name: "実行数の凡例" });
    for (const entry of STEP_COUNT_LEGEND) {
      expect(legend.textContent).toContain(entry.term);
      expect(legend.textContent).toContain(entry.description);
    }
  });

  it("素朴換算の実行数と方式固有の単位を、用語と説明の組(dt と dd)で示す", () => {
    render(<UnitLegend />);
    const terms = Array.from(screen.getByRole("region", { name: "実行数の凡例" }).querySelectorAll("dt")).map(
      (dt) => dt.textContent,
    );
    expect(terms).toEqual(["素朴実装換算の実行数", "方式固有の単位"]);
  });

  it("WASM は方式固有の単位を数えないことを明記する", () => {
    render(<UnitLegend />);
    expect(screen.getByRole("region", { name: "実行数の凡例" }).textContent).toContain("WASM = 数えない");
  });
});
