//! コード生成(`OptimizedProgram` → WASM バイト列)。
//!
//! 2.6 の範囲: 直線的な操作(`Add` / `Move` / `Output` / `Input`)と一般ループ(`LoopStart` / `LoopEnd`)を生成する。
//! クリア・移動ループ・走査(`Clear` / `Transfer` / `Scan`)と未知の操作種別は「常に離脱」として生成し、
//! 後続のタスク(2.7・2.8)が種別ごとに置き換える。
//!
//! 生成規則: 各操作の前に `pc` へ操作の先頭の命令添字を書き、検査(コスト・到達範囲)が
//! 成立しなければ `br` で離脱先へ分岐する。検査はすべて状態を変更する前に行う。
//! 検査が成立すれば状態を変更し、実行数へ素朴換算コストを加算する。
//!
//! 一般ループは `block { [検査 ・ loop { 本体 ・ ]検査 } }` で表す。`[` の検査は `block` の内側で入る時に 1 回だけ行い、
//! `block` の末尾が 0 セルの脱出先。`loop` の先頭は `[` の次(本体の直前)で、`]` が非 0 のとき戻る。
//! 離脱先の `br` の深さは、外側の離脱先ブロックから数えた開いている枠の数(`Nesting::frames`)で決める。

#![allow(dead_code)] // 公開窓口(2.4)から呼ばれるまでは未使用(`abi.rs` と同じ扱い)

use brainfuck_optimized::{Op, OpKind, OptimizedProgram};
use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction, MemArg, MemArgTarget, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

use crate::abi::{
    EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE, MEMORY_PAGES,
    STATUS_BAILOUT, STATUS_HALTED, TAPE_BYTES,
};
use crate::error::CompileError;
use crate::limits::{MAX_LOOP_DEPTH, MAX_MODULE_BYTES};

/// 生成中の制御構造の開き具合。離脱先(外側の `block`)より内側に開いている枠を数える。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Nesting {
    /// 開いている枠の数(一般ループ 1 つにつき `block` と `loop` の 2 つ)。
    frames: u32,
    /// 開いているループの数(入れ子の上限の検査に使う)。
    loops: usize,
}

/// 引数 `limit`(局所変数 0)。
const LOCAL_LIMIT: u32 = 0;
/// 現在のセル位置 `dp`(i32)。
const LOCAL_DP: u32 = 1;
/// 離脱時に返す `pc`(i32)。操作ごとに操作の先頭を書く。
const LOCAL_PC: u32 = 2;
/// 実行数 `steps`(i64)。
const LOCAL_STEPS: u32 = 3;

/// 型添字: `() -> i32`(`bf.read`)。
const TYPE_READ: u32 = 0;
/// 型添字: `(i32) -> ()`(`bf.write`)。
const TYPE_WRITE: u32 = 1;
/// 型添字: `(i64) -> (i32, i32, i32, i64)`(`run`)。
const TYPE_RUN: u32 = 2;

/// 関数添字: import の `bf.read`。
const FUNC_READ: u32 = 0;
/// 関数添字: import の `bf.write`。
const FUNC_WRITE: u32 = 1;
/// 関数添字: `run`(import の後ろ)。
const FUNC_RUN: u32 = 2;

/// 最後のセルの添字(`TAPE_BYTES - 1`)。
const LAST_CELL: i64 = (TAPE_BYTES - 1) as i64;

/// セル 1 バイトへの読み書き(オフセット 0、アラインメント 0、メモリ 0、順序なし)。
const CELL: MemArg = MemArg {
    offset: 0,
    align: 0,
    target: MemArgTarget::Memory(0),
    ordering: None,
};

