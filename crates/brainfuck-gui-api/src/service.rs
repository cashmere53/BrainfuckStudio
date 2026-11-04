//! `DebuggerService`: Tauri 層から呼ばれる操作の入口。
//!
//! 1 つのサービスは 1 プログラムと 1 セッションだけを保持する。要求の値はすべて状態を変更する前に
//! 検査し、拒否した要求は状態を変えない。エラーは `ApiError::new` の固定文面と項目名だけを返す
//! (値・ソース・入力・パスを含めない)。
//!
//! 現在のセッション ID は `Mutex` の外からも照合できるよう `AtomicU64` にも写す(0 = セッションなし)。
//! 書き込みは `Mutex` を保持したまま行う。`pause` は後続タスクでこの値だけを読む。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use brainfuck_core::{BatchSize, DebugSession, MachineConfig, Program, SourceMap};

use crate::codec::{self, CodecError};
use crate::convert;
use crate::dto::{
    ApiError, ApiErrorKind, BatchResultDto, BreakpointDto, BreakpointsDto, LoadSourceRequest,
    LoadSourceResult, RunBatchRequest, SessionStartedDto, SessionStateDto, SessionViewRequest,
    StartSessionRequest, StepLimitDto, StepResultDto, TapeWindowDto, TapeWindowRequest,
    ViewRequestDto,
};
use crate::limits::{
    MAX_BATCH_SIZE, MAX_BREAKPOINT_OFFSETS, MAX_INPUT_BYTES, MAX_SOURCE_BYTES, MAX_STEP_LIMIT,
    MAX_TAPE_RADIUS,
};

/// 読み込み済みのプログラム。解析結果と位置変換表を持つ(ソース文字列は保持しない)。
struct LoadedProgram {
    id: u64,
    program: Program,
    source_map: SourceMap,
}

/// 現在のセッション。
struct SessionSlot {
    id: u64,
    session: DebugSession,
    input_total: usize,
}

/// `Mutex` の内側の状態。
#[derive(Default)]
struct Inner {
    /// 最後に発行した ID。プログラム ID とセッション ID は同じ採番で一意にする(0 は未発行)。
    issued: u64,
    program: Option<LoadedProgram>,
    session: Option<SessionSlot>,
}

impl Inner {
    fn allocate_id(&mut self) -> Result<u64, ApiError> {
        let next = self.issued.checked_add(1).ok_or_else(internal)?;
        self.issued = next;
        Ok(next)
    }
}

/// デバッガの操作を提供するサービス。`Send + Sync`。
pub struct DebuggerService {
    inner: Mutex<Inner>,
    /// 現在のセッション ID(0 = なし)。`Mutex` の外から照合するための写し。
    current_session_id: AtomicU64,
    /// キャンセル旗。`run_batch` は入口で偽に戻し、バッチ中は `CancelSignal` として読む。
    cancel: Arc<AtomicBool>,
}

impl Default for DebuggerService {
    fn default() -> Self {
        Self::new()
    }
}

impl DebuggerService {
    /// 空のサービスを作る(プログラムもセッションもない)。
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            current_session_id: AtomicU64::new(0),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, ApiError> {
        // 汚染されたロックは回復せず内部エラーにする。
        self.inner.lock().map_err(|_| internal())
    }

    /// ソースを解析して読み込む。
    ///
    /// 上限超過は `sourceTooLarge`。解析エラーは `LoadSourceResult::ParseError` で返し、
    /// 既存のプログラムとセッションは変えない。成功時は新しいプログラム ID を採番し、
    /// 既存のセッションを破棄する。
    pub fn load_source(&self, req: LoadSourceRequest) -> Result<LoadSourceResult, ApiError> {
        if req.source.len() > MAX_SOURCE_BYTES {
            return Err(ApiError::new(ApiErrorKind::SourceTooLarge, Some("source")));
        }
        let program = match Program::parse(&req.source) {
            Ok(program) => program,
            Err(error) => {
                return Ok(LoadSourceResult::ParseError {
                    error: convert::parse_error_dto(&req.source, &error)?,
                })
            }
        };
        let source_map = SourceMap::new(&program, &req.source).map_err(|_| internal())?;
        let instruction_count = source_map.instruction_count();
        let line_count = source_map.line_count();

        let mut inner = self.lock()?;
        let id = inner.allocate_id()?;
        inner.program = Some(LoadedProgram {
            id,
            program,
            source_map,
        });
        inner.session = None;
        self.current_session_id.store(0, Ordering::Release);
        Ok(LoadSourceResult::Ok {
            program_id: id,
            instruction_count,
            line_count,
        })
    }

