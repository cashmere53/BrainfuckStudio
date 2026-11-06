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

describe("NoticeList: クライアント通知(要件 3.7・6.3)", () => {
  it("ブレークポイントの設定不可を、理由つきの日本語で role=alert の領域に表示する", () => {
    render(<NoticeList notices={[{ kind: "client", code: "breakpointRejected" }]} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("この行(カーソル以降)に命令が無いため、ブレークポイントを設定できません");
    expect(alert.textContent).not.toContain("項目:");
  });

  it("入力モード切替の拒否の理由を表示する(16 進 → テキストの UTF-8 不正)", () => {
    render(<NoticeList notices={[{ kind: "client", code: "inputSwitchRejected", reason: "notUtf8" }]} />);
    expect(screen.getByRole("alert").textContent).toContain("入力が UTF-8 として正しくないため、テキストに切り替えられません");
  });

  it("ApiError とクライアント通知を混在させても順序どおりに並べる", () => {
    render(
      <NoticeList
        notices={[
          { kind: "client", code: "inputSwitchRejected", reason: "oddDigits" },
          { kind: "programNotLoaded", message: "x", field: null },
        ]}
      />,
    );
    const items = screen.getAllByRole("listitem");
    expect(items[0]?.textContent).toContain("16 進数字の総数が奇数です");
    expect(items[1]?.textContent).toContain("プログラムが読み込まれていません");
  });
});

describe("NoticeList: 重要度による見出し(要件 2.12)", () => {
  it("成功・情報の通知(置換読み込み)は見出しを「通知」にし、操作エラーと見せない", () => {
    render(<NoticeList notices={[{ kind: "client", code: "lossyReplaced" }]} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("通知");
    expect(alert.textContent).not.toContain("操作エラー");
    expect(alert.textContent).toContain("置換文字");
  });
});
