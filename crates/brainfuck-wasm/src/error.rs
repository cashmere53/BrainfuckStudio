//! エラー型。すべて列挙値のみで、ソース・入力・パスを含めない。

/// 生成物の方針違反の種類。違反があれば実行せずに拒否する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyViolation {
    /// import 集合が `{bf.read: () -> i32, bf.write: (i32) -> ()}` と厳密一致しない。
    ImportSet,
    /// メモリ宣言が 1 つ・最小 = 最大 = 1 ページ・非共有・32 ビットでない。
    Memory,
    /// テーブルが存在する。
    Table,
    /// グローバルの定義・import・export が存在する。
    Global,
    /// start 関数が存在する。
    Start,
    /// export 集合が `{run, memory}` と厳密一致しない。
    ExportSet,
    /// 関数本体に `memory.grow` が存在する。
    MemoryGrow,
    /// 関数本体に間接呼び出し(`call_indirect`)が存在する。
    IndirectCall,
    /// 関数本体に浮動小数点命令が存在する。
    FloatingPoint,
}

/// コンパイル(生成・監査・ロード)の失敗。
///
/// `ModuleTooLarge` / `NestingTooDeep` は未対応(規模・入れ子の超過)、`PolicyViolation` / `Engine` は方式の失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompileError {
    /// モジュールが `MAX_MODULE_BYTES` を超える(未対応)。
    ModuleTooLarge,
    /// ループの入れ子が `MAX_LOOP_DEPTH` を超える(未対応)。
    NestingTooDeep,
    /// 生成物が方針に違反する(方式の失敗)。
    PolicyViolation(PolicyViolation),
    /// 実行エンジンがモジュールを受け付けない(方式の失敗)。
    Engine,
}

/// 実行の失敗(素朴意味論のエラーとは別の、方式の側の失敗)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunError {
    /// 燃料が枯渇した(実行量の監視違反)。
    WatchdogTripped,
    /// 出力量の上限を超えた。
    OutputLimitExceeded,
    /// 再生の前提を満たさない途中状態(生成コードまたは取り出し処理の不整合)。
    InvalidState,
    /// 実行エンジンの内部失敗。
    Engine,
}
