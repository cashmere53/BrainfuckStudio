//! wasmi 上の隔離実行。エンジンの型はこのモジュールの外へ出さない。

use brainfuck_core::TAPE_SIZE;
use wasmi::errors::LinkerError;
use wasmi::{
    Caller, CompilationMode, Config, Engine, Error as WasmiError, Linker, Module, ResourceLimiter,
    Store, StoreLimits, StoreLimitsBuilder, TrapCode,
};

use crate::abi::{
    EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE, MEMORY_PAGES,
    STATUS_BAILOUT, STATUS_HALTED,
};
use crate::error::{CompileError, RunError};
use crate::limits::{FUEL_BASE, FUEL_PER_STEP, MAX_OUTPUT_BYTES};

/// 1 ページのバイト数。
const PAGE_BYTES: usize = 65_536;

/// `run` の戻り値の型: `(状態コード, pc, dp, 実行数)`。
type RunResults = (i32, i32, i32, i64);

/// 実行の状態コード(生成コードの戻り値の解釈)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    /// 停止(`pc` = 命令数)。
    Halted,
    /// 離脱(`pc` = 離脱した操作の先頭の命令添字)。
    Bailout,
}

/// 読み込み済みのモジュール。実行ごとに新しいストアとインスタンスを作って使う。
pub(crate) struct Loaded {
    engine: Engine,
    module: Module,
    linker: Linker<HostState>,
    /// 生成物のバイト数。直線的な本体の入口での一括請求分を、燃料の下駄として加える。
    static_fuel: u64,
}

/// 1 回の実行の結果(素朴意味論への変換は呼び出し側が行う)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawRun {
    pub status: Status,
    pub pc: usize,
    pub dp: usize,
    pub steps: u64,
    /// 線形メモリ先頭 `TAPE_SIZE` バイト。
    pub tape: Box<[u8]>,
    pub input_pos: usize,
    pub output: Vec<u8>,
}

/// ストアのデータ: ホスト関数が参照する入出力とメモリ制限。
struct HostState {
    input: Vec<u8>,
    input_pos: usize,
    output: Vec<u8>,
    output_cap: usize,
    output_exceeded: bool,
    limits: StoreLimits,
}

/// モジュールをエンジンに読み込む(生成・監査は呼び出し側の責務)。
/// 形(run の署名、メモリ 1 ページ、start 関数なし、import の充足)を満たさなければ `CompileError::Engine`。
pub(crate) fn load(module: &[u8]) -> Result<Loaded, CompileError> {
    let mut config = Config::default();
    config
        .consume_fuel(true)
        // 遅延翻訳(既定)では関数本体の翻訳費用が最初の run の燃料から引かれ、実行順序で結果が変わる。
        // 読み込み時に全関数を翻訳して、翻訳費用を実行の燃料から切り離す。
        .compilation_mode(CompilationMode::Eager)
        .allow_start_fn(false)
        // SIMD・memory64 は wasmi の Cargo feature で既定オフ(本 crate は有効化しない)。
        .wasm_reference_types(false)
        .wasm_bulk_memory(false)
        .wasm_multi_memory(false)
        .wasm_tail_call(false)
        .wasm_extended_const(false)
        .wasm_custom_page_sizes(false)
        .wasm_wide_arithmetic(false)
        .wasm_saturating_float_to_int(false)
        .wasm_sign_extension(false)
        .wasm_mutable_global(false);
    let engine = Engine::new(&config);
    let static_fuel = u64::try_from(module.len()).unwrap_or(u64::MAX);
    let module = Module::new(&engine, module).map_err(|_| CompileError::Engine)?;
    let mut linker = Linker::new(&engine);
    define_host(&mut linker).map_err(|_| CompileError::Engine)?;
    let loaded = Loaded {
        engine,
        module,
        linker,
        static_fuel,
    };

    // 受け入れ確認: 使い捨てのストアでインスタンス化し、run とメモリの形を確かめる。
    let mut store = new_store(&loaded.engine, HostState::new(&[], MAX_OUTPUT_BYTES));
    store
        .set_fuel(FUEL_BASE)
        .map_err(|_| CompileError::Engine)?;
    let instance = loaded
        .linker
        .instantiate_and_start(&mut store, &loaded.module)
        .map_err(|_| CompileError::Engine)?;
    instance
        .get_typed_func::<i64, RunResults>(&store, EXPORT_RUN)
        .map_err(|_| CompileError::Engine)?;
    let memory = instance
        .get_memory(&store, EXPORT_MEMORY)
        .ok_or(CompileError::Engine)?;
    if memory.size(&store) != MEMORY_PAGES {
        return Err(CompileError::Engine);
    }
    Ok(loaded)
}