/// `OptimizedProgram` から WASM モジュールのバイト列を生成する。純粋関数で、同一 IR から同一バイト列を返す。
///
/// 生成物は `run(limit: i64) -> (status, pc, dp, steps)` を export し、`memory` は 1 ページ。
/// 戻り値の意味は `abi.rs` の取り決めに従う。
///
/// # Errors
///
/// 生成物が `MAX_MODULE_BYTES` を超える、または数値が生成コードの表現範囲を超える場合は
/// `CompileError::ModuleTooLarge`。
pub(crate) fn emit(ir: &OptimizedProgram) -> Result<Vec<u8>, CompileError> {
    // 局所変数: dp(i32)、pc(i32)、steps(i64)。いずれも 0 で初期化される。
    let mut body = Function::new([(2, ValType::I32), (1, ValType::I64)]);

    // 離脱先のブロック。すべての操作の検査失敗はここへ分岐する。
    body.instruction(&Instruction::Block(BlockType::Empty));
    let mut nest = Nesting::default();
    for op in ir.ops() {
        emit_op(&mut body, op, &mut nest)?;
    }
    debug_assert_eq!(nest, Nesting::default(), "括弧は対応して閉じられている");
    // 停止: (STATUS_HALTED, 命令数, dp, steps)。
    body.instruction(&Instruction::I32Const(STATUS_HALTED));
    body.instruction(&Instruction::I32Const(to_i32(ir.source().len())?));
    body.instruction(&Instruction::LocalGet(LOCAL_DP));
    body.instruction(&Instruction::LocalGet(LOCAL_STEPS));
    body.instruction(&Instruction::Return);
    body.instruction(&Instruction::End);
    // 離脱: (STATUS_BAILOUT, pc, dp, steps)。pc は離脱した操作の先頭。
    body.instruction(&Instruction::I32Const(STATUS_BAILOUT));
    body.instruction(&Instruction::LocalGet(LOCAL_PC));
    body.instruction(&Instruction::LocalGet(LOCAL_DP));
    body.instruction(&Instruction::LocalGet(LOCAL_STEPS));
    body.instruction(&Instruction::End);

    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]); // TYPE_READ
    types.ty().function([ValType::I32], []); // TYPE_WRITE
    types.ty().function(
        [ValType::I64],
        [ValType::I32, ValType::I32, ValType::I32, ValType::I64],
    ); // TYPE_RUN

    let mut imports = ImportSection::new();
    imports.import(IMPORT_MODULE, IMPORT_READ, EntityType::Function(TYPE_READ));
    imports.import(
        IMPORT_MODULE,
        IMPORT_WRITE,
        EntityType::Function(TYPE_WRITE),
    );

    let mut functions = FunctionSection::new();
    functions.function(TYPE_RUN);

    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: MEMORY_PAGES,
        maximum: Some(MEMORY_PAGES),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut exports = ExportSection::new();
    exports.export(EXPORT_RUN, ExportKind::Func, FUNC_RUN);
    exports.export(EXPORT_MEMORY, ExportKind::Memory, 0);

    let mut code = CodeSection::new();
    code.function(&body);

    let mut module = Module::new();
    module.section(&types);
    module.section(&imports);
    module.section(&functions);
    module.section(&memory);
    module.section(&exports);
    module.section(&code);
    let bytes = module.finish();
    if bytes.len() > MAX_MODULE_BYTES {
        return Err(CompileError::ModuleTooLarge);
    }
    Ok(bytes)
}

