//! `DebuggerService` 経由の結果一致テスト(9.2): `brainfuck_core` の `Machine` を正解とする。
//!
//! 確認する内容: コーパス(下記)を、バッチ幅の違う反復・Pause と再開・ブレークポイント停止と再開・
//! Step の反復・バッチと Step の混在の各分割で実行し、出力・テープ窓・`pc`・`dp`・実行数・
//! 入力の読み取り位置・停止の種類(正常終了 / 実行エラーの種類とフィールド)が core と完全に一致すること。
//! ブレークポイントで止まった各地点(`pc`・実行数・入力の読み取り位置)は、core を 1 命令ずつ進めて
//! 得た期待値と照合する(停止回数も含む)。出力は、各応答の `dataStart` からの差分を
//! 3.2 のフロントと同じ規則(保持末尾と連続なら連結、そうでなければ置換)で積み上げ、core の出力と比べる。
//! 解析エラー(括弧の不整合)は `loadSource` の結果を core の `Program::parse` の結果と比べる
//! (行・列・UTF-16 オフセットも、ソースの接頭辞から独立に数えた値と照合する)。
//! 加えて、Pause で `cancelled` になった後の Step・リセット・ブレークポイント置換・再開を
//! 1 本の操作列として確認する。入力を消費した後のリセットと再実行も確認する。
//!
//! テープは表示窓(最大 ±64、`MAX_TAPE_RADIUS`)のみ比較する(全体は比較しない)。理由: API が契約として
//! 返すのは表示窓の内側だけであり、窓の外の値は応答に含まれないため。ただしコーパスのどのプログラムも
//! 窓の外に書き込まないので、core の窓の外のテープはすべて 0 であることを確認する(書き込みを含む
//! プログラムをコーパスに足した場合は、このアサートが失敗して見直しを促す)。
//!
//! コーパスに含めない入力(文書化):
//! 1. 命令数上限 0: GUI は上限 1 以上の値だけを受け付けるが、CLI の `--max-steps 0` は受け付ける。
//!    そのため GUI の経路では表現できず、この正解比較の対象外とする。
//! 2. 非 UTF-8 ファイル: GUI はファイルを置換して読み込むため、`ParseError` のバイトオフセットが
//!    CLI が実ファイルを読んだときの値と異なりうる。ここではソースを常に UTF-8 文字列として与える。
//! 3. 解析エラー 2 種(括弧の不整合)の 2 種はコーパスに入れない: 解析に失敗したソースはセッションを
//!    作れず、`Machine::run` の結果(停止・実行数・出力)を持たないため、実行の分割を比べる対象にならない。
//!    代わりに `parse_errors_match_the_core_kind_index_and_position` で `loadSource` の結果を比べる。
//!
//! Pause は実行中のバッチにだけ効き、`run_batch` の入口で旗は下ろされる。そのため止まる位置は
//! 実行ごとに変わりうる。本テストは止まった位置によらず最終結果が一致することを確認し、
//! 止まった回数は `cancelled` として数えるだけで、回数そのものには依存しない。
//!
//! 別スレッドは本テストコード内でのみ使う(本番コードではスレッドを生成しない)。

mod common;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use brainfuck_core::{ParseError, PointerDirection, Program, RuntimeError, StopReason};
use brainfuck_gui_api::codec;
use brainfuck_gui_api::dto::{
    BatchResultDto, BatchStopDto, LoadSourceRequest, LoadSourceResult, OutputChunkDto,
    ParseErrorKind, PauseRequest, PointerDirectionDto, RunBatchRequest, RuntimeErrorDto,
    SessionStateDto, SessionViewRequest, SetBreakpointsRequest, StepLimitDto, StepOutcomeDto,
    StepResultDto, TapeWindowRequest,
};
use brainfuck_gui_api::limits::{MAX_INPUT_BYTES, MAX_TAPE_RADIUS};
use brainfuck_gui_api::service::DebuggerService;

use common::{core_machine, load_ok, run_core, service, start_request, view, CoreOutcome};

/// 解析可能な実例(コメントを含む)。
const HELLO: &str = include_str!("../../../examples/hello.bf");

/// 数万命令の入れ子ループ(終了する)。Pause と Step の検査に使う長さ。
const LONG_COUNTER: &str = "++++++++++[>++++++++++[>++++++++++[>++++++++++[>+<-]<-]<-]<-]>.";

