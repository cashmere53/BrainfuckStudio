import { describe, expect, it } from "vitest";
import { MAX_INPUT_BYTES } from "./limits";
import { byteCount, parseInput, switchMode, type InputModel } from "./inputBytes";

const text = (value: string): InputModel => ({ mode: "text", text: value });
const hex = (value: string): InputModel => ({ mode: "hex", text: value });

describe("parseInput: text mode", () => {
  it("encodes ASCII as UTF-8 without adding or converting newlines", () => {
    expect(parseInput(text("a\nb\r\n"))).toEqual({
      ok: true,
      bytes: new Uint8Array([0x61, 0x0a, 0x62, 0x0d, 0x0a]),
    });
  });

  it("encodes multi-byte characters as UTF-8", () => {
    // 「あ」= E3 81 82, 😀 = F0 9F 98 80
    expect(parseInput(text("あ😀"))).toEqual({
      ok: true,
      bytes: new Uint8Array([0xe3, 0x81, 0x82, 0xf0, 0x9f, 0x98, 0x80]),
    });
  });

  it("accepts the empty string as zero bytes", () => {
    expect(parseInput(text(""))).toEqual({ ok: true, bytes: new Uint8Array([]) });
  });

  it("keeps a leading U+FEFF as its UTF-8 bytes (no BOM stripping)", () => {
    expect(parseInput(text("﻿x"))).toEqual({
      ok: true,
      bytes: new Uint8Array([0xef, 0xbb, 0xbf, 0x78]),
    });
  });

  it("accepts exactly 1 MiB and rejects one byte more", () => {
    const exact = "a".repeat(MAX_INPUT_BYTES);
    expect(parseInput(text(exact)).ok).toBe(true);
    const over = parseInput(text(exact + "a"));
    expect(over).toEqual({ ok: false, error: { kind: "tooLarge" } });
  });

  it("measures the 1 MiB limit in UTF-8 bytes, not characters", () => {
    // 「あ」は 3 バイト。UTF-8 で 1 MiB ちょうどになる文字数の 1/3 を超えると拒否される
    const fits = "あ".repeat(Math.floor(MAX_INPUT_BYTES / 3));
    expect(parseInput(text(fits)).ok).toBe(true);
    expect(parseInput(text(fits + "あ"))).toEqual({ ok: false, error: { kind: "tooLarge" } });
  });
});

describe("parseInput: hex mode", () => {
  it("parses two-digit groups case-insensitively", () => {
    expect(parseInput(hex("0aFf 00"))).toEqual({
      ok: true,
      bytes: new Uint8Array([0x0a, 0xff, 0x00]),
    });
  });

  it("ignores spaces, tabs, and both kinds of newlines", () => {
    expect(parseInput(hex(" 4 8\t6 5\r\n6C\n6C  "))).toEqual({
      ok: true,
      bytes: new Uint8Array([0x48, 0x65, 0x6c, 0x6c]),
    });
  });

  it("treats only whitespace as ignorable: whitespace-only is zero bytes", () => {
    expect(parseInput(hex("  \t\n"))).toEqual({ ok: true, bytes: new Uint8Array([]) });
  });

  it("reports an invalid character with its UTF-16 index in the original text", () => {
    expect(parseInput(hex("41 4G"))).toEqual({
      ok: false,
      error: { kind: "invalidChar", index: 4 },
    });
  });

  it("reports full-width digits as invalid characters", () => {
    expect(parseInput(hex("４１"))).toEqual({
      ok: false,
      error: { kind: "invalidChar", index: 0 },
    });
  });

  it("reports an odd number of hex digits as oddDigits", () => {
    expect(parseInput(hex("414"))).toEqual({ ok: false, error: { kind: "oddDigits" } });
    expect(parseInput(hex("4 1 4"))).toEqual({ ok: false, error: { kind: "oddDigits" } });
  });

  it("prefers invalidChar over oddDigits when both apply", () => {
    expect(parseInput(hex("41Z"))).toEqual({
      ok: false,
      error: { kind: "invalidChar", index: 2 },
    });
  });

  it("rejects a hex string whose decoded size exceeds 1 MiB", () => {
    const fits = "00".repeat(MAX_INPUT_BYTES);
    expect(parseInput(hex(fits)).ok).toBe(true);
    expect(parseInput(hex(fits + "00"))).toEqual({ ok: false, error: { kind: "tooLarge" } });
  });

  it("accepts the bytes 0x00 and 0xFF", () => {
    expect(parseInput(hex("00 FF"))).toEqual({ ok: true, bytes: new Uint8Array([0x00, 0xff]) });
  });
});

describe("byteCount", () => {
  it("returns the total byte count for a valid model", () => {
    expect(byteCount(text("あ"))).toBe(3);
    expect(byteCount(hex("00 11 22"))).toBe(3);
  });

  it("returns null when the model cannot be parsed", () => {
    expect(byteCount(hex("0"))).toBeNull();
    expect(byteCount(hex("zz"))).toBeNull();
  });
});

describe("switchMode", () => {
  it("text to hex always succeeds and produces the same bytes", () => {
    const result = switchMode(text("Hi"), "hex");
    expect(result).toEqual({ ok: true, model: hex("48 69") });
  });

  it("text to hex keeps the bytes even when the text is over 1 MiB (the size error stays visible in hex)", () => {
    const over = "a".repeat(MAX_INPUT_BYTES + 1);
    const result = switchMode(text(over), "hex");
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.model.mode).toBe("hex");
      expect(parseInput(result.model)).toEqual({ ok: false, error: { kind: "tooLarge" } });
    }
  });

  it("text to hex of the empty string produces the empty hex string", () => {
    expect(switchMode(text(""), "hex")).toEqual({ ok: true, model: hex("") });
  });

  it("hex to text succeeds when the bytes are valid UTF-8", () => {
    expect(switchMode(hex("e3 81 82"), "text")).toEqual({ ok: true, model: text("あ") });
  });

  it("hex to text round-trips a text model through hex", () => {
    const original = text("あ😀\n");
    const toHex = switchMode(original, "hex");
    expect(toHex.ok).toBe(true);
    if (!toHex.ok) return;
    expect(switchMode(toHex.model, "text")).toEqual({ ok: true, model: original });
  });

  it("hex to text is refused when the bytes are not valid UTF-8", () => {
    expect(switchMode(hex("ff"), "text")).toEqual({ ok: false, reason: "notUtf8" });
  });

  it("hex to text is refused when the hex itself is invalid, keeping the reason", () => {
    expect(switchMode(hex("4"), "text")).toEqual({
      ok: false,
      reason: "oddDigits",
    });
    expect(switchMode(hex("4G"), "text")).toEqual({
      ok: false,
      reason: "invalidChar",
    });
  });

  it("switching to the same mode returns the model unchanged", () => {
    expect(switchMode(text("x"), "text")).toEqual({ ok: true, model: text("x") });
  });
});