/// 1 操作ぶんの生成。先頭で `pc` を書き、種別ごとの規則で検査・適用・分岐する。
fn emit_op(f: &mut Function, op: &Op, nest: &mut Nesting) -> Result<(), CompileError> {
    let start = to_i32(op.source_range().start)?;
    f.instruction(&Instruction::I32Const(start));
    f.instruction(&Instruction::LocalSet(LOCAL_PC));

    // 離脱先(最外の `block`)までの深さ。内側に開いている枠の数に等しい。
    let depth = nest.frames;
    match op.kind() {
        OpKind::Add(n) => {
            guard_cost(f, op.naive_cost(0, 0), depth)?;
            // tape[dp] = (tape[dp] + n) mod 256(i32.store8 が下位 8 ビットを格納する)。
            f.instruction(&Instruction::LocalGet(LOCAL_DP));
            f.instruction(&Instruction::LocalGet(LOCAL_DP));
            f.instruction(&Instruction::I32Load8U(CELL));
            f.instruction(&Instruction::I32Const(i32::from(*n)));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::I32Store8(CELL));
            charge(f, op.naive_cost(0, 0))?;
        }
        OpKind::Move(n) => match op.static_reach() {
            Some((min, max)) => {
                guard_cost(f, op.naive_cost(0, 0), depth)?;
                guard_reach(f, min, max, depth)?;
                // dp += n。範囲検査の後なので結果は 0..=LAST_CELL に収まる。
                f.instruction(&Instruction::LocalGet(LOCAL_DP));
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Const(i64_of(*n)?));
                f.instruction(&Instruction::I64Add);
                f.instruction(&Instruction::I32WrapI64);
                f.instruction(&Instruction::LocalSet(LOCAL_DP));
                charge(f, op.naive_cost(0, 0))?;
            }
            None => always_bail(f, depth),
        },
        OpKind::Output => {
            guard_cost(f, op.naive_cost(0, 0), depth)?;
            // bf.write(tape[dp])
            f.instruction(&Instruction::LocalGet(LOCAL_DP));
            f.instruction(&Instruction::I32Load8U(CELL));
            f.instruction(&Instruction::Call(FUNC_WRITE));
            charge(f, op.naive_cost(0, 0))?;
        }
        OpKind::Input => {
            guard_cost(f, op.naive_cost(0, 0), depth)?;
            // tape[dp] = bf.read()
            f.instruction(&Instruction::LocalGet(LOCAL_DP));
            f.instruction(&Instruction::Call(FUNC_READ));
            f.instruction(&Instruction::I32Store8(CELL));
            charge(f, op.naive_cost(0, 0))?;
        }
        OpKind::LoopStart { .. } => {
            if nest.loops >= MAX_LOOP_DEPTH {
                return Err(CompileError::NestingTooDeep);
            }
            // 構造: block { [の検査・charge・ゼロ判定(脱出は br 0)  loop { 本体 ] の検査・charge・br_if 0 } }。
            // `[` の検査は入る時の 1 回だけで、`]` の戻り先は `loop` の先頭(本体の直前)。
            // 核の `]` は非 0 のとき `[` の次へ戻り、`[` は再判定もコスト加算もされないため。
            f.instruction(&Instruction::Block(BlockType::Empty));
            nest.frames += 1;
            nest.loops += 1;
            // 検査は開いた `block` の内側で行う(離脱先までの深さは `nest.frames`)。
            guard_cost(f, op.naive_cost(0, 0), nest.frames)?;
            charge(f, op.naive_cost(0, 0))?;
            // tape[dp] = 0 なら脱出先(`block` の末尾)へ分岐する。最内の枠は `block` なので深さは 0。
            f.instruction(&Instruction::LocalGet(LOCAL_DP));
            f.instruction(&Instruction::I32Load8U(CELL));
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::BrIf(0));
            f.instruction(&Instruction::Loop(BlockType::Empty));
            nest.frames += 1;
        }
        OpKind::LoopEnd { .. } => {
            // 検査は開いたままの枠の内側で行う(離脱先までの深さは `nest.frames`)。
            guard_cost(f, op.naive_cost(0, 0), depth)?;
            charge(f, op.naive_cost(0, 0))?;
            // tape[dp] != 0 なら本体の先頭(直近の `loop`)へ戻る。内側の括弧は閉じ済みなので深さは 0。
            f.instruction(&Instruction::LocalGet(LOCAL_DP));
            f.instruction(&Instruction::I32Load8U(CELL));
            f.instruction(&Instruction::BrIf(0));
            // 脱出: `loop` と `block` を閉じる。
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
            nest.frames -= 2;
            nest.loops -= 1;
        }
        // 直線的でない残りの操作(クリア・移動ループ・走査)と未知の操作種別は、
        // 後続のタスクが種別ごとに置き換えるまで常に離脱する。
        _ => always_bail(f, depth),
    }
    Ok(())
}