/// 反復が終わらないことを防ぐ呼び出し回数の上限。
const CALL_BUDGET: usize = 5_000_000;

/// Pause を送る間隔。連続して送ると、バッチの入口で旗が立ち続けて 1 命令も進まなくなる
/// (`CancelSignal` は各命令の前に問い合わせられるため)。間隔を空けて進行を保証する。
const PAUSE_INTERVAL: Duration = Duration::from_millis(1);

/// コーパスの 1 件。
#[derive(Debug, Clone)]
struct Case {
    name: &'static str,
    source: String,
    input: Vec<u8>,
    limit: StepLimitDto,
}

fn unlimited() -> StepLimitDto {
    StepLimitDto::Unlimited
}

fn case(name: &'static str, source: &str, input: &[u8], limit: StepLimitDto) -> Case {
    Case {
        name,
        source: source.to_owned(),
        input: input.to_vec(),
        limit,
    }
}

/// 正常終了・実行エラー・命令数上限・空ソース・多バイトと CRLF を含むコーパス。
fn corpus() -> Vec<Case> {
    vec![
        case("hello", HELLO, b"", unlimited()),
        case("echo_raw_bytes", ",[.,]", &[0x41, 0xFF, 0x80], unlimited()),
        case("zero_and_ff_output", ",.,.,.", &[0x00, 0xFF], unlimited()),
        case("input_exhausted", ",.,.", b"A", unlimited()),
        case("nested_loops", "+++[>+++[>+++<-]<-]>>.", b"", unlimited()),
        case("pointer_left_out_of_range", "+.<", b"", unlimited()),
        case(
            "pointer_right_out_of_range",
            &">".repeat(30_000),
            b"",
            unlimited(),
        ),
        case(
            "step_limit_straight_line",
            "+++++",
            b"",
            StepLimitDto::Limited { max_steps: 3 },
        ),
        case(
            "step_limit_infinite_loop",
            "+[]",
            b"",
            StepLimitDto::Limited { max_steps: 5_000 },
        ),
        case(
            "multibyte_comment_crlf_surrogate",
            "あ 😀 comment\r\n+++++\r\n[>++<-]\r\n>+++.\r\n",
            b"",
            unlimited(),
        ),
        case("long_nested_counter", LONG_COUNTER, b"", unlimited()),
        case("empty_program", "", b"", unlimited()),
    ]
}

/// 分割の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    /// 一定幅のバッチを終端まで反復する。
    Batch { width: u64 },
    /// 一定幅のバッチを反復し、その間は別スレッドから Pause を送り続ける(止まれば再開する)。
    PauseAndResume { width: u64 },
    /// 命令のうち `every` 個おきにブレークポイントを置き、停止のたびに再開する。
    Breakpoints { every: usize },
    /// Step だけで 1 命令ずつ進める。
    Steps,
    /// バッチ(幅 `width`)と Step を交互に行う。
    Mixed { width: u64 },
}

fn strategies() -> Vec<Strategy> {
    vec![
        Strategy::Batch { width: 1 },
        Strategy::Batch { width: 2 },
        Strategy::Batch { width: 7 },
        Strategy::Batch { width: 1_000 },
        Strategy::Batch { width: 1_000_000 },
        Strategy::PauseAndResume { width: 1 },
        Strategy::PauseAndResume { width: 1_000 },
        Strategy::Breakpoints { every: 1 },
        Strategy::Breakpoints { every: 2 },
        Strategy::Steps,
        Strategy::Mixed { width: 3 },
    ]
}

/// ブレークポイントで止まった地点(`pc`・実行済み命令数・読み取り済み入力バイト数)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StopPoint {
    pc: usize,
    steps: u64,
    input_consumed: usize,
}

/// 終端に達した結果。`result` は正常終了なら `Ok(())`、実行エラーなら `Err`。
#[derive(Debug)]
struct Finished {
    state: SessionStateDto,
    result: Result<(), RuntimeErrorDto>,
    cancelled: usize,
    /// ブレークポイントで止まった地点を、止まった順に並べたもの。
    breakpoint_stops: Vec<StopPoint>,
}

