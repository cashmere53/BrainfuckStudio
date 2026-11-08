// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  missingLaunchPrerequisites,
  missingWebviewPrerequisites,
  type PrerequisiteFacts,
} from "./prerequisites.ts";

const READY: PrerequisiteFacts = {
  platform: "win32",
  appBinaryExists: true,
  appBinaryPath: "C:/target/debug/brainfuck-studio.exe",
  hostAvailable: true,
  webView2Version: "140.0.3485.66",
  distExists: true,
};

describe("missingWebviewPrerequisites(準備物の不足の一覧)", () => {
  it("すべて揃っていれば空の一覧(準備が整っている)", () => {
    expect(missingWebviewPrerequisites(READY)).toEqual([]);
  });

  it("Windows 以外は Windows 限定である旨の 1 行だけを返す(他の判定は行わない)", () => {
    const missing = missingWebviewPrerequisites({ ...READY, platform: "linux", appBinaryExists: false });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("Windows");
  });

  it.each(["darwin", "linux", "freebsd", "aix"] as const)(
    "Windows 以外(%s)は、他が不足していても Windows 限定の 1 行だけを返す",
    (platform) => {
      const missing = missingWebviewPrerequisites({
        ...READY,
        platform,
        appBinaryExists: false,
        hostAvailable: false,
        webView2Version: null,
        distExists: false,
      });
      expect(missing).toHaveLength(1);
      expect(missing[0]).toContain("Windows");
      expect(missing[0]).toContain(platform);
    },
  );

  it("実行ファイルが無ければ、そのパスと npm run e2e:webview:prepare の準備の案内を含む", () => {
    const missing = missingWebviewPrerequisites({ ...READY, appBinaryExists: false });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("C:/target/debug/brainfuck-studio.exe");
    expect(missing[0]).toContain("npm run e2e:webview:prepare");
  });

  it("PowerShell が使えなければ、その旨を含む(PowerShell の案内)", () => {
    const missing = missingWebviewPrerequisites({ ...READY, hostAvailable: false });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("PowerShell");
  });

  it("WebView2 が無ければ(版が null)、Evergreen の導入方法を含む", () => {
    const missing = missingWebviewPrerequisites({ ...READY, webView2Version: null });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("WebView2");
    expect(missing[0]).toContain("Evergreen");
  });

  it("画面のビルド成果物(dist)が無ければ、準備のコマンドを含む", () => {
    const missing = missingWebviewPrerequisites({ ...READY, distExists: false });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("dist");
    expect(missing[0]).toContain("npm run e2e:webview:prepare");
  });

  it("複数が不足していれば、確認の順にすべて並ぶ", () => {
    const missing = missingWebviewPrerequisites({
      ...READY,
      appBinaryExists: false,
      hostAvailable: false,
      webView2Version: null,
      distExists: false,
    });
    expect(missing).toHaveLength(4);
    expect(missing[0]).toContain("brainfuck-studio.exe");
    expect(missing[1]).toContain("PowerShell");
    expect(missing[2]).toContain("Evergreen");
    expect(missing[3]).toContain("dist");
  });
});

describe("missingLaunchPrerequisites(起動の前の判定:Windows と dist の有無だけ)", () => {
  it("Windows で dist があれば空", () => {
    expect(missingLaunchPrerequisites({ platform: "win32", distExists: true })).toEqual([]);
  });

  it("Windows 以外は Windows 限定の 1 行を返す", () => {
    const missing = missingLaunchPrerequisites({ platform: "darwin", distExists: true });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("Windows");
  });

  it.each(["darwin", "linux"] as const)(
    "Windows 以外(%s)は、dist が無くても Windows 限定の 1 行だけを返す",
    (platform) => {
      const missing = missingLaunchPrerequisites({ platform, distExists: false });
      expect(missing).toHaveLength(1);
      expect(missing[0]).toContain("Windows");
      expect(missing[0]).toContain(platform);
    },
  );

  it("dist が無ければ準備のコマンドを含む 1 行を返す(配信サーバの失敗より先に案内する)", () => {
    const missing = missingLaunchPrerequisites({ platform: "win32", distExists: false });
    expect(missing).toHaveLength(1);
    expect(missing[0]).toContain("npm run e2e:webview:prepare");
  });
});