/// 常に離脱する(`pc` は既に操作の先頭を指している)。`depth` は離脱先までの深さ。
fn always_bail(f: &mut Function, depth: u32) {
    f.instruction(&Instruction::Br(depth));
}

/// コスト検査: `remaining = limit - steps` が `cost` 未満なら離脱する。`depth` は離脱先までの深さ。
fn guard_cost(f: &mut Function, cost: u64, depth: u32) -> Result<(), CompileError> {
    f.instruction(&Instruction::LocalGet(LOCAL_LIMIT));
    f.instruction(&Instruction::LocalGet(LOCAL_STEPS));
    f.instruction(&Instruction::I64Sub);
    f.instruction(&Instruction::I64Const(to_i64(cost)?));
    f.instruction(&Instruction::I64LtS);
    f.instruction(&Instruction::BrIf(depth));
    Ok(())
}

/// 到達範囲検査: `dp + min < 0` または `dp + max > LAST_CELL` なら離脱する。`depth` は離脱先までの深さ。
fn guard_reach(f: &mut Function, min: isize, max: isize, depth: u32) -> Result<(), CompileError> {
    // dp < -min
    f.instruction(&Instruction::LocalGet(LOCAL_DP));
    f.instruction(&Instruction::I64ExtendI32U);
    f.instruction(&Instruction::I64Const(
        i64_of(min)?
            .checked_neg()
            .ok_or(CompileError::ModuleTooLarge)?,
    ));
    f.instruction(&Instruction::I64LtS);
    // dp > LAST_CELL - max
    f.instruction(&Instruction::LocalGet(LOCAL_DP));
    f.instruction(&Instruction::I64ExtendI32U);
    f.instruction(&Instruction::I64Const(
        LAST_CELL
            .checked_sub(i64_of(max)?)
            .ok_or(CompileError::ModuleTooLarge)?,
    ));
    f.instruction(&Instruction::I64GtS);
    f.instruction(&Instruction::I32Or);
    f.instruction(&Instruction::BrIf(depth));
    Ok(())
}

/// 実行数の加算: `steps += cost`。検査の後にだけ呼ぶ(`steps + cost <= limit` が保証される)。
fn charge(f: &mut Function, cost: u64) -> Result<(), CompileError> {
    f.instruction(&Instruction::LocalGet(LOCAL_STEPS));
    f.instruction(&Instruction::I64Const(to_i64(cost)?));
    f.instruction(&Instruction::I64Add);
    f.instruction(&Instruction::LocalSet(LOCAL_STEPS));
    Ok(())
}

/// 生成コードの定数へ変換する。表現できない値は規模の超過として扱う。
fn to_i32(n: usize) -> Result<i32, CompileError> {
    i32::try_from(n).map_err(|_| CompileError::ModuleTooLarge)
}

fn to_i64(n: u64) -> Result<i64, CompileError> {
    i64::try_from(n).map_err(|_| CompileError::ModuleTooLarge)
}

fn i64_of(n: isize) -> Result<i64, CompileError> {
    i64::try_from(n).map_err(|_| CompileError::ModuleTooLarge)
}

#[cfg(test)]
mod tests {
    use crate::runtime::{load, run, RawRun, Status};
    use brainfuck_core::{Program, TAPE_SIZE};
    use brainfuck_optimized::OptimizedProgram;
    use wasmparser::Validator;

    fn optimized(source: &str) -> OptimizedProgram {
        let program = Program::parse(source).expect("テスト用のソースは解析できる");
        OptimizedProgram::compile(&program)
    }

    /// 生成物が検証と監査を通ることを確かめる。
    fn assert_valid_and_audited(bytes: &[u8]) {
        Validator::new()
            .validate_all(bytes)
            .expect("生成物は wasmparser の検証を通る");
        crate::audit::audit(bytes).expect("生成物は方針監査を通る");
    }