/// 命令文字の UTF-16 オフセットを順に返す。
///
/// コーパスのコメントには命令文字を含めない(含めると命令数が変わるため)。
fn command_offsets(source: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut utf16 = 0;
    for ch in source.chars() {
        if "+-<>[].,".contains(ch) {
            offsets.push(utf16);
        }
        utf16 += ch.len_utf16();
    }
    offsets
}

/// 命令添字の集合。`every` 個おきに命令を選ぶ(0 番目から始める)。
fn breakpoint_indices(source: &str, every: usize) -> Vec<usize> {
    (0..command_offsets(source).len()).step_by(every).collect()
}

fn core_limit(limit: StepLimitDto) -> Option<u64> {
    match limit {
        StepLimitDto::Limited { max_steps } => Some(max_steps),
        StepLimitDto::Unlimited => None,
    }
}

/// core を 1 命令ずつ進め、命令添字の集合 `breakpoints` に当たるたびの停止地点を返す(正解の停止列)。
///
/// 停止の規則は core の `run_batch` と同じ: 試行ごとに、直前の停止で止まった命令でなければ現在の
/// `pc` を照合し、一致したら止まる。止まった命令は必ず 1 回実行されるため、試行のたびに 1 回止まる
/// (ループ内なら反復ごとに止まる)。`step` がエラーを返した試行は、その直前の照合までを数えて終わる。
fn core_breakpoint_stops(
    source: &str,
    input: &[u8],
    max_steps: Option<u64>,
    breakpoints: &BTreeSet<usize>,
) -> Vec<StopPoint> {
    let mut machine = core_machine(source, input, max_steps);
    let mut stops = Vec::new();
    while !machine.is_halted() {
        let snapshot = machine.snapshot();
        if breakpoints.contains(&snapshot.pc) {
            stops.push(StopPoint {
                pc: snapshot.pc,
                steps: snapshot.steps_executed,
                input_consumed: machine.input_position(),
            });
        }
        if machine.step().is_err() {
            break;
        }
    }
    stops
}

fn core_error_dto(error: &RuntimeError) -> RuntimeErrorDto {
    match *error {
        RuntimeError::PointerOutOfBounds { pc, dp, direction } => {
            RuntimeErrorDto::PointerOutOfBounds {
                pc,
                dp,
                direction: match direction {
                    PointerDirection::Left => PointerDirectionDto::Left,
                    PointerDirection::Right => PointerDirectionDto::Right,
                },
            }
        }
        RuntimeError::StepLimitExceeded { limit, pc } => {
            RuntimeErrorDto::StepLimitExceeded { limit, pc }
        }
        // 上流の列挙は非網羅(`non_exhaustive`)。未知の種類は黙って写さず失敗させる。
        ref other => panic!("unknown runtime error from the core: {other:?}"),
    }
}

fn decode(text: &str) -> Vec<u8> {
    codec::decode(text, MAX_INPUT_BYTES).expect("output is valid base64")
}

/// スコープを抜けるとき(正常・panic の両方)に旗を立てるガード。
/// 別スレッドはこの旗で止まり、スコープの終わりで結合される。
struct StopOnDrop<'a>(&'a AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn run_batch(service: &DebuggerService, session_id: u64, width: Option<u64>) -> BatchResultDto {
    service
        .run_batch(RunBatchRequest {
            session_id,
            batch_size: width,
            view: view(16, 0),
        })
        .expect("run_batch accepts the request")
}

fn step(service: &DebuggerService, session_id: u64) -> StepResultDto {
    service
        .step(SessionViewRequest {
            session_id,
            view: view(16, 0),
        })
        .expect("step accepts the request")
}

fn stop_point(state: &SessionStateDto) -> StopPoint {
    StopPoint {
        pc: state.snapshot.pc,
        steps: state.snapshot.steps_executed,
        input_consumed: state.input_consumed,
    }
}

/// `run_batch` を終端まで反復する。`cancelled`・`breakpoint` の停止は数えて続ける。
fn batches_to_end(service: &DebuggerService, session_id: u64, width: Option<u64>) -> Finished {
    let mut cancelled = 0;
    let mut breakpoint_stops = Vec::new();
    for _ in 0..CALL_BUDGET {
        let BatchResultDto { stop, state, .. } = run_batch(service, session_id, width);
        let result = match stop {
            BatchStopDto::Halted => Ok(()),
            BatchStopDto::Error { error } => Err(error),
            BatchStopDto::Cancelled => {
                cancelled += 1;
                continue;
            }
            BatchStopDto::Breakpoint => {
                breakpoint_stops.push(stop_point(&state));
                continue;
            }
            BatchStopDto::BatchReached => continue,
        };
        return Finished {
            state,
            result,
            cancelled,
            breakpoint_stops,
        };
    }
    panic!("the run did not finish within {CALL_BUDGET} calls");
}