    /// 読み込み済みのプログラムから新しいセッションを開始する。既存のセッションは置き換える。
    ///
    /// ブレークポイントの位置は命令添字へ正規化する(命令の無い位置は未解決)。
    /// 初期状態と設定結果を返す。
    pub fn start_session(&self, req: StartSessionRequest) -> Result<SessionStartedDto, ApiError> {
        check_view(&req.view)?;
        if req.breakpoint_offsets.len() > MAX_BREAKPOINT_OFFSETS {
            return Err(invalid("breakpointOffsets"));
        }
        let config = machine_config(req.limit)?;
        let input = decode_input(&req.input_base64)?;
        let input_total = input.len();

        let mut inner = self.lock()?;
        let loaded = inner
            .program
            .as_ref()
            .ok_or_else(|| ApiError::new(ApiErrorKind::ProgramNotLoaded, None))?;
        if loaded.id != req.program_id {
            return Err(ApiError::new(ApiErrorKind::StaleProgram, None));
        }
        let normalized = normalize_breakpoints(&loaded.source_map, &req.breakpoint_offsets);
        let breakpoints = breakpoints_dto(&loaded.source_map, &normalized)?;
        let mut session = DebugSession::new(loaded.program.clone(), input, config);
        for &(index, _) in &normalized.resolved {
            session
                .breakpoints_mut()
                .insert(index)
                .map_err(|_| internal())?;
        }

        let id = inner.allocate_id()?;
        let Inner {
            program,
            session: current,
            ..
        } = &mut *inner;
        let loaded = program.as_ref().ok_or_else(internal)?;
        let slot = SessionSlot {
            id,
            session,
            input_total,
        };
        // 初期状態は新しいセッションから組み立ててから置き換える。組み立てに失敗したら既存のセッションを残す。
        let state = state_of(&loaded.source_map, &slot, &req.view)?;
        *current = Some(slot);
        self.current_session_id.store(id, Ordering::Release);
        Ok(SessionStartedDto { state, breakpoints })
    }

    /// バッチを 1 回実行する。`batchSize` は省略で既定の 1,000、指定時は 1..=1,000,000。
    ///
    /// 要求の検査の後、キャンセル旗を偽に戻してからバッチを実行する(前回の Pause の残りで
    /// 次の Run が即中断しないため)。`Mutex` は 1 バッチの間保持する。
    pub fn run_batch(&self, req: RunBatchRequest) -> Result<BatchResultDto, ApiError> {
        let size = batch_size(req.batch_size)?;
        check_view(&req.view)?;

        let mut inner = self.lock()?;
        let Inner {
            program, session, ..
        } = &mut *inner;
        let slot = session_slot(session, req.session_id)?;
        let loaded = program.as_ref().ok_or_else(internal)?;

        self.cancel.store(false, Ordering::Release);
        let report = slot.session.run_batch(size, &*self.cancel);
        let state = state_of(&loaded.source_map, slot, &req.view)?;
        convert::batch_result_dto(report.executed, report.stop, state)
    }

    /// 1 命令だけ実行する(Step Into)。ブレークポイントは参照しない。
    pub fn step(&self, req: SessionViewRequest) -> Result<StepResultDto, ApiError> {
        check_view(&req.view)?;

        let mut inner = self.lock()?;
        let Inner {
            program, session, ..
        } = &mut *inner;
        let slot = session_slot(session, req.session_id)?;
        let loaded = program.as_ref().ok_or_else(internal)?;

        let outcome = convert::step_outcome_dto(slot.session.step_into())?;
        let state = state_of(&loaded.source_map, slot, &req.view)?;
        Ok(StepResultDto { outcome, state })
    }

