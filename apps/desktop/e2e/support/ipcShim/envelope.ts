// 中継の封筒を開く。成功なら value で解決し、失敗なら error の値そのもので reject する。
// ブラウザで動くシムのため、Node の型には依存せず、封筒の形だけを構造的に定義する。

/** 中継が返す封筒(bridge/protocol.ts の同名の型と同じ形) */
export type InvokeOutcome = { ok: true; value: unknown } | { ok: false; error: unknown };

/** 成功なら value、失敗なら error の値そのものを reject する(拒否値を包み直さない) */
export function unwrapOutcome(outcome: InvokeOutcome): Promise<unknown> {
  if (outcome.ok) {
    return Promise.resolve(outcome.value);
  }
  return Promise.reject(outcome.error);
}
