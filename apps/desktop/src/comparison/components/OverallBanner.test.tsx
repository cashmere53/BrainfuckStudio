import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { OverallDto } from "../../generated/OverallDto";
import { OverallBanner } from "./OverallBanner";

function renderBanner(overall: OverallDto) {
  render(<OverallBanner overall={overall} />);
  return screen.getByRole("status");
}

describe("OverallBanner(要件 10.8)", () => {
  it("全方式一致は記号と文言で「すべて一致」を示す", () => {
    const banner = renderBanner("allMatch");
    expect(banner.textContent).toBe("✓ すべて一致");
  });

  it("不一致ありは記号と文言で「不一致あり」を示す", () => {
    const banner = renderBanner("mismatch");
    expect(banner.textContent).toBe("✗ 不一致あり");
  });

  it("未対応または失敗ありは記号と文言で示し、一致とは別の語にする", () => {
    const banner = renderBanner("incomplete");
    expect(banner.textContent).toBe("△ 未対応または失敗あり");
    expect(banner.textContent).not.toContain("すべて一致");
  });

  it("記号は支援技術に読ませず、文言だけを名前として持つ", () => {
    renderBanner("allMatch");
    const symbol = screen.getByRole("status").querySelector("[aria-hidden='true']");
    expect(symbol?.textContent).toBe("✓");
  });
});
