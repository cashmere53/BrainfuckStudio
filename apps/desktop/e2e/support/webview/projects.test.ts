// @vitest-environment node
// opt-in のプロジェクト定義(E2E_WEBVIEW=1 のときだけ webview 用の 2 件を返す)の単体テスト。
import { describe, expect, it } from "vitest";
import { WEBVIEW_ENV_FLAG, webviewProjects } from "./projects.ts";

describe("webviewProjects(opt-in のプロジェクト)", () => {
  it("E2E_WEBVIEW が無ければ空配列を返し、既存のブラウザのプロジェクトに影響しない", () => {
    expect(webviewProjects({})).toEqual([]);
  });

  it("E2E_WEBVIEW=1 以外の値では空配列を返す", () => {
    expect(webviewProjects({ E2E_WEBVIEW: "" })).toEqual([]);
    expect(webviewProjects({ E2E_WEBVIEW: "0" })).toEqual([]);
    expect(webviewProjects({ E2E_WEBVIEW: "true" })).toEqual([]);
  });

  it("環境変数の名前は E2E_WEBVIEW", () => {
    expect(WEBVIEW_ENV_FLAG).toBe("E2E_WEBVIEW");
  });

  it("E2E_WEBVIEW=1 のとき [webview-setup, webview] の順に返す", () => {
    const projects = webviewProjects({ E2E_WEBVIEW: "1" });
    expect(projects.map((p) => p.name)).toEqual(["webview-setup", "webview"]);
  });

  it("setup は準備の確認のファイルだけを対象にする", () => {
    const [setup] = webviewProjects({ E2E_WEBVIEW: "1" });
    expect(setup?.testDir).toBe("e2e/webview");
    expect(setup?.testMatch).toEqual(/prerequisites\.setup\.ts$/);
  });

  it("webview は .webview.spec.ts だけを対象にし、setup に依存し、@perf を除く", () => {
    const [, webview] = webviewProjects({ E2E_WEBVIEW: "1" });
    expect(webview?.testDir).toBe("e2e/webview");
    expect(webview?.testMatch).toEqual(/\.webview\.spec\.ts$/);
    expect(webview?.dependencies).toEqual(["webview-setup"]);
    expect(webview?.grepInvert).toEqual(/@perf/);
  });

  it("webview は直列(fullyParallel: false・workers: 1)で、テストの時間は 120 秒", () => {
    const [, webview] = webviewProjects({ E2E_WEBVIEW: "1" });
    expect(webview?.fullyParallel).toBe(false);
    expect(webview?.workers).toBe(1);
    expect(webview?.timeout).toBe(120_000);
  });
});