/// バッチを反復し、その間は別スレッドから Pause を送り続ける。
fn batches_with_pauses(service: &DebuggerService, session_id: u64, width: u64) -> Finished {
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        let _stop = StopOnDrop(&done);
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                service.pause(PauseRequest { session_id });
                thread::sleep(PAUSE_INTERVAL);
            }
        });
        batches_to_end(service, session_id, Some(width))
    })
}

/// Step だけで終端まで進める。
fn steps_to_end(service: &DebuggerService, session_id: u64) -> Finished {
    for _ in 0..CALL_BUDGET {
        let StepResultDto { outcome, state } = step(service, session_id);
        let result = match outcome {
            StepOutcomeDto::Executed => continue,
            StepOutcomeDto::Halted => Ok(()),
            StepOutcomeDto::Error { error } => Err(error),
        };
        return Finished {
            state,
            result,
            cancelled: 0,
            breakpoint_stops: Vec::new(),
        };
    }
    panic!("the step run did not finish within {CALL_BUDGET} calls");
}

/// バッチ(幅 `width`)と Step を交互に行い、終端まで進める。
fn mixed_to_end(service: &DebuggerService, session_id: u64, width: u64) -> Finished {
    for _ in 0..CALL_BUDGET {
        let BatchResultDto { stop, state, .. } = run_batch(service, session_id, Some(width));
        match stop {
            BatchStopDto::Halted => {
                return Finished {
                    state,
                    result: Ok(()),
                    cancelled: 0,
                    breakpoint_stops: Vec::new(),
                }
            }
            BatchStopDto::Error { error } => {
                return Finished {
                    state,
                    result: Err(error),
                    cancelled: 0,
                    breakpoint_stops: Vec::new(),
                }
            }
            _ => {}
        }
        let StepResultDto { outcome, state } = step(service, session_id);
        let result = match outcome {
            StepOutcomeDto::Executed => continue,
            StepOutcomeDto::Halted => Ok(()),
            StepOutcomeDto::Error { error } => Err(error),
        };
        return Finished {
            state,
            result,
            cancelled: 0,
            breakpoint_stops: Vec::new(),
        };
    }
    panic!("the mixed run did not finish within {CALL_BUDGET} calls");
}

/// 1 件のプログラムを、指定の分割で終端まで実行する。
fn drive(service: &DebuggerService, case: &Case, strategy: Strategy) -> Finished {
    let program = load_ok(service, &case.source);
    let offsets = match strategy {
        Strategy::Breakpoints { every } => {
            let indices = breakpoint_indices(&case.source, every);
            let offsets = command_offsets(&case.source);
            indices.into_iter().map(|index| offsets[index]).collect()
        }
        _ => Vec::new(),
    };
    let started = service
        .start_session(start_request(program, &case.input, case.limit, offsets))
        .expect("session starts");
    let session_id = started.state.session_id;

    if let Strategy::Breakpoints { every } = strategy {
        // 置いた位置が命令添字へ正しく解決されていること(UTF-16 オフセット経由)。
        let resolved: Vec<usize> = started
            .breakpoints
            .breakpoints
            .iter()
            .map(|bp| bp.instruction_index)
            .collect();
        assert_eq!(
            resolved,
            breakpoint_indices(&case.source, every),
            "{}: breakpoint resolution",
            case.name
        );
    }

    match strategy {
        Strategy::Batch { width } => batches_to_end(service, session_id, Some(width)),
        Strategy::PauseAndResume { width } => batches_with_pauses(service, session_id, width),
        Strategy::Breakpoints { .. } => batches_to_end(service, session_id, None),
        Strategy::Steps => steps_to_end(service, session_id),
        Strategy::Mixed { width } => mixed_to_end(service, session_id, width),
    }
}