    /// 生成 → ロード → 実行までを通す。
    fn run_source(source: &str, input: &[u8], limit: u64) -> RawRun {
        let bytes = crate::emit::emit(&optimized(source)).expect("生成できる");
        assert_valid_and_audited(&bytes);
        let loaded = load(&bytes).expect("ロードできる");
        run(&loaded, input, limit).expect("実行できる")
    }

    #[test]
    fn linear_program_emits_a_module_that_validates_and_audits() {
        // Add・Move(両向き)・Output・Input をすべて含む。
        let bytes = crate::emit::emit(&optimized("+++>--.,<-.")).expect("生成できる");
        assert_valid_and_audited(&bytes);
    }

    #[test]
    fn empty_program_emits_a_valid_module_that_halts_in_the_initial_state() {
        let bytes = crate::emit::emit(&optimized("")).expect("生成できる");
        assert_valid_and_audited(&bytes);
        let result = run_source("", &[], 0);
        assert_eq!(result.status, Status::Halted);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
        assert!(result.output.is_empty());
        assert_eq!(result.tape.len(), TAPE_SIZE);
        assert!(result.tape.iter().all(|&cell| cell == 0));
    }

    #[test]
    fn emission_is_deterministic_for_the_same_ir() {
        let ir = optimized("+++>--.,<-.");
        let first = crate::emit::emit(&ir).expect("生成できる");
        let second = crate::emit::emit(&ir).expect("生成できる");
        assert_eq!(first, second);
        let again = crate::emit::emit(&optimized("+++>--.,<-.")).expect("生成できる");
        assert_eq!(first, again);
    }