/// 1 回の実行。燃料は `limit × FUEL_PER_STEP + FUEL_BASE + 生成物のバイト数`、出力量の上限は `MAX_OUTPUT_BYTES`。
/// 生成物のバイト数を加えるのは、直線的な本体が入口で一括に請求されても上限 0 で枯渇しないようにするため。
pub(crate) fn run(loaded: &Loaded, input: &[u8], limit: u64) -> Result<RawRun, RunError> {
    run_with_output_cap(loaded, input, limit, MAX_OUTPUT_BYTES)
}

/// `run` の本体。出力量の上限を引数で受け取る(テストが小さい上限で中断を確かめるため)。
fn run_with_output_cap(
    loaded: &Loaded,
    input: &[u8],
    limit: u64,
    output_cap: usize,
) -> Result<RawRun, RunError> {
    // 実行ごとに新しいストア(テープのゼロ初期化を含む)とインスタンスを作る。
    let mut store = new_store(&loaded.engine, HostState::new(input, output_cap));
    let fuel = limit
        .saturating_mul(FUEL_PER_STEP)
        .saturating_add(FUEL_BASE)
        .saturating_add(loaded.static_fuel);
    store.set_fuel(fuel).map_err(|_| RunError::Engine)?;
    let instance = loaded
        .linker
        .instantiate_and_start(&mut store, &loaded.module)
        .map_err(|_| RunError::Engine)?;
    let run = instance
        .get_typed_func::<i64, RunResults>(&store, EXPORT_RUN)
        .map_err(|_| RunError::Engine)?;
    // 生成コードの引数は i64。上限が i64 の範囲を超える場合は i64::MAX に丸める(燃料が第二の防御として残る)。
    let arg = i64::try_from(limit).unwrap_or(i64::MAX);
    let (status, pc, dp, steps) = match run.call(&mut store, arg) {
        Ok(results) => results,
        Err(error) => {
            let output_exceeded = store.data().output_exceeded;
            return Err(classify_trap(&error, output_exceeded));
        }
    };

    let status = match status {
        STATUS_HALTED => Status::Halted,
        STATUS_BAILOUT => Status::Bailout,
        _ => return Err(RunError::InvalidState),
    };
    let pc = usize::try_from(pc).map_err(|_| RunError::InvalidState)?;
    let dp = usize::try_from(dp).map_err(|_| RunError::InvalidState)?;
    let steps = u64::try_from(steps).map_err(|_| RunError::InvalidState)?;
    let memory = instance
        .get_memory(&store, EXPORT_MEMORY)
        .ok_or(RunError::Engine)?;
    let tape: Box<[u8]> = memory
        .data(&store)
        .get(..TAPE_SIZE)
        .ok_or(RunError::InvalidState)?
        .into();
    let host = store.into_data();
    Ok(RawRun {
        status,
        pc,
        dp,
        steps,
        tape,
        input_pos: host.input_pos,
        output: host.output,
    })
}

impl HostState {
    fn new(input: &[u8], output_cap: usize) -> HostState {
        HostState {
            input: input.to_vec(),
            input_pos: 0,
            output: Vec::new(),
            output_cap,
            output_exceeded: false,
            limits: StoreLimitsBuilder::new()
                .memory_size(PAGE_BYTES * MEMORY_PAGES as usize)
                .memories(1)
                .instances(1)
                .tables(0)
                .build(),
        }
    }
}

/// ストアを作り、メモリ制限を接続する。燃料は呼び出し側が設定する。
fn new_store(engine: &Engine, state: HostState) -> Store<HostState> {
    let mut store = Store::new(engine, state);
    store.limiter(|state: &mut HostState| -> &mut dyn ResourceLimiter { &mut state.limits });
    store
}