/// 終端の状態が `brainfuck_core` の結果と一致することを確認する。
fn assert_matches_core(
    label: &str,
    service: &DebuggerService,
    finished: &Finished,
    core: &CoreOutcome,
    input: &[u8],
) {
    let snapshot = finished.state.snapshot;
    assert_eq!(snapshot.pc, core.pc, "{label}: pc");
    assert_eq!(snapshot.dp, core.dp, "{label}: dp");
    assert_eq!(snapshot.steps_executed, core.steps, "{label}: steps");
    assert_eq!(
        snapshot.halted,
        matches!(core.result, Ok(StopReason::Halted)),
        "{label}: halted"
    );
    let expected_result = match &core.result {
        Ok(StopReason::Halted) => Ok(()),
        Ok(other) => panic!("{label}: Machine::run returned {other:?}"),
        Err(error) => Err(core_error_dto(error)),
    };
    assert_eq!(finished.result, expected_result, "{label}: stop");

    // 出力: コーパスの出力は 64 KiB 未満なので、先頭から全体が載る。
    assert_eq!(finished.state.output.data_start, 0, "{label}: output start");
    assert_eq!(
        finished.state.output.total_len,
        core.output.len(),
        "{label}: output length"
    );
    assert_eq!(
        decode(&finished.state.output.data_base64),
        core.output,
        "{label}: output bytes"
    );

    // 入力: 総数と、読み取り済みの位置(core の `input_position`)。
    assert_eq!(
        finished.state.input_total,
        input.len(),
        "{label}: input total"
    );
    assert_eq!(
        finished.state.input_consumed, core.input_consumed,
        "{label}: input consumed"
    );
    assert_eq!(
        snapshot.current_cell,
        core.tape.get(core.dp).copied().unwrap_or(0),
        "{label}: current cell"
    );

    // テープ: 表示窓の各セルは core のテープの同じ位置と一致し、窓内の dp は core の dp を指す。
    let window = service
        .get_tape_window(TapeWindowRequest {
            session_id: finished.state.session_id,
            radius: MAX_TAPE_RADIUS,
        })
        .expect("the radius is within the limit");
    assert_eq!(
        window.start + window.pointer_offset,
        core.dp,
        "{label}: dp window"
    );
    for (offset, &cell) in window.cells.iter().enumerate() {
        let position = window.start + offset;
        assert_eq!(
            cell,
            core.tape.get(position).copied().unwrap_or(0),
            "{label}: tape cell {position}"
        );
    }
    // 窓の外は比較しない。コーパスは窓の外に書き込まないため、core の窓の外は 0 のはず。
    let shown = window.start..window.start + window.cells.len();
    let written_outside = core
        .tape
        .iter()
        .enumerate()
        .any(|(position, &cell)| cell != 0 && !shown.contains(&position));
    assert!(
        !written_outside,
        "{label}: the core wrote outside the displayed window (the corpus must not do that)"
    );
}

#[test]
fn every_corpus_program_matches_the_core_under_every_split() {
    for case in corpus() {
        let limit = core_limit(case.limit);
        let core = run_core(&case.source, &case.input, limit);
        for strategy in strategies() {
            let service = service();
            let finished = drive(&service, &case, strategy);
            let label = format!("{} / {strategy:?}", case.name);
            assert_matches_core(&label, &service, &finished, &core, &case.input);
            if let Strategy::Breakpoints { every } = strategy {
                let breakpoints: BTreeSet<usize> = breakpoint_indices(&case.source, every)
                    .into_iter()
                    .collect();
                let expected =
                    core_breakpoint_stops(&case.source, &case.input, limit, &breakpoints);
                assert_eq!(
                    finished.breakpoint_stops, expected,
                    "{label}: breakpoint stops (pc, executed steps, input consumed)"
                );
            }
        }
    }
}

#[test]
fn hello_world_reaches_the_documented_final_state_under_every_split() {
    for strategy in strategies() {
        let service = service();
        let case = case("hello", HELLO, b"", unlimited());
        let finished = drive(&service, &case, strategy);
        let label = format!("hello / {strategy:?}");
        assert_eq!(finished.result, Ok(()), "{label}: halts normally");
        assert_eq!(
            decode(&finished.state.output.data_base64),
            b"Hello World!\n".to_vec(),
            "{label}: output"
        );
        assert_eq!(finished.state.snapshot.pc, 106, "{label}: pc");
        assert_eq!(finished.state.snapshot.dp, 6, "{label}: dp");
        assert_eq!(
            finished.state.snapshot.steps_executed, 906,
            "{label}: steps"
        );
    }
}

