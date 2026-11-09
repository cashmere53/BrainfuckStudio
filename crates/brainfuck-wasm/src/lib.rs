//! WASM バックエンド(`brainfuck-wasm`)。
//!
//! 最適化 IR(`brainfuck-optimized` の `OptimizedProgram`)から WebAssembly バイナリを生成し、
//! `wasmi`(インタプリタ)上で隔離して実行する。結果は素朴インタプリタ(`brainfuck-core`)と完全状態一致させる。
//!
//! # 採用方式
//!
//! - 生成: `wasm-encoder`(IR → バイナリ。同一 IR から同一バイト列)
//! - 監査: `wasmparser`(生成物の方針検査。1 つでも外れれば拒否し、実行しない)
//! - 実行: `wasmi`(機械語を生成しないインタプリタ。燃料計測とメモリ制限を標準で持つ)
//!
//! # 安全方針の要点
//!
//! - 外部との接点は import `bf.read` / `bf.write` のみ(WASI なし)。export は `run` と `memory` のみ
//! - メモリは 1 ページ固定で拡張不可。テーブル・グローバル・start 関数は持たない
//! - 実行量は命令数上限(厳密)と燃料(第二の防御)で有界。出力量は `limits::MAX_OUTPUT_BYTES` で有界
//! - 上限・範囲外・資源制限に当たる操作は、状態を変更する前に生成コードから離脱する。
//!   離脱後の再生は上流の公開 `brainfuck_optimized::replay` が行い、素朴意味論はこの crate に複製しない
//! - 外部供給の WASM を読み込む経路は持たない。`unsafe` は禁止(crate 属性とワークスペース lint の二重)
//!
//! 公開窓口は `WasmProgram`(`compile` / `run`)と `WasmRun`(観測値)。

#![forbid(unsafe_code)]

pub mod limits;

mod abi;
mod audit;
mod emit;
mod error;
mod program;
mod runtime;

pub use error::{CompileError, PolicyViolation, RunError};
pub use program::{WasmProgram, WasmRun};

#[cfg(test)]
mod tests {
    use crate::abi::{
        EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE, STATUS_BAILOUT,
        STATUS_HALTED,
    };
    use crate::error::{CompileError, PolicyViolation, RunError};
    use crate::limits::{
        FUEL_BASE, FUEL_PER_STEP, MAX_LOOP_DEPTH, MAX_MODULE_BYTES, MAX_OUTPUT_BYTES,
    };

    #[test]
    fn limits_match_design() {
        assert_eq!(MAX_MODULE_BYTES, 8 * 1024 * 1024);
        assert_eq!(MAX_LOOP_DEPTH, 256);
        assert_eq!(FUEL_PER_STEP, 64);
        assert_eq!(FUEL_BASE, 1_024);
        assert_eq!(MAX_OUTPUT_BYTES, 64 * 1024 * 1024);
    }

    #[test]
    fn abi_names_and_status_codes() {
        assert_eq!(IMPORT_MODULE, "bf");
        assert_eq!(IMPORT_READ, "read");
        assert_eq!(IMPORT_WRITE, "write");
        assert_eq!(EXPORT_RUN, "run");
        assert_eq!(EXPORT_MEMORY, "memory");
        assert_eq!(STATUS_HALTED, 0);
        assert_eq!(STATUS_BAILOUT, 1);
    }

