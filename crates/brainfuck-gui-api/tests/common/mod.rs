//! サービス系の統合テストで共有するヘルパ。
//!
//! - サービス生成、ソース/入力の用意、要求の組み立て
//! - コア基準の実行(`brainfuck_core::Machine::run` の結果を正解として比較する)
//!
//! バッチ反復のヘルパは `run_batch` の実装(2.5)と同時に追加する。
#![allow(dead_code)]

use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError, StopReason};
use brainfuck_gui_api::codec;
use brainfuck_gui_api::dto::{
    BatchResultDto, BatchStopDto, LoadSourceRequest, LoadSourceResult, RunBatchRequest,
    StartSessionRequest, StepLimitDto, ViewRequestDto,
};
use brainfuck_gui_api::service::DebuggerService;

/// 新しいサービスを作る。
pub fn service() -> DebuggerService {
    DebuggerService::new()
}

/// 表示窓の要求を作る。
pub fn view(tape_radius: usize, output_from: usize) -> ViewRequestDto {
    ViewRequestDto {
        tape_radius,
        output_from,
    }
}

/// 入力バイト列を要求の Base64 表記にする。
pub fn encode_input(bytes: &[u8]) -> String {
    codec::encode(bytes)
}

/// ソースを読み込み、成功したプログラム ID を返す。失敗したら panic する。
pub fn load_ok(service: &DebuggerService, source: &str) -> u64 {
    match service
        .load_source(LoadSourceRequest {
            source: source.to_owned(),
        })
        .expect("load_source accepts the source")
    {
        LoadSourceResult::Ok { program_id, .. } => program_id,
        other => panic!("expected a loaded program, got {other:?}"),
    }
}

/// 開始要求を作る。入力はバイト列から Base64 表記へ変換する。
pub fn start_request(
    program_id: u64,
    input: &[u8],
    limit: StepLimitDto,
    breakpoint_offsets: Vec<usize>,
) -> StartSessionRequest {
    StartSessionRequest {
        program_id,
        input_base64: encode_input(input),
        limit,
        breakpoint_offsets,
        view: view(16, 0),
    }
}

/// `run_batch` を `batchReached` の間反復した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRun {
    /// 最後の応答(停止理由と停止時点の状態)。
    pub last: BatchResultDto,
    /// 反復で実行した命令数の総和。
    pub executed_total: usize,
    /// `run_batch` の呼び出し回数。
    pub calls: usize,
}

/// `run_batch` を `batchReached` の間反復し、それ以外の停止理由で止める。
///
/// `max_calls` 回を超えても止まらない場合は panic する(反復が終わらない不具合の検出)。
/// バッチ幅 `batch_size` は `None`(既定)または指定値をそのまま要求に使う。
pub fn run_until_stop(
    service: &DebuggerService,
    session_id: u64,
    batch_size: Option<u64>,
    max_calls: usize,
) -> BatchRun {
    let mut executed_total = 0;
    for calls in 1..=max_calls {
        let result = service
            .run_batch(RunBatchRequest {
                session_id,
                batch_size,
                view: view(16, 0),
            })
            .expect("run_batch accepts the request");
        executed_total += result.executed;
        if result.stop != BatchStopDto::BatchReached {
            return BatchRun {
                last: result,
                executed_total,
                calls,
            };
        }
    }
    panic!("run_batch kept reaching the batch width after {max_calls} calls");
}

/// コア基準の実行結果(比較の正解)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreOutcome {
    /// 出力バイト列。
    pub output: Vec<u8>,
    /// 命令ポインタ。
    pub pc: usize,
    /// データポインタ。
    pub dp: usize,
    /// 実行した命令数。
    pub steps: u64,
    /// テープ(機械が確保した範囲)。
    pub tape: Vec<u8>,
    /// 完走の結果(正常終了、または実行エラー)。
    pub result: Result<StopReason, RuntimeError>,
}

/// `brainfuck_core::Machine::run` でソースを完走させた結果を返す。
///
/// `max_steps` が `None` のときは命令数の上限を設けない。ソースは解析に成功する必要がある。
pub fn run_core(source: &str, input: &[u8], max_steps: Option<u64>) -> CoreOutcome {
    let program = Program::parse(source).expect("core oracle needs a valid program");
    let config = match max_steps {
        Some(limit) => MachineConfig::with_max_steps(limit),
        None => MachineConfig::unlimited(),
    };
    let mut machine = Machine::new(program, input.to_vec(), config);
    let result = machine.run();
    let snapshot = machine.snapshot();
    CoreOutcome {
        output: machine.output().to_vec(),
        pc: snapshot.pc,
        dp: snapshot.data_pointer,
        steps: snapshot.steps_executed,
        tape: machine.tape().to_vec(),
        result,
    }
}