#[test]
fn breakpoint_and_pause_splits_actually_stop_the_run() {
    // 1 命令ごとのブレークポイントは、実行される試行ごとに 1 回止まる(hello は 906 命令)。
    // 止まった k 回目の実行済み命令数は k − 1 である。
    let hello_service = service();
    let hello = case("hello", HELLO, b"", unlimited());
    let breakpoints = drive(&hello_service, &hello, Strategy::Breakpoints { every: 1 });
    assert_eq!(
        breakpoints.breakpoint_stops.len(),
        906,
        "every instruction is a breakpoint: one stop per executed instruction"
    );
    for (index, stop) in breakpoints.breakpoint_stops.iter().enumerate() {
        assert_eq!(stop.steps, index as u64, "stop {index}: executed steps");
    }

    // Pause で止まることを確かめる(分割が無視されていないこと)。
    // Pause が width-1 バッチの実行中に届くかは実行時間の競合で決まるため、届くまで実行を繰り返す
    // (上限つき)。届いた回では、止まった後の再開も含めて core の結果と一致することを確かめる。
    let long = case("long", LONG_COUNTER, b"", unlimited());
    let core = run_core(LONG_COUNTER, b"", None);
    let mut observed = false;
    for _ in 0..50 {
        let long_service = service();
        let paused = drive(&long_service, &long, Strategy::PauseAndResume { width: 1 });
        if paused.cancelled > 0 {
            assert_matches_core("long / PauseAndResume", &long_service, &paused, &core, b"");
            observed = true;
            break;
        }
    }
    assert!(
        observed,
        "a pause sent during width-1 batches must stop at least one batch"
    );
}

/// 出力の差分を受け取る側の保持(フロントの 3.2 `applyChunk` と同じ規則)。
#[derive(Debug, Default)]
struct OutputBuffer {
    /// 保持している先頭が出力全体のどの位置か。
    start: usize,
    bytes: Vec<u8>,
}

impl OutputBuffer {
    /// 保持している末尾の位置。次の要求の `outputFrom` にする。
    fn held_end(&self) -> usize {
        self.start + self.bytes.len()
    }

    /// 差分を取り込む。`dataStart` が保持末尾と連続なら連結し、そうでなければ置換する。
    fn apply(&mut self, chunk: &OutputChunkDto) {
        let data = decode(&chunk.data_base64);
        if chunk.data_start == self.held_end() {
            self.bytes.extend_from_slice(&data);
        } else {
            self.start = chunk.data_start;
            self.bytes = data;
        }
    }
}

/// 進め方(バッチ幅、または Step)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Advance {
    Batch(u64),
    Step,
}

/// 出力を `outputFrom` 付きで要求しながら終端まで進め、差分を `OutputBuffer` に積み上げる。
///
/// 各応答の `dataStart` は要求した `outputFrom` と一致しなければならない(差分だけが返ること)。
fn accumulate_to_end(
    service: &DebuggerService,
    session_id: u64,
    advance: Advance,
) -> (OutputBuffer, SessionStateDto) {
    let mut buffer = OutputBuffer::default();
    for _ in 0..CALL_BUDGET {
        let output_from = buffer.held_end();
        let (finished, state) = match advance {
            Advance::Batch(width) => {
                let BatchResultDto { stop, state, .. } = service
                    .run_batch(RunBatchRequest {
                        session_id,
                        batch_size: Some(width),
                        view: view(16, output_from),
                    })
                    .expect("run_batch accepts the request");
                (
                    matches!(stop, BatchStopDto::Halted | BatchStopDto::Error { .. }),
                    state,
                )
            }
            Advance::Step => {
                let StepResultDto { outcome, state } = service
                    .step(SessionViewRequest {
                        session_id,
                        view: view(16, output_from),
                    })
                    .expect("step accepts the request");
                (!matches!(outcome, StepOutcomeDto::Executed), state)
            }
        };
        assert_eq!(
            state.output.data_start, output_from,
            "{advance:?}: the response returns only the delta after outputFrom"
        );
        buffer.apply(&state.output);
        assert_eq!(
            buffer.held_end(),
            state.output.total_len,
            "{advance:?}: the accumulated output reaches the total length"
        );
        if finished {
            return (buffer, state);
        }
    }
    panic!("the output run did not finish within {CALL_BUDGET} calls");
}

