// フロントで共有する制限値。Rust 側(brainfuck-gui-api::limits)と値を一致させる。
// ts-rs は定数を出力しないため手で同期し、一致は 9.1 の契約テストで確認する。

/** ソースの上限(バイト)。1 MiB */
export const MAX_SOURCE_BYTES = 1_048_576;
/** 入力の上限(バイト)。1 MiB */
export const MAX_INPUT_BYTES = 1_048_576;
/** 出力保持の上限(末尾バイト数) */
export const OUTPUT_TAIL_LIMIT = 65_536;
/** 命令数上限の上限: 2^53 - 1 */
export const MAX_STEP_LIMIT = Number.MAX_SAFE_INTEGER;
/** 既定の命令数上限(CLI と同じ) */
export const DEFAULT_MAX_STEPS = 10_000_000;
/** テープ窓の半径の上限 */
export const MAX_TAPE_RADIUS = 64;
/** テープ窓の既定半径 */
export const DEFAULT_TAPE_RADIUS = 16;
/** バッチ幅の下限 */
export const MIN_BATCH_SIZE = 1_000;
/** バッチ幅の既定(初期値) */
export const DEFAULT_BATCH_SIZE = 1_000;
/** バッチ幅の上限 */
export const MAX_BATCH_SIZE = 1_000_000;
