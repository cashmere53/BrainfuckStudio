// webview のフィクスチャの後始末で、失敗の材料(スクリーンショット・出力の末尾)を残すかを決める純粋関数。
// 準備(接続・ページの特定・エディタの表示待ち・observeIpc)の例外は、テストの状態が passed のまま伝わるため、
// 状態だけでは失敗と見分けられない。そこで準備の失敗を別に受ける(要件 1.9)。

/**
 * 失敗の材料を残すか。準備で失敗したら常に残す。本体では、結果の状態が期待の状態と一致しないときに残す
 * (test.fail() で期待どおりに失敗したときは残さない)。状態が未定・タイムアウト・中断も、期待と一致しなければ残す。
 */
export function shouldCollectFailureMaterials(
  setupFailed: boolean,
  status: string | undefined,
  expectedStatus: string,
): boolean {
  if (setupFailed) {
    return true;
  }
  return status !== expectedStatus;
}

/** アプリの終了の状態(LaunchedApp.exit と同じ形。動いていれば null) */
export type AppExitState = { readonly code: number | null; readonly signal: string | null } | null;

/**
 * OS の窓口を呼んでよいかを決める(純粋関数)。アプリが終了していれば、呼ばずに理由を返す。
 * 終了した後の pid は別のプロセスに再利用されうるため、閉じる要求やキーが別のウィンドウへ届くのを仕組みとして防ぐ。
 * 動いていれば null(呼んでよい)。
 */
export function hostCallRefusal(exit: AppExitState, operation: string, pid: number): string | null {
  if (exit === null) {
    return null;
  }
  const code = exit.code ?? "なし";
  const signal = exit.signal ?? "なし";
  return `アプリ(pid ${pid})は終了しています(終了コード ${code}、シグナル ${signal})。OS の操作 ${operation} は送りません(終了後の pid は別のプロセスに再利用されうるため)`;
}