#[test]
fn output_deltas_accumulated_by_the_frontend_rule_match_the_core() {
    // 多バイトの出力(0x00/0xFF/0x80 を含む)と、既定の Hello World を、差分で積み上げて確かめる。
    for case in corpus()
        .into_iter()
        .filter(|case| matches!(case.name, "hello" | "echo_raw_bytes"))
    {
        let core = run_core(&case.source, &case.input, None);
        assert!(
            !core.output.is_empty(),
            "{}: the case produces output",
            case.name
        );
        for advance in [
            Advance::Batch(1),
            Advance::Batch(2),
            Advance::Batch(7),
            Advance::Step,
        ] {
            let service = service();
            let program = load_ok(&service, &case.source);
            let session_id = service
                .start_session(start_request(program, &case.input, case.limit, vec![]))
                .expect("session starts")
                .state
                .session_id;
            let (buffer, last) = accumulate_to_end(&service, session_id, advance);
            let label = format!("{} / {advance:?}", case.name);
            assert_eq!(buffer.bytes, core.output, "{label}: accumulated output");
            assert_eq!(
                last.output.total_len,
                core.output.len(),
                "{label}: output length"
            );
        }
    }
}

#[test]
fn reset_after_input_is_consumed_restores_the_input_and_reruns_like_the_core() {
    let case = corpus()
        .into_iter()
        .find(|case| case.name == "echo_raw_bytes")
        .expect("the corpus has the echo case");
    let core = run_core(&case.source, &case.input, None);
    let service = service();
    let program = load_ok(&service, &case.source);
    let session_id = service
        .start_session(start_request(program, &case.input, case.limit, vec![]))
        .expect("session starts")
        .state
        .session_id;

    let first = batches_to_end(&service, session_id, None);
    assert_eq!(
        first.state.input_consumed,
        case.input.len(),
        "the first run reads the whole input"
    );
    assert_matches_core("first run", &service, &first, &core, &case.input);

    let reset = service
        .reset(SessionViewRequest {
            session_id,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    assert_eq!(reset.input_consumed, 0, "reset rewinds the input position");
    assert_eq!(
        reset.input_total,
        case.input.len(),
        "reset keeps the whole input"
    );
    assert_eq!(reset.output.total_len, 0, "reset empties the output");

    let rerun = batches_to_end(&service, session_id, None);
    assert_matches_core("rerun after reset", &service, &rerun, &core, &case.input);
}

/// 行・列・UTF-16 オフセットを、ソースの接頭辞から直接数える(core の `SourceMap` は使わない)。
///
/// 返り値は `(行(1 始まり), 列(スカラ値個数 + 1), UTF-16 オフセット)`。
fn expected_position(source: &str, byte_offset: usize) -> (usize, usize, usize) {
    let prefix = &source[..byte_offset];
    let utf16 = prefix.encode_utf16().count();
    let line = prefix.matches('\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = prefix[line_start..].chars().count() + 1;
    (line, column, utf16)
}

#[test]
fn parse_errors_match_the_core_kind_index_and_position() {
    // 解析エラーは実行の分割ではなく `loadSource` の結果として返るため、コーパスの外で比べる
    // (セッションを作れず、`Machine::run` の結果を持たないため)。
    // 2 種の括弧不整合を、多バイト・CRLF・サロゲートペアの前置と合わせて確かめる。
    for source in ["+]", "あ\r\n[+", "あ\r\n+]", "😀\n+]"] {
        let (kind, index, offset) = match Program::parse(source).expect_err("source is invalid") {
            ParseError::UnmatchedClosingBracket {
                instruction_index,
                source_offset,
            } => (
                ParseErrorKind::UnmatchedClosingBracket,
                instruction_index,
                source_offset,
            ),
            ParseError::UnmatchedOpeningBracket {
                instruction_index,
                source_offset,
            } => (
                ParseErrorKind::UnmatchedOpeningBracket,
                instruction_index,
                source_offset,
            ),
            // 上流の列挙は非網羅(`non_exhaustive`)。未知の種類は黙って写さず失敗させる。
            other => panic!("unknown parse error from the core: {other:?}"),
        };
        let (line, column, utf16) = expected_position(source, offset);
        let service = service();
        match service
            .load_source(LoadSourceRequest {
                source: source.to_owned(),
            })
            .expect("parse errors are results, not API errors")
        {
            LoadSourceResult::ParseError { error } => {
                assert_eq!(error.kind, kind, "{source:?}: kind");
                assert_eq!(error.instruction_index, index, "{source:?}: index");
                assert_eq!(
                    error.position.byte_offset, offset,
                    "{source:?}: byte offset"
                );
                assert_eq!(error.position.line, line, "{source:?}: line");
                assert_eq!(error.position.column, column, "{source:?}: column");
                assert_eq!(
                    error.position.utf16_offset, utf16,
                    "{source:?}: utf16 offset"
                );
            }
            other => panic!("{source:?}: expected a parse error, got {other:?}"),
        }
    }
}

#[test]
fn operation_sequence_pause_step_reset_breakpoints_and_resume_matches_the_core() {
    let core = run_core(LONG_COUNTER, b"", None);
    let service = service();
    let program = load_ok(&service, LONG_COUNTER);
    let session = service
        .start_session(start_request(program, b"", unlimited(), vec![]))
        .expect("session starts")
        .state
        .session_id;

    // 1. バッチを 1 回実行する(幅 1,000 は停止しない)。
    let first = run_batch(&service, session, Some(1_000));
    assert_eq!(first.stop, BatchStopDto::BatchReached);
    assert_eq!(first.executed, 1_000);

    // 2. Pause を送り続けて、幅 1 のバッチを `cancelled` で止める。
    // ここでは最初の `cancelled` が届けばよいので、間隔を空けず譲りながら送り続ける。
    // 間隔を空けると、プログラムが終わる前に Pause が 1 回も届かないことがある(Halted で打ち切られる)。
    let done = AtomicBool::new(false);
    let cancelled = thread::scope(|scope| {
        let _stop = StopOnDrop(&done);
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                service.pause(PauseRequest {
                    session_id: session,
                });
                thread::yield_now();
            }
        });
        for _ in 0..CALL_BUDGET {
            let result = run_batch(&service, session, Some(1));
            match result.stop {
                BatchStopDto::Cancelled => return result,
                BatchStopDto::BatchReached => {}
                other => panic!("the run ended before a pause took effect: {other:?}"),
            }
        }
        panic!("no pause took effect within {CALL_BUDGET} calls");
    });
    let paused_at = cancelled.state.snapshot.steps_executed;
    assert!(paused_at >= 1_000, "the pause follows the first batch");

    // 3. 停止後の Step は 1 命令だけ進む。
    let stepped = step(&service, session);
    assert_eq!(stepped.outcome, StepOutcomeDto::Executed);
    assert_eq!(stepped.state.snapshot.steps_executed, paused_at + 1);

    // 4. リセットで初期状態へ戻る(出力は空、ブレークポイントは保持する)。
    let reset = service
        .reset(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    assert_eq!(reset.snapshot.pc, 0);
    assert_eq!(reset.snapshot.dp, 0);
    assert_eq!(reset.snapshot.steps_executed, 0);
    assert_eq!(reset.output.total_len, 0);
    assert_eq!(reset.last_stop, None);

    // 5. ブレークポイントを命令 5 へ置き換えて、そこで停止することを確かめる。
    let offset = command_offsets(LONG_COUNTER)[5];
    let set = service
        .set_breakpoints(SetBreakpointsRequest {
            session_id: session,
            offsets: vec![offset],
        })
        .expect("breakpoints are accepted");
    assert_eq!(set.breakpoints.len(), 1);
    let stopped = run_batch(&service, session, None);
    assert_eq!(stopped.stop, BatchStopDto::Breakpoint);
    assert_eq!(stopped.state.snapshot.pc, 5);
    assert_eq!(stopped.state.snapshot.steps_executed, 5);

    // 6. ブレークポイントを解除し、残りを終端まで再開する。
    service
        .set_breakpoints(SetBreakpointsRequest {
            session_id: session,
            offsets: vec![],
        })
        .expect("clearing breakpoints is accepted");
    let finished = batches_to_end(&service, session, None);
    assert_eq!(finished.result, Ok(()));
    assert_matches_core("operation sequence", &service, &finished, &core, b"");
}