    /// 機械を初期状態へ戻す(ブレークポイントは保持する)。出力は空になる。
    pub fn reset(&self, req: SessionViewRequest) -> Result<SessionStateDto, ApiError> {
        check_view(&req.view)?;

        let mut inner = self.lock()?;
        let Inner {
            program, session, ..
        } = &mut *inner;
        let slot = session_slot(session, req.session_id)?;
        let loaded = program.as_ref().ok_or_else(internal)?;

        slot.session.reset();
        state_of(&loaded.source_map, slot, &req.view)
    }

    /// 現在の状態を返す。機械は変えない。
    pub fn get_snapshot(&self, req: SessionViewRequest) -> Result<SessionStateDto, ApiError> {
        check_view(&req.view)?;

        let mut inner = self.lock()?;
        let Inner {
            program, session, ..
        } = &mut *inner;
        let slot = session_slot(session, req.session_id)?;
        let loaded = program.as_ref().ok_or_else(internal)?;

        state_of(&loaded.source_map, slot, &req.view)
    }

    /// テープの表示窓を返す。半径は 0..=64。
    pub fn get_tape_window(&self, req: TapeWindowRequest) -> Result<TapeWindowDto, ApiError> {
        if req.radius > MAX_TAPE_RADIUS {
            return Err(invalid("radius"));
        }

        let mut inner = self.lock()?;
        let slot = session_slot(&mut inner.session, req.session_id)?;
        Ok(convert::tape_window_dto(
            &slot.session.tape_window(req.radius),
        ))
    }
}

/// 現在のセッションを ID で引く。未開始・破棄済み・別 ID は `sessionNotFound`。
fn session_slot(
    session: &mut Option<SessionSlot>,
    session_id: u64,
) -> Result<&mut SessionSlot, ApiError> {
    match session {
        Some(slot) if slot.id == session_id => Ok(slot),
        _ => Err(ApiError::new(ApiErrorKind::SessionNotFound, None)),
    }
}

/// バッチ幅の指定を検査する。省略は既定の 1,000、指定時は 1..=1,000,000。
fn batch_size(requested: Option<u64>) -> Result<BatchSize, ApiError> {
    match requested {
        None => Ok(BatchSize::DEFAULT),
        Some(n) if (1..=MAX_BATCH_SIZE).contains(&n) => {
            BatchSize::new(n).ok_or_else(|| invalid("batchSize"))
        }
        Some(_) => Err(invalid("batchSize")),
    }
}

fn internal() -> ApiError {
    ApiError::new(ApiErrorKind::Internal, None)
}

fn invalid(field: &str) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidArgument, Some(field))
}

fn check_view(view: &ViewRequestDto) -> Result<(), ApiError> {
    if view.tape_radius > MAX_TAPE_RADIUS {
        return Err(invalid("tapeRadius"));
    }
    Ok(())
}

/// 命令数上限の指定を機械の設定へ写す。`limited` の `maxSteps` は 1..=2^53−1。
fn machine_config(limit: StepLimitDto) -> Result<MachineConfig, ApiError> {
    match limit {
        StepLimitDto::Limited { max_steps } if (1..=MAX_STEP_LIMIT).contains(&max_steps) => {
            Ok(MachineConfig::with_max_steps(max_steps))
        }
        StepLimitDto::Limited { .. } => Err(invalid("maxSteps")),
        StepLimitDto::Unlimited => Ok(MachineConfig::unlimited()),
    }
}

/// 入力の Base64 をバイト列へ復号する。上限は復号前に長さで検査する。
fn decode_input(text: &str) -> Result<Vec<u8>, ApiError> {
    codec::decode(text, MAX_INPUT_BYTES).map_err(|error| match error {
        CodecError::TooLong { .. } => {
            ApiError::new(ApiErrorKind::InputTooLarge, Some("inputBase64"))
        }
        CodecError::InvalidBase64 => invalid("inputBase64"),
    })
}

