//! 比較クレート(`brainfuck-compare`)。
//!
//! 素朴インタプリタ・最適化インタプリタ・WASM バックエンドの 3 方式を同一条件で実行し、
//! 完全状態の観測値を比較する。UI・標準入出力・JSON には依存しない。
//!
//! # 実行数の 2 つの定義
//!
//! - **素朴換算の実行数**(`Observation::steps_executed`): 素朴実装が同じ実行で数える命令数。
//!   3 方式の完全状態の比較と、命令数の上限はこの値で行う。
//! - **方式固有の単位**(`Units`): 最適化は IR 操作回数、WASM は単位を持たない(`Units::None`)。
//!   素朴は実行命令数を持つ。方式ごとの計測値として報告し、比較の対象には含めない。
//!
//! # 型の構成
//!
//! - `limits`: 比較の上限値(命令数・繰り返し回数・時間予算)
//! - `observation`: 完全状態の観測値、項目ごとの差分、表示用の指紋
//! - `report`: 方式・単位・失敗理由・計測値・判定・結果・総合結果

#![forbid(unsafe_code)]

pub mod limits;
pub mod methods;
mod observation;
mod report;

pub use observation::{Field, Observation};
pub use report::{
    Comparison, FailureKind, Method, MethodReport, Metrics, Outcome, Overall, Units,
    UnsupportedReason, Verdict,
};