    #[test]
    fn error_types_are_payload_free_except_policy() {
        fn assert_value<T: Copy + Eq + std::fmt::Debug>() {}
        assert_value::<CompileError>();
        assert_value::<RunError>();
        assert_value::<PolicyViolation>();

        fn compile_kind(e: CompileError) -> u8 {
            match e {
                CompileError::ModuleTooLarge => 0,
                CompileError::NestingTooDeep => 1,
                CompileError::PolicyViolation(_) => 2,
                CompileError::Engine => 3,
            }
        }
        fn run_kind(e: RunError) -> u8 {
            match e {
                RunError::WatchdogTripped => 0,
                RunError::OutputLimitExceeded => 1,
                RunError::InvalidState => 2,
                RunError::Engine => 3,
            }
        }
        fn policy_kind(v: PolicyViolation) -> u8 {
            match v {
                PolicyViolation::ImportSet => 0,
                PolicyViolation::Memory => 1,
                PolicyViolation::Table => 2,
                PolicyViolation::Global => 3,
                PolicyViolation::Start => 4,
                PolicyViolation::ExportSet => 5,
                PolicyViolation::MemoryGrow => 6,
                PolicyViolation::IndirectCall => 7,
                PolicyViolation::FloatingPoint => 8,
            }
        }
        assert_eq!(
            compile_kind(CompileError::PolicyViolation(PolicyViolation::Start)),
            2
        );
        assert_eq!(compile_kind(CompileError::Engine), 3);
        assert_eq!(run_kind(RunError::InvalidState), 2);
        assert_eq!(policy_kind(PolicyViolation::FloatingPoint), 8);
    }

    /// 依存の疎通: wasm-encoder で手組みしたモジュールを wasmparser で検証し、
    /// wasmi で燃料つき・メモリ制限つき・start 禁止の設定のまま実行できること、
    /// および上流の再生と操作のコスト・到達範囲ヘルパをこの crate から呼べること。
    mod connectivity {
        use crate::abi::{EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE};
        use crate::limits::FUEL_BASE;
        use brainfuck_core::{Program, StopReason, TAPE_SIZE};
        use brainfuck_optimized::replay::{replay, ReplayParts, ReplayState};
        use brainfuck_optimized::{OpKind, OptimizedProgram};
        use wasm_encoder::{
            CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
            ImportSection, Instruction, MemorySection, MemoryType, Module, StartSection,
            TypeSection, ValType,
        };
        use wasmi::{Config, Engine, Linker, Module as WasmiModule, Store, StoreLimitsBuilder};
        use wasmparser::Validator;

