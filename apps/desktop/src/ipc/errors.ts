// ApiError の判別・正規化と日本語の通知文。
// 拒否値(Tauri が reject する値)を構造化エラーへ判別し、未知の形は internal に正規化する。
// 通知文は `kind` とフィールドから組み立てる。ソース・入力・パスは含めない(要件 10.9 / 13.7)。
// I/O の文面だけは OS の原因文を含む(要件 2.11)。

import type { ApiError } from "../generated/ApiError";
import type { ApiErrorKind } from "../generated/ApiErrorKind";
import type { SwitchRejectReason } from "../domain/inputBytes";
import { isClientNotice, type ClientNotice, type Notice } from "../domain/notice";

/** `ApiErrorKind` の全要素(生成型と一致させる。網羅は型で検査する) */
export const API_ERROR_KINDS = [
  "invalidArgument",
  "sessionNotFound",
  "programNotLoaded",
  "staleProgram",
  "sourceTooLarge",
  "inputTooLarge",
  "fileTooLarge",
  "io",
  "internal",
] as const satisfies readonly ApiErrorKind[];

// 生成型の全要素を列挙しているかを型で検査する(欠けると compile error)。
type MissingKind = Exclude<ApiErrorKind, (typeof API_ERROR_KINDS)[number]>;
const _allKindsListed: [MissingKind] extends [never] ? true : never = true;
void _allKindsListed;

const KIND_SET: ReadonlySet<string> = new Set<string>(API_ERROR_KINDS);

/** 未知の形の拒否値を正規化した結果の文面(固定。元の値を含めない) */
const UNEXPECTED_MESSAGE = "unexpected error response from the backend";

/** 拒否値が `ApiError` の形(既知の kind・文字列の message・string | null の field)か */
export function isApiError(value: unknown): value is ApiError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.kind === "string" &&
    KIND_SET.has(candidate.kind) &&
    typeof candidate.message === "string" &&
    (candidate.field === null || typeof candidate.field === "string")
  );
}

/**
 * 任意の拒否値を `ApiError` にする。`ApiError` の形ならそのまま、それ以外は `internal` にする。
 * 元の値の内容(文字列・Error のメッセージ・未知の kind の文面)は引き継がない。
 */
export function normalizeApiError(value: unknown): ApiError {
  if (isApiError(value)) {
    return { kind: value.kind, message: value.message, field: value.field };
  }
  return { kind: "internal", message: UNEXPECTED_MESSAGE, field: null };
}

/**
 * セッションを無効化すべき種類か(実行中・非実行中の順序は呼び出し側の責務)。
 * `sessionNotFound` は破棄済み、`staleProgram` は読み込み済みプログラムの差し替えを表す。
 */
export function isSessionInvalidating(error: ApiError): boolean {
  return error.kind === "sessionNotFound" || error.kind === "staleProgram";
}

// I/O の文面は Rust 側 `ApiError::io` が作る形: `failed to <read|write> file: <OS の文>`
const IO_MESSAGE = /^failed to (read|write) file: (.+)$/;

/**
 * backend の invalidArgument の項目名(`crates/brainfuck-gui-api/src/service.rs` の `invalid(...)` の引数)と
 * 画面に出す日本語の項目名の対応。ここに無い項目名は表示しない(Map で検索し、プロトタイプの名前を引かない)。
 */
const FIELD_LABEL_JA: ReadonlyMap<string, string> = new Map([
  ["batchSize", "バッチ幅"],
  ["tapeRadius", "テープの表示半径"],
  ["radius", "テープの表示半径"],
  ["offsets", "ブレークポイント"],
  ["breakpointOffsets", "ブレークポイント"],
  ["maxSteps", "命令数上限"],
  ["inputBase64", "入力"],
]);

/** 種類とフィールドから日本語の通知文を組み立てる */
export function apiErrorMessageJa(error: ApiError): string {
  switch (error.kind) {
    case "invalidArgument": {
      // 既知の項目だけ日本語の項目名で示す。未知の項目名は backend の内部名を出さないため示さない(要件 1.5)
      const label = error.field === null ? undefined : FIELD_LABEL_JA.get(error.field);
      return label === undefined
        ? "入力の値が不正です。内容を確認してください"
        : `入力の値が不正です(項目: ${label})。内容を確認してください`;
    }
    case "sessionNotFound":
      return "実行セッションが見つかりません。最初から実行し直してください";
    case "programNotLoaded":
      return "プログラムが読み込まれていません。ソースを読み込んでから操作してください";
    case "staleProgram":
      return "読み込んだプログラムが変わったため、実行を開始し直してください";
    case "sourceTooLarge":
      return "ソースが上限(1 MiB)を超えています";
    case "inputTooLarge":
      return "入力が上限(1 MiB)を超えています";
    case "fileTooLarge":
      return "ファイルが上限(1 MiB)を超えています";
    case "io":
      return ioMessageJa(error.message);
    case "internal":
      return "内部エラーが発生しました。もう一度操作してください";
  }
}

function ioMessageJa(message: string): string {
  const matched = IO_MESSAGE.exec(message);
  if (matched === null) {
    return "ファイルの読み書きに失敗しました";
  }
  const operation = matched[1] === "read" ? "読み込み" : "書き込み";
  return `ファイルの${operation}に失敗しました(原因: ${matched[2] ?? ""})`;
}

/**
 * 画面の通知の日本語文面(要件 10.7・3.7・6.3)。バックエンドの ApiError は `apiErrorMessageJa`、
 * UI が検出したクライアント通知は種類ごとの固定文を使う。英語の項目名は出さない(要件 1.5)。
 */
export function noticeMessageJa(notice: Notice): string {
  return isClientNotice(notice) ? clientNoticeMessageJa(notice) : apiErrorMessageJa(notice);
}

function clientNoticeMessageJa(notice: ClientNotice): string {
  switch (notice.code) {
    case "breakpointRejected":
      return "この行(カーソル以降)に命令が無いため、ブレークポイントを設定できません";
    case "inputSwitchRejected":
      return SWITCH_REJECT_MESSAGE_JA[notice.reason];
    case "lossyReplaced":
      return "ファイルに不正な UTF-8 のバイト列が含まれていたため、置換文字に置き換えて読み込みました";
  }
}

/** 入力モードの切替(16 進 → テキスト)が拒否された理由ごとの文面 */
const SWITCH_REJECT_MESSAGE_JA: Record<SwitchRejectReason, string> = {
  notUtf8: "入力が UTF-8 として正しくないため、テキストに切り替えられません",
  invalidChar: "16 進として解釈できないため(16 進数字・空白以外の文字が含まれています)、テキストに切り替えられません",
  oddDigits: "16 進として解釈できないため(16 進数字の総数が奇数です)、テキストに切り替えられません",
  tooLarge: "入力が 1 MiB を超えているため、テキストに切り替えられません",
};
