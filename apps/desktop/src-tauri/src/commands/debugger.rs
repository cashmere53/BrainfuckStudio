//! デバッガのコマンド(薄い層)。検証・変換は行わず `DebuggerService` へ委譲する。
//!
//! 引数は Tauri の規約どおり `args` という 1 個の構造体にまとめる(フロントは `invoke(name, { args })` で呼ぶ)。
//! `pause` はセッションのロックを取らないため、実行中の `run_batch` を追い越して即座に戻る。

use brainfuck_gui_api::dto::{
    ApiError, BatchResultDto, BreakpointsDto, LoadSourceRequest, LoadSourceResult, PauseRequest,
    RunBatchRequest, SessionStartedDto, SessionStateDto, SessionViewRequest, SetBreakpointsRequest,
    StartSessionRequest, StepResultDto, TapeWindowDto, TapeWindowRequest,
};
use tauri::State;

use crate::app_state::AppState;

/// ソースを読み込む(解析)。既存のセッションは破棄される。
#[tauri::command]
pub async fn load_source(
    args: LoadSourceRequest,
    state: State<'_, AppState>,
) -> Result<LoadSourceResult, ApiError> {
    state.service.load_source(args)
}

/// セッションを開始する。
#[tauri::command]
pub async fn start_session(
    args: StartSessionRequest,
    state: State<'_, AppState>,
) -> Result<SessionStartedDto, ApiError> {
    state.service.start_session(args)
}

/// バッチを 1 回実行する。
#[tauri::command]
pub async fn run_batch(
    args: RunBatchRequest,
    state: State<'_, AppState>,
) -> Result<BatchResultDto, ApiError> {
    state.service.run_batch(args)
}

/// 命令を 1 つ実行する(Step Into)。
#[tauri::command]
pub async fn step(
    args: SessionViewRequest,
    state: State<'_, AppState>,
) -> Result<StepResultDto, ApiError> {
    state.service.step(args)
}

/// セッションを初期状態へ戻す(ブレークポイントは保持する)。
#[tauri::command]
pub async fn reset(
    args: SessionViewRequest,
    state: State<'_, AppState>,
) -> Result<SessionStateDto, ApiError> {
    state.service.reset(args)
}

/// 現在の状態を取得する。
#[tauri::command]
pub async fn get_snapshot(
    args: SessionViewRequest,
    state: State<'_, AppState>,
) -> Result<SessionStateDto, ApiError> {
    state.service.get_snapshot(args)
}

/// テープ窓を取得する。
#[tauri::command]
pub async fn get_tape_window(
    args: TapeWindowRequest,
    state: State<'_, AppState>,
) -> Result<TapeWindowDto, ApiError> {
    state.service.get_tape_window(args)
}

/// ブレークポイントを置き換える。
#[tauri::command]
pub async fn set_breakpoints(
    args: SetBreakpointsRequest,
    state: State<'_, AppState>,
) -> Result<BreakpointsDto, ApiError> {
    state.service.set_breakpoints(args)
}

/// 実行中のバッチを一時停止する。ロックを取らず、不明・古いセッションの要求は無視する。
#[tauri::command]
pub async fn pause(args: PauseRequest, state: State<'_, AppState>) -> Result<(), ApiError> {
    state.service.pause(args);
    Ok(())
}
