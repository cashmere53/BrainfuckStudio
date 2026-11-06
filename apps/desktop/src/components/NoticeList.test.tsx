import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ApiError } from "../generated/ApiError";
import { NoticeList } from "./NoticeList";

describe("NoticeList(要件 10.7・2.11)", () => {
  it("通知が無いときは何も描画しない", () => {
    const { container } = render(<NoticeList notices={[]} />);
    expect(container.innerHTML).toBe("");
  });

  it("操作エラーを日本語の通知文で role=alert の領域に表示する(要件 10.7)", () => {
    const notice: ApiError = { kind: "inputTooLarge", message: "input exceeds 1 MiB", field: null };
    render(<NoticeList notices={[notice]} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("操作エラー");
    expect(alert.textContent).toContain("入力が上限(1 MiB)を超えています");
  });

  it("ファイル読み込みの失敗は OS の文言を含めて表示する(要件 2.11)", () => {
    const notice: ApiError = {
      kind: "io",
      message: "failed to read file: 指定されたファイルが見つかりません。 (os error 2)",
      field: null,
    };
    render(<NoticeList notices={[notice]} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("ファイルの読み込みに失敗しました");
    expect(alert.textContent).toContain("指定されたファイルが見つかりません。 (os error 2)");
  });

  it("不正な要求の項目名を含めて表示する", () => {
    const notice: ApiError = { kind: "invalidArgument", message: "invalid", field: "maxSteps" };
    render(<NoticeList notices={[notice]} />);
    expect(screen.getByRole("alert").textContent).toContain("項目: maxSteps");
  });

  it("複数の通知を順序どおりに並べる", () => {
    const notices: ApiError[] = [
      { kind: "programNotLoaded", message: "x", field: null },
      { kind: "internal", message: "y", field: null },
    ];
    render(<NoticeList notices={notices} />);
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]?.textContent).toContain("プログラムが読み込まれていません");
    expect(items[1]?.textContent).toContain("内部エラーが発生しました");
  });
});
