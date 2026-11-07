import { describe, expect, it } from "vitest";
import { decodeBase64, encodeBase64 } from "./base64";

describe("encodeBase64", () => {
  it("encodes an empty array to an empty string", () => {
    expect(encodeBase64(new Uint8Array([]))).toBe("");
  });

  it("uses the standard alphabet with padding", () => {
    expect(encodeBase64(new Uint8Array([0x48]))).toBe("SA==");
    expect(encodeBase64(new Uint8Array([0x48, 0x69]))).toBe("SGk=");
    expect(encodeBase64(new Uint8Array([0x48, 0x69, 0x21]))).toBe("SGkh");
  });

  it("keeps 0x00 and 0xFF bytes intact", () => {
    expect(encodeBase64(new Uint8Array([0x00, 0xff]))).toBe("AP8=");
  });

  it("handles inputs far larger than the argument-spread limit (1 MiB)", () => {
    const big = new Uint8Array(1_048_576);
    for (let i = 0; i < big.length; i++) big[i] = i & 0xff;
    const encoded = encodeBase64(big);
    expect(encoded.length).toBe(Math.ceil(1_048_576 / 3) * 4);
    const decoded = decodeBase64(encoded);
    expect(decoded.ok).toBe(true);
    if (decoded.ok) expect(decoded.bytes).toEqual(big);
  });
});

describe("decodeBase64", () => {
  it("round-trips arbitrary bytes", () => {
    const original = new Uint8Array([0, 1, 2, 127, 128, 254, 255]);
    const decoded = decodeBase64(encodeBase64(original));
    expect(decoded).toEqual({ ok: true, bytes: original });
  });

  it("accepts an empty string as zero bytes", () => {
    expect(decodeBase64("")).toEqual({ ok: true, bytes: new Uint8Array([]) });
  });

  it("rejects characters outside the standard alphabet", () => {
    expect(decodeBase64("SGk*").ok).toBe(false);
    expect(decodeBase64("SG k=").ok).toBe(false);
    expect(decodeBase64("SGk-").ok).toBe(false);
  });

  it("rejects a length that is not a multiple of four", () => {
    expect(decodeBase64("SGk").ok).toBe(false);
  });

  it("rejects misplaced or excessive padding", () => {
    expect(decodeBase64("S===").ok).toBe(false);
    expect(decodeBase64("SA=A").ok).toBe(false);
  });
});