    #[test]
    fn add_counts_the_naive_cost_of_a_merged_run() {
        // `+` を 3 つ: Add(3) のコストは素朴換算の 3。
        let result = run_source("+++.", &[], 100);
        assert_eq!(result.status, Status::Halted);
        assert_eq!(result.pc, 4);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 4);
        assert_eq!(result.tape[0], 3);
        assert_eq!(result.output, vec![3]);
    }

    #[test]
    fn add_wraps_to_eight_bits_without_touching_the_neighbour() {
        // `-.++.`: Add(255) の後に出力、Add(2) で 255 + 2 = 257 → 8 ビットへ wrap して 1。
        // 併合されない並びなので、i32.store への置き換えは tape[1] へ 1 を漏らして落ちる。
        let result = run_source("-.++.", &[], 100);
        assert_eq!(result.status, Status::Halted);
        assert_eq!(result.pc, 5);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 5);
        assert_eq!(result.tape[0], 1);
        assert_eq!(result.tape[1], 0);
        assert_eq!(result.output, vec![255, 1]);
    }

    #[test]
    fn add_wraps_across_zero_between_outputs() {
        // `-.+.`: 255 を出力し、`+` で 0 に wrap して再び出力する。
        let result = run_source("-.+.", &[], 100);
        assert_eq!(result.status, Status::Halted);
        assert_eq!(result.pc, 4);
        assert_eq!(result.steps, 4);
        assert_eq!(result.tape[0], 0);
        assert_eq!(result.tape[1], 0);
        assert_eq!(result.output, vec![255, 0]);
    }

    #[test]
    fn move_past_the_last_cell_bails_before_moving() {
        // 移動量 30000 は最後のセル 29999 を超える。コストは上限内でも範囲検査で離脱する。
        let result = run_source(&">".repeat(30_000), &[], 100_000);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
        assert!(result.tape.iter().all(|&cell| cell == 0));
    }

    #[test]
    fn last_cell_is_reachable_and_one_step_further_bails() {
        let mut reach = ">".repeat(29_999);
        reach.push('+');
        let ok = run_source(&reach, &[], 100_000);
        assert_eq!(ok.status, Status::Halted);
        assert_eq!(ok.pc, 30_000);
        assert_eq!(ok.dp, 29_999);
        assert_eq!(ok.steps, 30_000);
        assert_eq!(ok.tape[29_999], 1);
        assert_eq!(ok.tape[0], 0);

        let mut beyond = reach.clone();
        beyond.push('>');
        let bail = run_source(&beyond, &[], 100_000);
        assert_eq!(bail.status, Status::Bailout);
        assert_eq!(bail.pc, 30_000);
        assert_eq!(bail.dp, 29_999);
        assert_eq!(bail.steps, 30_000);
        assert_eq!(bail.tape[29_999], 1);
    }

    #[test]
    fn output_over_budget_bails_without_writing() {
        let result = run_source(".", &[], 0);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
        assert!(result.output.is_empty());
    }

    #[test]
    fn output_after_a_spent_budget_bails_at_its_own_start() {
        // `+` で予算 1 を使い切り、続く `.` のコスト 1 は残予算 0 を超える。
        let result = run_source("+.", &[], 1);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 1);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 1);
        assert_eq!(result.tape[0], 1);
        assert!(result.output.is_empty());
    }

    #[test]
    fn input_over_budget_bails_without_consuming_input() {
        let result = run_source(",", b"A", 0);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
        assert_eq!(result.input_pos, 0);
        assert_eq!(result.tape[0], 0);
    }

    #[test]
    fn move_over_budget_bails_without_moving() {
        // `>>` は Move(2) で、コスト 2 は上限 1 を超える。
        let result = run_source(">>", &[], 1);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
    }

    #[test]
    fn move_updates_the_data_pointer_within_the_tape() {
        // セル 2 に 1 を置き、戻って セル 0 を出力する。
        let result = run_source(">>+<<.", &[], 100);
        assert_eq!(result.status, Status::Halted);
        assert_eq!(result.pc, 6);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 6);
        assert_eq!(result.tape[2], 1);
        assert_eq!(result.output, vec![0]);
    }

    #[test]
    fn input_and_output_go_through_the_host_functions() {
        let result = run_source(",.", b"A", 100);
        assert_eq!(result.status, Status::Halted);
        assert_eq!(result.pc, 2);
        assert_eq!(result.steps, 2);
        assert_eq!(result.tape[0], b'A');
        assert_eq!(result.input_pos, 1);
        assert_eq!(result.output, b"A".to_vec());

        // 入力尽きでは 0 を格納し、位置は進めない。
        let exhausted = run_source(",,", b"", 100);
        assert_eq!(exhausted.status, Status::Halted);
        assert_eq!(exhausted.tape[0], 0);
        assert_eq!(exhausted.input_pos, 0);
    }

    #[test]
    fn cost_over_remaining_budget_bails_before_any_state_change() {
        // Add(5) のコスト 5 は上限 3 を超える: 状態は初期のまま、pc = 操作の先頭 0。
        let result = run_source("+++++", &[], 3);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
        assert_eq!(result.tape[0], 0);
    }

    #[test]
    fn bail_in_the_middle_reports_the_start_of_the_failing_operation() {
        // Add(2) ・ Output ・ Add(3) の順。上限 3 では Add(3) の前に予算が尽きる。
        let result = run_source("++.+++", &[], 3);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 3);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 3);
        assert_eq!(result.tape[0], 2);
        assert_eq!(result.output, vec![2]);
    }

    #[test]
    fn move_out_of_range_bails_before_moving() {
        let result = run_source("<+", &[], 100);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 0);
        assert_eq!(result.dp, 0);
        assert_eq!(result.steps, 0);
        assert_eq!(result.tape[0], 0);
    }

    #[test]
    fn non_linear_operations_always_bail_at_their_start() {
        // `+` の後の `[-]`(Clear)は 2.3 の既定により常に離脱する。
        let result = run_source("+[-]", &[], 100);
        assert_eq!(result.status, Status::Bailout);
        assert_eq!(result.pc, 1);
        assert_eq!(result.steps, 1);
        assert_eq!(result.tape[0], 1);
    }
}
