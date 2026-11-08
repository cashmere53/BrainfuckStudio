// @vitest-environment node
import { describe, expect, it } from "vitest";
import { ProtocolError, encodeLine, parseLine } from "./protocol.ts";

describe("encodeLine(Node → ブリッジの行)", () => {
  it("要求を 1 行の JSON にし、末尾に改行を付ける", () => {
    const line = encodeLine({ type: "request", id: 1, cmd: "e2e:ping", payload: null });
    expect(line.endsWith("\n")).toBe(true);
    expect(line.slice(0, -1)).not.toContain("\n");
    expect(JSON.parse(line)).toEqual({ type: "request", id: 1, cmd: "e2e:ping", payload: null });
  });

  it("ダイアログの返事を往復できる形にする(パスと取消の両方)", () => {
    const pathLine = encodeLine({
      type: "dialogResult",
      id: 9,
      result: { kind: "path", path: "C:\\work\\a.bf" },
    });
    expect(JSON.parse(pathLine)).toEqual({
      type: "dialogResult",
      id: 9,
      result: { kind: "path", path: "C:\\work\\a.bf" },
    });
    const cancelLine = encodeLine({ type: "dialogResult", id: 9, result: { kind: "cancel" } });
    expect(JSON.parse(cancelLine)).toEqual({
      type: "dialogResult",
      id: 9,
      result: { kind: "cancel" },
    });
  });

  it("payload が無い要求や形の違う返事は書かずに ProtocolError にする", () => {
    expect(() =>
      encodeLine({ type: "request", id: 1, cmd: "e2e:ping" } as unknown as Parameters<typeof encodeLine>[0]),
    ).toThrow(ProtocolError);
    expect(() =>
      encodeLine({ type: "dialogResult", id: 1, result: { kind: "path" } } as unknown as Parameters<typeof encodeLine>[0]),
    ).toThrow(ProtocolError);
    expect(() =>
      encodeLine({ type: "request", id: 1.5, cmd: "x", payload: null }),
    ).toThrow(ProtocolError);
  });

  it("payload が undefined の要求は、行に書かずに ProtocolError にする(JSON では値が消え、ブリッジが応答しなくなるため)", () => {
    expect(() => encodeLine({ type: "request", id: 1, cmd: "e2e:ping", payload: undefined })).toThrow(ProtocolError);
  });

  it("要求と返事は、JSON 1 つと末尾の改行 1 つだけの行にする(途中に改行は入れない)", () => {
    const request = { type: "request", id: 2, cmd: "e2e:ping", payload: { a: 1 } } as const;
    expect(encodeLine(request)).toBe(`${JSON.stringify(request)}\n`);

    const dialog = {
      type: "dialogResult",
      id: 9,
      result: { kind: "path", path: "C:\\work\\a.bf" },
    } as const;
    expect(encodeLine(dialog)).toBe(`${JSON.stringify(dialog)}\n`);
    expect(encodeLine({ type: "dialogResult", id: 9, result: { kind: "cancel" } })).toBe(
      '{"type":"dialogResult","id":9,"result":{"kind":"cancel"}}\n',
    );
  });

  it("形の違う要求や返事は ProtocolError にする(型・未知の type・result の不正・余分なキー)", () => {
    const bad = [
      { type: "request", id: 1, cmd: 5, payload: null },
      { type: "bogus", id: 1 },
      { type: "dialogResult", id: 1, result: { kind: "maybe" } },
      { type: "dialogResult", id: 1, result: { kind: "cancel", extra: 1 } },
      { type: "dialogResult", id: 1, result: { kind: "path", path: "x", extra: 1 } },
      { type: "request", id: 1, cmd: "x", payload: null, extra: 1 },
      { type: "dialogResult", id: 1, result: { kind: "cancel" }, extra: 1 },
    ];
    for (const message of bad) {
      expect(() => encodeLine(message as unknown as Parameters<typeof encodeLine>[0]), JSON.stringify(message)).toThrow(
        ProtocolError,
      );
    }
  });
});

describe("parseLine(ブリッジ → Node の行)", () => {
  it("成功の応答は value だけを持つ", () => {
    expect(parseLine('{"type":"response","id":3,"ok":true,"value":{"fileName":"a.bf"}}')).toEqual({
      type: "response",
      id: 3,
      ok: true,
      value: { fileName: "a.bf" },
    });
  });

  it("値が null の成功の応答も受け付ける", () => {
    expect(parseLine('{"type":"response","id":3,"ok":true,"value":null}')).toEqual({
      type: "response",
      id: 3,
      ok: true,
      value: null,
    });
  });

  it("失敗の応答は error だけを持つ", () => {
    expect(
      parseLine('{"type":"response","id":4,"ok":false,"error":{"kind":"io","message":"x"}}'),
    ).toEqual({ type: "response", id: 4, ok: false, error: { kind: "io", message: "x" } });
  });

  it("ダイアログの問い合わせを解釈する", () => {
    expect(parseLine('{"type":"dialog","id":9,"kind":"save"}')).toEqual({
      type: "dialog",
      id: 9,
      kind: "save",
    });
  });

  it("JSON として壊れた行、オブジェクトでない行は ProtocolError にする", () => {
    expect(() => parseLine("{not json")).toThrow(ProtocolError);
    expect(() => parseLine("null")).toThrow(ProtocolError);
    expect(() => parseLine("[1,2]")).toThrow(ProtocolError);
    expect(() => parseLine('"text"')).toThrow(ProtocolError);
  });

  it("形の違う行は ProtocolError にする(未知の type、id の欠落や不正、余分なキー)", () => {
    const bad = [
      '{"type":"pong","id":1}',
      '{"type":"response","ok":true,"value":1}',
      '{"type":"response","id":"1","ok":true,"value":1}',
      '{"type":"response","id":-1,"ok":true,"value":1}',
      '{"type":"response","id":1.5,"ok":true,"value":1}',
      '{"type":"response","id":1,"ok":true}',
      '{"type":"response","id":1,"ok":true,"value":1,"error":{}}',
      '{"type":"response","id":1,"ok":false}',
      '{"type":"response","id":1,"ok":false,"error":{},"value":1}',
      '{"type":"response","id":1,"ok":"yes","value":1}',
      '{"type":"response","id":1,"ok":true,"value":1,"extra":true}',
      '{"type":"dialog","id":9,"kind":"print"}',
      '{"type":"dialog","kind":"open"}',
      '{"type":"dialog","id":9,"kind":"open","extra":1}',
    ];
    for (const line of bad) {
      expect(() => parseLine(line), line).toThrow(ProtocolError);
    }
  });

  it("ProtocolError の文面に、違反した行の種類を示す", () => {
    expect(() => parseLine('{"type":"pong","id":1}')).toThrow(/type/);
  });
});