        /// 入出力 2 関数の import、1 ページのメモリ、`run`(read・write を呼んで 42 を返す)を持つモジュール。
        /// `with_start` のときは `() -> ()` の start 関数を追加する。
        fn hand_built_module(with_start: bool) -> Vec<u8> {
            let mut types = TypeSection::new();
            types.ty().function([], [ValType::I32]); // type 0: () -> i32(read と run)
            types.ty().function([ValType::I32], []); // type 1: (i32) -> ()(write)
            types.ty().function([], []); // type 2: () -> ()(start)

            let mut imports = ImportSection::new();
            imports.import(IMPORT_MODULE, IMPORT_READ, EntityType::Function(0));
            imports.import(IMPORT_MODULE, IMPORT_WRITE, EntityType::Function(1));

            let mut functions = FunctionSection::new();
            functions.function(0); // func 2: run
            if with_start {
                functions.function(2); // func 3: start
            }

            let mut memory = MemorySection::new();
            memory.memory(MemoryType {
                minimum: 1,
                maximum: Some(1),
                memory64: false,
                shared: false,
                page_size_log2: None,
            });

            let mut exports = ExportSection::new();
            exports.export(EXPORT_RUN, ExportKind::Func, 2);
            exports.export(EXPORT_MEMORY, ExportKind::Memory, 0);

            let mut code = CodeSection::new();
            let mut run = Function::new([]);
            run.instruction(&Instruction::Call(0));
            run.instruction(&Instruction::Call(1));
            run.instruction(&Instruction::I32Const(42));
            run.instruction(&Instruction::End);
            code.function(&run);
            if with_start {
                let mut start = Function::new([]);
                start.instruction(&Instruction::End);
                code.function(&start);
            }

            let mut module = Module::new();
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

        #[test]
        fn hand_built_module_validates_and_runs_under_fuel_and_memory_limits() {
            let bytes = hand_built_module(false);
            Validator::new()
                .validate_all(&bytes)
                .expect("手組みモジュールは検証を通る");

            let mut config = Config::default();
            config.consume_fuel(true).allow_start_fn(false);
            let engine = Engine::new(&config);
            let module =
                WasmiModule::new(&engine, &bytes).expect("エンジンがモジュールを受け付ける");

            let mut linker: Linker<wasmi::StoreLimits> = Linker::new(&engine);
            linker
                .func_wrap(IMPORT_MODULE, IMPORT_READ, || -> i32 { 0 })
                .expect("bf.read を定義できる");
            linker
                .func_wrap(IMPORT_MODULE, IMPORT_WRITE, |_value: i32| {})
                .expect("bf.write を定義できる");

            let limits = StoreLimitsBuilder::new()
                .memory_size(64 * 1024)
                .instances(1)
                .tables(0)
                .memories(1)
                .build();
            let mut store = Store::new(&engine, limits);
            store.limiter(|limits| limits);
            store.set_fuel(FUEL_BASE).expect("燃料を設定できる");

            let instance = linker
                .instantiate_and_start(&mut store, &module)
                .expect("インスタンス化できる");
            let run = instance
                .get_typed_func::<(), i32>(&store, EXPORT_RUN)
                .expect("run を取得できる");
            assert_eq!(run.call(&mut store, ()).expect("燃料つきで実行できる"), 42);
            assert!(
                store.get_fuel().expect("燃料を読める") < FUEL_BASE,
                "燃料が実行中に消費される"
            );
            let memory = instance
                .get_memory(&store, EXPORT_MEMORY)
                .expect("memory を取得できる");
            assert_eq!(memory.size(&store), 1);
        }

        #[test]
        fn start_function_is_rejected_when_disallowed() {
            let bytes = hand_built_module(true);
            let mut config = Config::default();
            config.consume_fuel(true).allow_start_fn(false);
            let engine = Engine::new(&config);
            let mut linker: Linker<wasmi::StoreLimits> = Linker::new(&engine);
            linker
                .func_wrap(IMPORT_MODULE, IMPORT_READ, || -> i32 { 0 })
                .expect("bf.read を定義できる");
            linker
                .func_wrap(IMPORT_MODULE, IMPORT_WRITE, |_value: i32| {})
                .expect("bf.write を定義できる");
            let limits = StoreLimitsBuilder::new().build();
            let mut store = Store::new(&engine, limits);
            store.limiter(|limits| limits);
            let result = WasmiModule::new(&engine, &bytes)
                .and_then(|module| linker.instantiate_and_start(&mut store, &module));
            assert!(result.is_err(), "start 関数を含むモジュールは拒否される");
        }

        #[test]
        fn upstream_replay_and_cost_helpers_are_callable() {
            // 空プログラムの再生は停止を返し、状態は初期状態のまま。
            let empty = Program::parse("").expect("空のソースは解析できる");
            let mut state = ReplayState::new(Vec::new());
            let replayed = replay(&empty, None, 0, &mut state);
            assert_eq!(replayed.result, Ok(StopReason::Halted));
            assert_eq!(replayed.pc, 0);
            assert_eq!(state.tape().len(), TAPE_SIZE);

            // 状態の読み戻しと、検査つきの生成(不整合は拒否され、panic しない)。
            let parts: ReplayParts = state.into_parts();
            assert_eq!(parts.tape.len(), TAPE_SIZE);
            let rebuilt =
                ReplayState::from_parts(parts.clone()).expect("正しい途中状態は受け付ける");
            assert_eq!(rebuilt.into_parts(), parts);
            let mut broken = parts;
            broken.dp = TAPE_SIZE;
            assert!(ReplayState::from_parts(broken).is_err());

            // 操作の素朴換算コストと到達範囲のヘルパを呼べる。
            let program = Program::parse("+++>").expect("解析できる");
            let optimized = OptimizedProgram::compile(&program);
            let movement = optimized
                .ops()
                .iter()
                .find(|op| matches!(op.kind(), OpKind::Move(_)))
                .expect("移動操作がある");
            assert_eq!(movement.naive_cost(0, 0), 1);
            assert_eq!(movement.static_reach(), Some((0, 1)));
        }
    }
}