/// ブレークポイント位置の正規化結果。
struct Normalized {
    /// 解決済み。命令添字の昇順、同じ命令への重複は 1 件。
    /// 組は `(命令添字, その命令へ解決した要求のうち最初の UTF-16 オフセット)`。
    resolved: Vec<(usize, usize)>,
    /// 未解決の UTF-16 オフセット。昇順・重複なし。
    unresolved: Vec<usize>,
}

/// UTF-16 オフセットを命令添字へ正規化する。ブレークポイント置換でも同じ規則を使う。
///
/// 各位置は `instruction_at_or_after_utf16` で命令へ解決する(コメント上の位置は次の命令)。
/// 後続の命令がない位置は未解決になる。
fn normalize_breakpoints(source_map: &SourceMap, offsets: &[usize]) -> Normalized {
    let mut resolved: BTreeMap<usize, usize> = BTreeMap::new();
    let mut unresolved: BTreeSet<usize> = BTreeSet::new();
    for &offset in offsets {
        match source_map.instruction_at_or_after_utf16(offset) {
            Some(index) => {
                resolved.entry(index).or_insert(offset);
            }
            None => {
                unresolved.insert(offset);
            }
        }
    }
    Normalized {
        resolved: resolved.into_iter().collect(),
        unresolved: unresolved.into_iter().collect(),
    }
}