/// ホスト関数 `bf.read` / `bf.write` を定義する。これ以外の import は持たない(WASI なし)。
fn define_host(linker: &mut Linker<HostState>) -> Result<(), LinkerError> {
    linker.func_wrap(
        IMPORT_MODULE,
        IMPORT_READ,
        |mut caller: Caller<'_, HostState>| -> i32 {
            let state = caller.data_mut();
            match state.input.get(state.input_pos) {
                Some(&byte) => {
                    state.input_pos += 1;
                    i32::from(byte)
                }
                None => 0,
            }
        },
    )?;
    linker.func_wrap(
        IMPORT_MODULE,
        IMPORT_WRITE,
        |mut caller: Caller<'_, HostState>, value: i32| -> Result<(), WasmiError> {
            let state = caller.data_mut();
            if state.output.len() >= state.output_cap {
                state.output_exceeded = true;
                return Err(WasmiError::from(TrapCode::UnreachableCodeReached));
            }
            state.output.push(value as u8);
            Ok(())
        },
    )?;
    Ok(())
}

/// 実行中のトラップを方式の失敗へ分類する。出力量の超過はホスト側の印で判定する。
fn classify_trap(error: &WasmiError, output_exceeded: bool) -> RunError {
    if output_exceeded {
        RunError::OutputLimitExceeded
    } else if error.as_trap_code() == Some(TrapCode::OutOfFuel) {
        RunError::WatchdogTripped
    } else {
        RunError::Engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE};
    use brainfuck_core::TAPE_SIZE;
    use wasm_encoder::{
        BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
        ImportSection, Instruction, MemArg, MemArgTarget, MemorySection, MemoryType,
        Module as WasmModule, StartSection, TypeSection, ValType,
    };

    /// 手組みモジュールの run 本体の末尾: 状態 = 停止、`pc` = `dp` = 実行数 = 0。
    fn halted_tail() -> Vec<Instruction<'static>> {
        vec![
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I64Const(0),
        ]
    }

    fn store8_at_zero() -> MemArg {
        MemArg {
            offset: 0,
            align: 0,
            target: MemArgTarget::Memory(0),
            ordering: None,
        }
    }

    /// import `bf.read`(func 0)・`bf.write`(func 1)、`run`(func 2、`(i64) -> (i32, i32, i32, i64)`)、
    /// 最大 1 ページのメモリを持つ `module_with_memory` の既定形。
    fn module_with(run_body: Vec<Instruction<'static>>, with_start: bool) -> Vec<u8> {
        module_with_memory(run_body, with_start, 1)
    }

    /// import `bf.read`(func 0)・`bf.write`(func 1)、`run`(func 2、`(i64) -> (i32, i32, i32, i64)`)、
    /// 最小 1 ページ・最大 `memory_max` ページのメモリを持つモジュール。`run` は i32 のローカルを 1 つ持つ(index 1)。
    /// `with_start` のときは `() -> ()` の start 関数(func 3)を追加する。
    fn module_with_memory(
        run_body: Vec<Instruction<'static>>,
        with_start: bool,
        memory_max: u64,
    ) -> Vec<u8> {
        let mut types = TypeSection::new();
        types.ty().function([], [ValType::I32]); // type 0: read
        types.ty().function([ValType::I32], []); // type 1: write
        types.ty().function(
            [ValType::I64],
            [ValType::I32, ValType::I32, ValType::I32, ValType::I64],
        ); // type 2: run
        types.ty().function([], []); // type 3: start

        let mut imports = ImportSection::new();
        imports.import(IMPORT_MODULE, IMPORT_READ, EntityType::Function(0));
        imports.import(IMPORT_MODULE, IMPORT_WRITE, EntityType::Function(1));

        let mut functions = FunctionSection::new();
        functions.function(2); // func 2: run
        if with_start {
            functions.function(3); // func 3: start
        }

        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: Some(memory_max),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        let mut exports = ExportSection::new();
        exports.export(EXPORT_RUN, ExportKind::Func, 2);
        exports.export(EXPORT_MEMORY, ExportKind::Memory, 0);

        let mut code = CodeSection::new();
        let mut run = Function::new([(1, ValType::I32)]);
        for instruction in &run_body {
            run.instruction(instruction);
        }
        run.instruction(&Instruction::End);
        code.function(&run);
        if with_start {
            let mut start = Function::new([]);
            start.instruction(&Instruction::End);
            code.function(&start);
        }

        let mut module = WasmModule::new();
        module.section(&types);
        module.section(&imports);
        module.section(&functions);
        module.section(&memory);
        module.section(&exports);
        if with_start {
            module.section(&StartSection { function_index: 3 });
        }
        module.section(&code);
        module.finish()
    }

    fn loaded(run_body: Vec<Instruction<'static>>) -> Loaded {
        load(&module_with(run_body, false)).expect("手組みモジュールを読み込める")
    }

    /// `memory.grow(1)` を 2 回試みる本体(宣言上の最大は 2 ページ)。結果は先頭 2 セルへ格納する。
    /// リミッタが無ければ 1 回目は成功(1 を返す)するため、テープの値でリミッタの接続を判定できる。
    #[test]
    fn memory_grow_is_refused_by_the_limiter() {
        let mut body = vec![
            Instruction::I32Const(0),
            Instruction::I32Const(1),
            Instruction::MemoryGrow(0),
            Instruction::I32Store8(store8_at_zero()),
        ];
        body.extend(halted_tail());
        let loaded = load(&module_with_memory(body, false, 2)).expect("読み込める");

        let raw = run(&loaded, b"", 1_000).expect("拡張の拒否は実行の失敗ではない");
        assert_eq!(raw.status, Status::Halted);
        assert_eq!(
            raw.tape[0], 0xFF,
            "宣言上の最大は 2 ページでも、リミッタが 1 ページに抑えるため memory.grow は -1 を返す"
        );
        assert_eq!(raw.tape.len(), TAPE_SIZE);
    }

    /// 燃料 = limit × FUEL_PER_STEP + FUEL_BASE + 生成物のバイト数 の接続を確かめる。
    /// 1000 反復・反復あたり 5 命令の有限ループ(約 5000 の燃料を消費する)を走らせる。
    /// 生成物のバイト数(数十バイト)を含めても、下の境界(limit 10 → 1664 + 数十)は中断、
    /// 上の境界(limit 100 → 7424 + 数十)は完走する。
    #[test]
    fn fuel_formula_bounds_a_finite_loop() {
        let mut body = vec![
            Instruction::I32Const(1000),
            Instruction::LocalSet(1),
            Instruction::Loop(BlockType::Empty),
            Instruction::LocalGet(1),
            Instruction::I32Const(1),
            Instruction::I32Sub,
            Instruction::LocalTee(1),
            Instruction::BrIf(0),
            Instruction::End,
        ];
        body.extend(halted_tail());
        let loaded = loaded(body);

        assert_eq!(
            run(&loaded, b"", 10).err(),
            Some(RunError::WatchdogTripped),
            "燃料 1664 では 5000 程度の反復を完走できない"
        );
        let raw = run(&loaded, b"", 100).expect("燃料 7424 なら完走する");
        assert_eq!(raw.status, Status::Halted);
    }

    #[test]
    fn infinite_loop_trips_the_fuel_watchdog() {
        let mut body = vec![
            Instruction::Loop(BlockType::Empty),
            Instruction::Br(0),
            Instruction::End,
        ];
        body.extend(halted_tail());
        let loaded = loaded(body);

        assert_eq!(
            run(&loaded, b"", 1_000).err(),
            Some(RunError::WatchdogTripped)
        );
    }

    #[test]
    fn output_beyond_the_cap_bails_out_with_output_limit() {
        let mut body = vec![
            Instruction::Loop(BlockType::Empty),
            Instruction::I32Const(65),
            Instruction::Call(1),
            Instruction::Br(0),
            Instruction::End,
        ];
        body.extend(halted_tail());
        let loaded = loaded(body);

        assert_eq!(
            run_with_output_cap(&loaded, b"", 1_000_000, 10).err(),
            Some(RunError::OutputLimitExceeded)
        );
    }

    #[test]
    fn output_exactly_at_the_cap_is_accepted() {
        let mut body = vec![
            Instruction::I32Const(65),
            Instruction::Call(1),
            Instruction::I32Const(66),
            Instruction::Call(1),
            Instruction::I32Const(67),
            Instruction::Call(1),
        ];
        body.extend(halted_tail());
        let loaded = loaded(body);

        let raw = run_with_output_cap(&loaded, b"", 1_000, 3).expect("上限ちょうどは成功する");
        assert_eq!(raw.output, b"ABC".to_vec());
        assert_eq!(
            run_with_output_cap(&loaded, b"", 1_000, 2).err(),
            Some(RunError::OutputLimitExceeded),
            "上限を 1 バイト下回れば中断する"
        );
    }

    #[test]
    fn read_returns_bytes_then_zero_without_advancing_at_eof() {
        // 3 回読んで先頭の 3 セルへ格納する。入力は 2 バイトなので 3 回目は EOF。
        let mut body = Vec::new();
        for address in 0..3 {
            body.push(Instruction::I32Const(address));
            body.push(Instruction::Call(0));
            body.push(Instruction::I32Store8(store8_at_zero()));
        }
        body.extend(halted_tail());
        let loaded = loaded(body);

        let raw = run(&loaded, &[0x41, 0xFF], 1_000).expect("実行できる");
        assert_eq!(&raw.tape[..3], &[0x41, 0xFF, 0x00]);
        assert_eq!(raw.input_pos, 2, "EOF では位置を進めない");
    }

    #[test]
    fn each_run_starts_from_a_fresh_zeroed_instance() {
        // tape[0] = tape[0] + 1。毎回ゼロ初期化されるなら、どの実行でも 1 になる。
        let mut body = vec![
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Load8U(store8_at_zero()),
            Instruction::I32Const(1),
            Instruction::I32Add,
            Instruction::I32Store8(store8_at_zero()),
        ];
        body.extend(halted_tail());
        let loaded = loaded(body);

        for _ in 0..2 {
            let raw = run(&loaded, b"", 1_000).expect("実行できる");
            assert_eq!(raw.tape[0], 1);
        }
    }

    #[test]
    fn start_function_is_rejected_at_load() {
        let bytes = module_with(halted_tail(), true);
        assert_eq!(load(&bytes).err(), Some(CompileError::Engine));
    }

    /// 直線的な本体(`i32.const 0; i32.const 1; i32.store8` を `pairs` 回並べ、末尾で停止)。
    /// 命令数は `3 × pairs`。ループを含まないため、燃料は本体の実行分だけを消費する。
    fn straight_line_body(pairs: usize) -> Vec<Instruction<'static>> {
        let mut body = Vec::with_capacity(pairs * 3 + 4);
        for _ in 0..pairs {
            body.push(Instruction::I32Const(0));
            body.push(Instruction::I32Const(1));
            body.push(Instruction::I32Store8(store8_at_zero()));
        }
        body.extend(halted_tail());
        body
    }

    /// 遅延翻訳では、最初の run が関数本体の翻訳費用を燃料から引く。
    /// そのため同じ `Loaded` でも実行順序によって結果が変わってはならない(要件 4.8 の根拠: 燃料の挙動は実行順序に依存せず決定的)。
    /// 最初の実行を上限 0 にしても `WatchdogTripped` にならず、後の大きな上限の結果は新規読み込みと一致する。
    #[test]
    fn first_run_with_zero_limit_does_not_depend_on_translation_order() {
        let bytes = module_with(straight_line_body(2_000), false);

        let shared = load(&bytes).expect("読み込める");
        let first = run(&shared, b"", 0).expect("上限 0 でも翻訳費用で燃料枯渇しない");
        assert_eq!(first.status, Status::Halted);
        let later = run(&shared, b"", 1_000_000).expect("後の実行も完走する");

        let fresh = load(&bytes).expect("読み込める");
        let baseline = run(&fresh, b"", 1_000_000).expect("新規の読み込みでも完走する");
        assert_eq!(later, baseline, "実行順序に依存せず結果が同じ");
        assert_eq!(later.tape[0], 1);
    }

    /// 長い直線の本体は入口で一括に燃料を請求されるため、上限 0 でも枯渇してはならない(生成物のバイト数を燃料に加える)。
    #[test]
    fn long_straight_line_body_does_not_run_out_of_fuel_at_zero_limit() {
        let loaded = load(&module_with(straight_line_body(2_000), false)).expect("読み込める");

        let raw = run(&loaded, b"", 0).expect("直線の本体は上限 0 でも枯渇しない");
        assert_eq!(raw.status, Status::Halted);
        assert_eq!(raw.tape[0], 1);
    }
}
