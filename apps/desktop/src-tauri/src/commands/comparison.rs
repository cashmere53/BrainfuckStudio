//! 比較のコマンド(薄い層)。検証・変換は行わず `ComparisonService` へ委譲する。
//!
//! 比較の実行は時間がかかるため、ブロッキング用の実行基盤(`spawn_blocking`)で動かし、
//! デバッガのコマンドや画面切替を妨げない。実行基盤の失敗(パニック等)は固定文面の内部エラーへ変換し、アプリを継続させる。

use brainfuck_compare_api::dto::{ComparisonInfoDto, ComparisonResultDto, RunComparisonRequest};
use brainfuck_gui_api::dto::{ApiError, ApiErrorKind};
use tauri::State;

use crate::app_state::AppState;

/// 比較の定数(上限値・既定値・利用できる方式)を取得する。
#[tauri::command]
pub async fn get_comparison_info(
    state: State<'_, AppState>,
) -> Result<ComparisonInfoDto, ApiError> {
    Ok(state.comparison.info())
}

/// 比較を実行する。解析・比較は `ComparisonService` が行い、ブロッキング用の実行基盤で動かす。
#[tauri::command]
pub async fn run_comparison(
    args: RunComparisonRequest,
    state: State<'_, AppState>,
) -> Result<ComparisonResultDto, ApiError> {
    let service = state.comparison.clone();
    run_blocking(move || service.run(args)).await
}

/// 処理をブロッキング用の実行基盤で実行する。実行基盤の失敗(パニック等)は固定文面の内部エラーにする。
pub async fn run_blocking<T, F>(func: F) -> Result<T, ApiError>
where
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(func)
        .await
        .map_err(|_| ApiError::new(ApiErrorKind::Internal, None))?
}