/// 解決済みの位置の `utf16Offset` は、同じ命令へ解決した要求のうち要求順で最初のオフセット
/// (`normalize_breakpoints` が要求順に走査して決める)。
fn breakpoints_dto(
    source_map: &SourceMap,
    normalized: &Normalized,
) -> Result<BreakpointsDto, ApiError> {
    let breakpoints = normalized
        .resolved
        .iter()
        .map(|&(index, utf16_offset)| {
            let range = source_map.range_of(index).ok_or_else(internal)?;
            Ok(BreakpointDto {
                utf16_offset,
                instruction_index: index,
                range: convert::source_range_dto(&range),
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(BreakpointsDto {
        breakpoints,
        unresolved: normalized.unresolved.clone(),
    })
}

/// セッションの状態応答を組み立てる。現在の命令の範囲は `SourceMap::range_of(pc)`
/// (停止状態では `None`)。
fn state_of(
    source_map: &SourceMap,
    slot: &SessionSlot,
    view: &ViewRequestDto,
) -> Result<SessionStateDto, ApiError> {
    let state = slot.session.state_view(view.tape_radius);
    let current_range = source_map.range_of(state.snapshot.pc);
    convert::session_state_dto(
        slot.id,
        &state,
        current_range.as_ref(),
        slot.session.machine().input_position(),
        slot.input_total,
        view.output_from,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_limit_maps_to_the_machine_config() {
        assert_eq!(
            machine_config(StepLimitDto::Limited { max_steps: 7 }).expect("in range"),
            MachineConfig::with_max_steps(7)
        );
        assert_eq!(
            machine_config(StepLimitDto::Limited {
                max_steps: MAX_STEP_LIMIT
            })
            .expect("in range"),
            MachineConfig::with_max_steps(MAX_STEP_LIMIT)
        );
        assert_eq!(
            machine_config(StepLimitDto::Unlimited).expect("unlimited"),
            MachineConfig::unlimited()
        );
    }

    #[test]
    fn step_limit_zero_and_over_the_json_maximum_are_rejected() {
        for max_steps in [0, MAX_STEP_LIMIT + 1, u64::MAX] {
            let error =
                machine_config(StepLimitDto::Limited { max_steps }).expect_err("out of range");
            assert_eq!(error, invalid("maxSteps"));
        }
    }

    #[test]
    fn normalization_keeps_the_first_request_per_instruction_and_sorts_unresolved() {
        let program = Program::parse("+ x\n+").expect("valid");
        let source_map = SourceMap::new(&program, "+ x\n+").expect("matches");
        let normalized = normalize_breakpoints(&source_map, &[4, 1, 99, 0, 2, 99]);
        assert_eq!(normalized.resolved, vec![(0, 0), (1, 4)]);
        assert_eq!(normalized.unresolved, vec![99]);
    }

    use brainfuck_core::RuntimeError;

    use crate::dto::BatchStopDto;

    fn load(service: &DebuggerService, source: &str) -> u64 {
        match service
            .load_source(LoadSourceRequest {
                source: source.to_owned(),
            })
            .expect("source is accepted")
        {
            LoadSourceResult::Ok { program_id, .. } => program_id,
            other => panic!("expected a loaded program, got {other:?}"),
        }
    }

    fn start(
        service: &DebuggerService,
        program_id: u64,
        input: &[u8],
        limit: StepLimitDto,
        offsets: Vec<usize>,
    ) -> SessionStartedDto {
        service
            .start_session(StartSessionRequest {
                program_id,
                input_base64: codec::encode(input),
                limit,
                breakpoint_offsets: offsets,
                view: ViewRequestDto {
                    tape_radius: 16,
                    output_from: 0,
                },
            })
            .expect("session starts")
    }

    #[test]
    fn limited_step_limit_stops_the_held_session_and_unlimited_does_not() {
        let service = DebuggerService::new();
        let program = load(&service, "+++");
        start(
            &service,
            program,
            b"",
            StepLimitDto::Limited { max_steps: 1 },
            vec![],
        );
        {
            let mut inner = service.inner.lock().expect("not poisoned");
            let slot = inner.session.as_mut().expect("a session is held");
            slot.session
                .step_into()
                .expect("first step is within the limit");
            let error = slot
                .session
                .step_into()
                .expect_err("second step is over the limit");
            assert!(matches!(
                error,
                RuntimeError::StepLimitExceeded { limit: 1, .. }
            ));
        }

        let service = DebuggerService::new();
        let program = load(&service, "+++");
        start(&service, program, b"", StepLimitDto::Unlimited, vec![]);
        let mut inner = service.inner.lock().expect("not poisoned");
        let slot = inner.session.as_mut().expect("a session is held");
        for _ in 0..3 {
            slot.session
                .step_into()
                .expect("unlimited sessions are not stopped by a step limit");
        }
    }

    #[test]
    fn held_breakpoints_are_the_resolved_instruction_indices_only() {
        let service = DebuggerService::new();
        // 命令は添字 0 の `+`(オフセット 0)と添字 1 の `+`(オフセット 4)。`x` はコメント(オフセット 2)。
        let program = load(&service, "+ x\n+");
        let started = start(
            &service,
            program,
            b"",
            StepLimitDto::Limited { max_steps: 1_000 },
            vec![0, 2, 99],
        );
        assert_eq!(started.breakpoints.unresolved, vec![99]);

        let mut inner = service.inner.lock().expect("not poisoned");
        let slot = inner.session.as_mut().expect("a session is held");
        let held: Vec<usize> = slot.session.breakpoints().iter().collect();
        assert_eq!(held, vec![0, 1]);
    }

    #[test]
    fn successful_load_drops_the_held_session() {
        let service = DebuggerService::new();
        let program = load(&service, "+.");
        start(&service, program, b"", StepLimitDto::Unlimited, vec![]);
        assert!(service
            .inner
            .lock()
            .expect("not poisoned")
            .session
            .is_some());

        load(&service, "+.");
        assert!(service
            .inner
            .lock()
            .expect("not poisoned")
            .session
            .is_none());
    }

    #[test]
    fn current_session_id_follows_start_replace_and_load() {
        let service = DebuggerService::new();
        let program = load(&service, "+.");
        assert_eq!(service.current_session_id.load(Ordering::Acquire), 0);

        let first = start(&service, program, b"", StepLimitDto::Unlimited, vec![])
            .state
            .session_id;
        assert_ne!(first, 0);
        assert_eq!(service.current_session_id.load(Ordering::Acquire), first);

        let second = start(&service, program, b"", StepLimitDto::Unlimited, vec![])
            .state
            .session_id;
        assert_ne!(second, first);
        assert_eq!(service.current_session_id.load(Ordering::Acquire), second);

        load(&service, "+.");
        assert_eq!(service.current_session_id.load(Ordering::Acquire), 0);
    }

    #[test]
    fn run_batch_clears_a_cancel_flag_left_over_before_it_entered() {
        let service = DebuggerService::new();
        let program = load(&service, "+.");
        let session = start(&service, program, b"", StepLimitDto::Unlimited, vec![]);
        // 前回の Pause の残り(旗が立ったまま)で次の Run が即中断しないこと。
        service.cancel.store(true, Ordering::Release);

        let result = service
            .run_batch(RunBatchRequest {
                session_id: session.state.session_id,
                batch_size: None,
                view: ViewRequestDto {
                    tape_radius: 16,
                    output_from: 0,
                },
            })
            .expect("run_batch accepts the request");
        assert_eq!(result.stop, BatchStopDto::Halted);
        assert_eq!(result.executed, 2);
        assert!(!service.cancel.load(Ordering::Acquire));
    }

    #[test]
    fn input_instruction_stores_the_first_input_byte_and_advances_the_position() {
        let service = DebuggerService::new();
        let program = load(&service, ",");
        start(&service, program, b"A", StepLimitDto::Unlimited, vec![]);

        let mut inner = service.inner.lock().expect("not poisoned");
        let slot = inner.session.as_mut().expect("a session is held");
        slot.session
            .step_into()
            .expect("the input instruction runs");
        assert_eq!(slot.session.machine().tape()[0], 0x41);
        assert_eq!(slot.session.machine().input_position(), 1);
    }

    fn start_request_for(program_id: u64, limit: StepLimitDto) -> StartSessionRequest {
        StartSessionRequest {
            program_id,
            input_base64: codec::encode(b""),
            limit,
            breakpoint_offsets: vec![],
            view: ViewRequestDto {
                tape_radius: 16,
                output_from: 0,
            },
        }
    }

    #[test]
    fn rejected_or_stale_start_keeps_the_live_session_id_and_state() {
        let service = DebuggerService::new();
        let program = load(&service, "+++");
        let live = start(&service, program, b"", StepLimitDto::Unlimited, vec![])
            .state
            .session_id;
        let view = || ViewRequestDto {
            tape_radius: 16,
            output_from: 0,
        };
        service
            .step(SessionViewRequest {
                session_id: live,
                view: view(),
            })
            .expect("the live session steps once");

        // ロック前の拒否(tapeRadius・maxSteps)とロック後の拒否(古い programId)。
        let mut too_wide = start_request_for(program, StepLimitDto::Unlimited);
        too_wide.view.tape_radius = 65;
        let no_steps = start_request_for(program, StepLimitDto::Limited { max_steps: 0 });
        let stale = start_request_for(program + 1_000, StepLimitDto::Unlimited);
        for request in [too_wide, no_steps, stale] {
            service
                .start_session(request)
                .expect_err("the request is rejected");
            assert_eq!(service.current_session_id.load(Ordering::Acquire), live);
            let state = service
                .get_snapshot(SessionViewRequest {
                    session_id: live,
                    view: view(),
                })
                .expect("the live session still answers under its own id");
            assert_eq!(state.session_id, live);
            assert_eq!(state.snapshot.steps_executed, 1);
            assert_eq!(state.snapshot.pc, 1);
        }

        let finished = service
            .run_batch(RunBatchRequest {
                session_id: live,
                batch_size: None,
                view: view(),
            })
            .expect("run_batch continues the live session");
        assert_eq!(finished.stop, BatchStopDto::Halted);
        assert_eq!(finished.state.session_id, live);
        assert_eq!(finished.state.snapshot.steps_executed, 3);
    }
}
