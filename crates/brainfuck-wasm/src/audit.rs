//! 生成物の方針監査。実行の前に、生成物が方針の範囲内であることを機械的に検査する。
//!
//! 外部供給のモジュールを受け取る公開入口は持たない。監査の肯定・否定のテストは
//! このファイル内のテスト専用モジュールで、手組みの違反モジュールを使って行う。

use crate::abi::{EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE};
use crate::error::PolicyViolation;
use wasmparser::{
    BinaryReaderError, CompositeInnerType, ExternalKind, FunctionBody, MemoryType, Operator,
    Parser, Payload, SubType, TypeRef, ValType, Validator, WasmFeatures,
};

/// 関数型の `(引数, 戻り値)`。関数型でない型定義は `None`。
type FuncSig = (Vec<ValType>, Vec<ValType>);

/// 監査に必要な生成物の構造。セクションを 1 回読んで採取し、判定は `judge` が順に行う。
#[derive(Default)]
struct Facts<'a> {
    /// 型定義の並び(型インデックスの順)。
    types: Vec<Option<FuncSig>>,
    /// import の (モジュール名, 名前, 種類)。
    imports: Vec<(&'a str, &'a str, TypeRef)>,
    /// メモリ定義の並び。
    memories: Vec<MemoryType>,
    /// テーブル定義の数。
    tables: usize,
    /// グローバル定義の数。
    globals: usize,
    /// start 関数の有無。
    start: bool,
    /// export の (名前, 種類)。
    exports: Vec<(&'a str, ExternalKind)>,
    /// 関数本体の命令から見つかった最初の違反(本体の順)。
    body: Option<PolicyViolation>,
}

/// 生成物が方針に適合するかを検査する。1 つでも外れれば違反を返し、実行されない。
///
/// 最初に `wasmparser` で整形式かつ検証を通ることを確認し、そのうえで方針の項目を判定する。
/// 整形式でないバイト列は方針に適合しないものとして拒否する(専用の違反種別は無いため
/// `ImportSet` に写す。これは生成物の不整合を示す防御的な写像であり、生成器の出力は常に整形式)。
///
/// 配線は後続タスク 2.4 で行うまで未使用のため、限定的に警告を抑止する。
#[allow(dead_code)]
pub(crate) fn audit(module: &[u8]) -> Result<(), PolicyViolation> {
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(module)
        .map_err(malformed)?;
    let facts = collect(module)?;
    judge(&facts)
}

fn malformed(_error: BinaryReaderError) -> PolicyViolation {
    PolicyViolation::ImportSet
}

/// セクションを読んで構造を採取する。整形式であることは `audit` の検証で確認済み。
fn collect(module: &[u8]) -> Result<Facts<'_>, PolicyViolation> {
    let mut facts = Facts::default();
    for payload in Parser::new(0).parse_all(module) {
        match payload.map_err(malformed)? {
            Payload::TypeSection(reader) => {
                for group in reader {
                    for sub in group.map_err(malformed)?.into_types() {
                        facts.types.push(func_sig(&sub));
                    }
                }
            }
            Payload::ImportSection(reader) => {
                for group in reader {
                    for item in group.map_err(malformed)? {
                        let (_offset, import) = item.map_err(malformed)?;
                        facts.imports.push((import.module, import.name, import.ty));
                    }
                }
            }
            Payload::MemorySection(reader) => {
                for memory in reader {
                    facts.memories.push(memory.map_err(malformed)?);
                }
            }
            Payload::TableSection(reader) => facts.tables += reader.count() as usize,
            Payload::GlobalSection(reader) => facts.globals += reader.count() as usize,
            Payload::StartSection { .. } => facts.start = true,
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(malformed)?;
                    facts.exports.push((export.name, export.kind));
                }
            }
            Payload::CodeSectionEntry(body) if facts.body.is_none() => {
                facts.body = scan_body(body).err();
            }
            _ => {}
        }
    }
    Ok(facts)
}

fn func_sig(sub: &SubType) -> Option<FuncSig> {
    match &sub.composite_type.inner {
        CompositeInnerType::Func(func) => Some((func.params().to_vec(), func.results().to_vec())),
        _ => None,
    }
}

/// 方針の判定。順序は import → メモリ → 関数本体 → テーブル → グローバル → start → export。
fn judge(facts: &Facts<'_>) -> Result<(), PolicyViolation> {
    check_imports(&facts.types, &facts.imports)?;
    check_memory(&facts.memories)?;
    if let Some(violation) = facts.body {
        return Err(violation);
    }
    if facts.tables > 0 {
        return Err(PolicyViolation::Table);
    }
    if facts.globals > 0 {
        return Err(PolicyViolation::Global);
    }
    if facts.start {
        return Err(PolicyViolation::Start);
    }
    check_exports(&facts.exports)
}

/// import 集合が `{bf.read: () -> i32, bf.write: (i32) -> ()}` と厳密一致することを確認する。
fn check_imports(
    types: &[Option<FuncSig>],
    imports: &[(&str, &str, TypeRef)],
) -> Result<(), PolicyViolation> {
    let mut read_seen = false;
    let mut write_seen = false;
    for (module, name, ty) in imports {
        match ty {
            TypeRef::Table(_) => return Err(PolicyViolation::Table),
            TypeRef::Memory(_) => return Err(PolicyViolation::Memory),
            TypeRef::Global(_) => return Err(PolicyViolation::Global),
            TypeRef::Func(index) => {
                let sig = types.get(*index as usize).and_then(Option::as_ref);
                let is_bf = *module == IMPORT_MODULE;
                match (is_bf, *name, sig) {
                    (true, IMPORT_READ, Some((params, results)))
                        if !read_seen
                            && params.is_empty()
                            && results.as_slice() == [ValType::I32] =>
                    {
                        read_seen = true;
                    }
                    (true, IMPORT_WRITE, Some((params, results)))
                        if !write_seen
                            && params.as_slice() == [ValType::I32]
                            && results.is_empty() =>
                    {
                        write_seen = true;
                    }
                    _ => return Err(PolicyViolation::ImportSet),
                }
            }
            _ => return Err(PolicyViolation::ImportSet),
        }
    }
    if read_seen && write_seen {
        Ok(())
    } else {
        Err(PolicyViolation::ImportSet)
    }
}

/// メモリが 1 つで、最小 = 最大 = 1 ページ、非共有、32 ビットであることを確認する。
fn check_memory(memories: &[MemoryType]) -> Result<(), PolicyViolation> {
    match memories {
        [memory]
            if !memory.memory64
                && !memory.shared
                && memory.initial == 1
                && memory.maximum == Some(1) =>
        {
            Ok(())
        }
        _ => Err(PolicyViolation::Memory),
    }
}

/// export 集合が `{run: 関数, memory: メモリ}` と厳密一致することを確認する。
fn check_exports(exports: &[(&str, ExternalKind)]) -> Result<(), PolicyViolation> {
    let mut run_seen = false;
    let mut memory_seen = false;
    for (name, kind) in exports {
        match kind {
            ExternalKind::Global => return Err(PolicyViolation::Global),
            ExternalKind::Table => return Err(PolicyViolation::Table),
            ExternalKind::Func if *name == EXPORT_RUN && !run_seen => run_seen = true,
            ExternalKind::Memory if *name == EXPORT_MEMORY && !memory_seen => memory_seen = true,
            _ => return Err(PolicyViolation::ExportSet),
        }
    }
    if run_seen && memory_seen {
        Ok(())
    } else {
        Err(PolicyViolation::ExportSet)
    }
}

/// 関数本体の命令を走査し、禁止命令の最初の違反を返す。
fn scan_body(body: FunctionBody<'_>) -> Result<(), PolicyViolation> {
    for operator in body.get_operators_reader().map_err(malformed)? {
        check_operator(&operator.map_err(malformed)?)?;
    }
    Ok(())
}

fn check_operator(operator: &Operator<'_>) -> Result<(), PolicyViolation> {
    match operator {
        Operator::MemoryGrow { .. } => Err(PolicyViolation::MemoryGrow),
        Operator::CallIndirect { .. }
        | Operator::ReturnCallIndirect { .. }
        | Operator::CallRef { .. }
        | Operator::ReturnCallRef { .. } => Err(PolicyViolation::IndirectCall),
        _ if is_floating_point(operator) => Err(PolicyViolation::FloatingPoint),
        _ => Ok(()),
    }
}

/// 命令名が値の型 `f32` / `f64` を含むか。浮動小数点の定数・演算・読み書き・変換を含む。
fn is_float_name(name: &str) -> bool {
    name.contains("F32") || name.contains("F64")
}

// 全演算子を `wasmparser` の一覧から列挙し、命令名で浮動小数点かを判定する対応表を生成する。
// 一覧に無い将来の演算子は浮動小数点とみなさない(検証器が提案機能の無効化で先に拒否する)。
macro_rules! floating_point_operators {
    ( $( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*) )* ) => {
        /// 演算子が浮動小数点命令か。
        fn is_floating_point(operator: &Operator<'_>) -> bool {
            match operator {
                $( Operator::$op $( { $($arg: _),* } )? => is_float_name(stringify!($op)), )*
                _ => false,
            }
        }
    };
}
wasmparser::for_each_operator!(floating_point_operators);

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // 違反モジュールは既定の仕様から 1 項目だけ変えて組む
mod tests {
    use super::audit;
    use crate::abi::{EXPORT_MEMORY, EXPORT_RUN, IMPORT_MODULE, IMPORT_READ, IMPORT_WRITE};
    use crate::error::PolicyViolation;
    use wasm_encoder::{
        CodeSection, ConstExpr, EntityType, ExportKind, ExportSection, Function, FunctionSection,
        GlobalSection, GlobalType, ImportSection, Instruction, MemorySection, MemoryType, Module,
        RefType, StartSection, TableSection, TableType, TypeSection, ValType,
    };

    /// 手組みモジュールの仕様。既定値は方針に適合するモジュール。
    ///
    /// 型の並び: 0 = `() -> i32`(read)、1 = `(i32) -> ()`(write)、
    /// 2 = `(i64) -> (i32, i32, i32, i64)`(run)、3 = `() -> ()`(helper / start / 追加 import)。
    #[derive(Clone)]
    struct Spec {
        imports: Vec<(&'static str, &'static str, EntityType)>,
        memories: Vec<MemoryType>,
        memory_export: bool,
        table: bool,
        global: bool,
        start: bool,
        helper: bool,
        extra_exports: Vec<(&'static str, ExportKind, u32)>,
        body: Vec<Instruction<'static>>,
    }

    fn conforming_memory() -> MemoryType {
        MemoryType {
            minimum: 1,
            maximum: Some(1),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec {
                imports: vec![
                    (IMPORT_MODULE, IMPORT_READ, EntityType::Function(0)),
                    (IMPORT_MODULE, IMPORT_WRITE, EntityType::Function(1)),
                ],
                memories: vec![conforming_memory()],
                memory_export: true,
                table: false,
                global: false,
                start: false,
                helper: false,
                extra_exports: Vec::new(),
                body: Vec::new(),
            }
        }
    }

    fn build(spec: &Spec) -> Vec<u8> {
        let mut types = TypeSection::new();
        types.ty().function([], [ValType::I32]);
        types.ty().function([ValType::I32], []);
        types.ty().function(
            [ValType::I64],
            [ValType::I32, ValType::I32, ValType::I32, ValType::I64],
        );
        types.ty().function([], []);

        let mut imports = ImportSection::new();
        let mut func_imports = 0u32;
        for (module, name, ty) in &spec.imports {
            if matches!(ty, EntityType::Function(_)) {
                func_imports += 1;
            }
            imports.import(module, name, *ty);
        }

        // 関数: run(必ず)、helper、start の順。
        let mut functions = FunctionSection::new();
        functions.function(2);
        let run_index = func_imports;
        let mut next = func_imports + 1;
        let mut code = CodeSection::new();

        let mut run = Function::new([]);
        for instruction in &spec.body {
            run.instruction(instruction);
        }
        run.instruction(&Instruction::I32Const(0));
        run.instruction(&Instruction::I32Const(0));
        run.instruction(&Instruction::I32Const(0));
        run.instruction(&Instruction::I64Const(0));
        run.instruction(&Instruction::End);
        code.function(&run);

        let mut helper_index = None;
        if spec.helper {
            functions.function(3);
            let mut helper = Function::new([]);
            helper.instruction(&Instruction::End);
            code.function(&helper);
            helper_index = Some(next);
            next += 1;
        }
        let mut start_index = None;
        if spec.start {
            functions.function(3);
            let mut start = Function::new([]);
            start.instruction(&Instruction::End);
            code.function(&start);
            start_index = Some(next);
        }

        let mut tables = TableSection::new();
        if spec.table {
            tables.table(TableType {
                element_type: RefType::FUNCREF,
                table64: false,
                minimum: 1,
                maximum: None,
                shared: false,
            });
        }

        let mut memories = MemorySection::new();
        for memory in &spec.memories {
            memories.memory(*memory);
        }

        let mut globals = GlobalSection::new();
        if spec.global {
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: true,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
        }

        let mut exports = ExportSection::new();
        exports.export(EXPORT_RUN, ExportKind::Func, run_index);
        if spec.memory_export {
            exports.export(EXPORT_MEMORY, ExportKind::Memory, 0);
        }
        if let Some(index) = helper_index {
            exports.export("helper", ExportKind::Func, index);
        }
        for (name, kind, index) in &spec.extra_exports {
            exports.export(name, *kind, *index);
        }

        let mut module = Module::new();
        module.section(&types);
        module.section(&imports);
        module.section(&functions);
        if spec.table {
            module.section(&tables);
        }
        if !spec.memories.is_empty() {
            module.section(&memories);
        }
        if spec.global {
            module.section(&globals);
        }
        module.section(&exports);
        if let Some(function_index) = start_index {
            module.section(&StartSection { function_index });
        }
        module.section(&code);
        module.finish()
    }

    fn assert_rejected(spec: &Spec, expected: PolicyViolation) {
        let bytes = build(spec);
        assert_eq!(audit(&bytes), Err(expected));
    }

    #[test]
    fn conforming_module_is_accepted() {
        assert_eq!(audit(&build(&Spec::default())), Ok(()));
    }

    #[test]
    fn conforming_integer_operations_are_accepted() {
        let spec = Spec {
            body: vec![
                Instruction::I32Const(1),
                Instruction::I32Const(2),
                Instruction::I32Add,
                Instruction::Drop,
                Instruction::I64Const(3),
                Instruction::Drop,
            ],
            ..Spec::default()
        };
        assert_eq!(audit(&build(&spec)), Ok(()));
    }

    #[test]
    fn malformed_bytes_are_rejected() {
        assert!(audit(b"\0asm").is_err());
        assert!(audit(&[]).is_err());
    }

    #[test]
    fn extra_import_is_rejected() {
        let mut spec = Spec::default();
        spec.imports
            .push((IMPORT_MODULE, "extra", EntityType::Function(3)));
        assert_rejected(&spec, PolicyViolation::ImportSet);
    }

    #[test]
    fn import_with_wrong_signature_is_rejected() {
        let mut spec = Spec::default();
        spec.imports[0] = (IMPORT_MODULE, IMPORT_READ, EntityType::Function(1));
        assert_rejected(&spec, PolicyViolation::ImportSet);
    }

    #[test]
    fn missing_write_import_is_rejected() {
        let mut spec = Spec::default();
        spec.imports.truncate(1);
        assert_rejected(&spec, PolicyViolation::ImportSet);
    }

    #[test]
    fn import_from_other_module_is_rejected() {
        let mut spec = Spec::default();
        spec.imports[1] = ("wasi", IMPORT_WRITE, EntityType::Function(1));
        assert_rejected(&spec, PolicyViolation::ImportSet);
    }

    #[test]
    fn memory_import_is_rejected_as_memory() {
        let mut spec = Spec::default();
        spec.imports.push((
            IMPORT_MODULE,
            "mem",
            EntityType::Memory(conforming_memory()),
        ));
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn table_import_is_rejected_as_table() {
        let mut spec = Spec::default();
        spec.imports.push((
            IMPORT_MODULE,
            "tbl",
            EntityType::Table(TableType {
                element_type: RefType::FUNCREF,
                table64: false,
                minimum: 1,
                maximum: None,
                shared: false,
            }),
        ));
        assert_rejected(&spec, PolicyViolation::Table);
    }

    #[test]
    fn global_import_is_rejected_as_global() {
        let mut spec = Spec::default();
        spec.imports.push((
            IMPORT_MODULE,
            "glob",
            EntityType::Global(GlobalType {
                val_type: ValType::I32,
                mutable: false,
                shared: false,
            }),
        ));
        assert_rejected(&spec, PolicyViolation::Global);
    }

    #[test]
    fn memory_with_two_pages_is_rejected() {
        let mut spec = Spec::default();
        spec.memories = vec![MemoryType {
            minimum: 2,
            maximum: Some(2),
            ..conforming_memory()
        }];
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn growable_memory_without_maximum_is_rejected() {
        let mut spec = Spec::default();
        spec.memories = vec![MemoryType {
            maximum: None,
            ..conforming_memory()
        }];
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn memory_with_larger_maximum_is_rejected() {
        let mut spec = Spec::default();
        spec.memories = vec![MemoryType {
            minimum: 1,
            maximum: Some(2),
            ..conforming_memory()
        }];
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn shared_memory_is_rejected() {
        let mut spec = Spec::default();
        spec.memories = vec![MemoryType {
            shared: true,
            ..conforming_memory()
        }];
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn memory64_is_rejected() {
        let mut spec = Spec::default();
        spec.memories = vec![MemoryType {
            memory64: true,
            ..conforming_memory()
        }];
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn two_memories_are_rejected() {
        let mut spec = Spec::default();
        spec.memories = vec![conforming_memory(), conforming_memory()];
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn no_memory_is_rejected() {
        let mut spec = Spec::default();
        spec.memories = Vec::new();
        spec.memory_export = false;
        assert_rejected(&spec, PolicyViolation::Memory);
    }

    #[test]
    fn table_definition_is_rejected() {
        let mut spec = Spec::default();
        spec.table = true;
        assert_rejected(&spec, PolicyViolation::Table);
    }

    #[test]
    fn global_definition_is_rejected() {
        let mut spec = Spec::default();
        spec.global = true;
        assert_rejected(&spec, PolicyViolation::Global);
    }

    #[test]
    fn exported_global_is_rejected_as_global() {
        let mut spec = Spec::default();
        spec.global = true;
        spec.extra_exports.push(("g", ExportKind::Global, 0));
        assert_rejected(&spec, PolicyViolation::Global);
    }

    #[test]
    fn start_function_is_rejected() {
        let mut spec = Spec::default();
        spec.start = true;
        assert_rejected(&spec, PolicyViolation::Start);
    }

    #[test]
    fn extra_function_export_is_rejected() {
        let mut spec = Spec::default();
        spec.helper = true;
        assert_rejected(&spec, PolicyViolation::ExportSet);
    }

    #[test]
    fn extra_memory_export_is_rejected() {
        let mut spec = Spec::default();
        spec.extra_exports.push(("memory2", ExportKind::Memory, 0));
        assert_rejected(&spec, PolicyViolation::ExportSet);
    }

    #[test]
    fn memory_grow_in_body_is_rejected() {
        let mut spec = Spec::default();
        spec.body = vec![
            Instruction::I32Const(0),
            Instruction::MemoryGrow(0),
            Instruction::Drop,
        ];
        assert_rejected(&spec, PolicyViolation::MemoryGrow);
    }

    #[test]
    fn call_indirect_in_body_is_rejected() {
        let mut spec = Spec::default();
        spec.table = true;
        spec.body = vec![
            Instruction::I32Const(0),
            Instruction::CallIndirect {
                type_index: 3,
                table_index: 0,
            },
        ];
        assert_rejected(&spec, PolicyViolation::IndirectCall);
    }

    #[test]
    fn return_call_indirect_in_body_is_rejected() {
        let mut spec = Spec::default();
        spec.table = true;
        spec.body = vec![
            Instruction::I64Const(0),
            Instruction::I32Const(0),
            Instruction::ReturnCallIndirect {
                type_index: 2,
                table_index: 0,
            },
        ];
        assert_rejected(&spec, PolicyViolation::IndirectCall);
    }

    #[test]
    fn f32_constant_in_body_is_rejected() {
        let mut spec = Spec::default();
        spec.body = vec![Instruction::F32Const(1.0f32.into()), Instruction::Drop];
        assert_rejected(&spec, PolicyViolation::FloatingPoint);
    }

    #[test]
    fn f64_constant_in_body_is_rejected() {
        let mut spec = Spec::default();
        spec.body = vec![Instruction::F64Const(1.0f64.into()), Instruction::Drop];
        assert_rejected(&spec, PolicyViolation::FloatingPoint);
    }

    #[test]
    fn float_conversion_in_body_is_rejected() {
        let mut spec = Spec::default();
        spec.body = vec![
            Instruction::I32Const(0),
            Instruction::F32ConvertI32S,
            Instruction::I32TruncF32S,
            Instruction::Drop,
        ];
        assert_rejected(&spec, PolicyViolation::FloatingPoint);
    }
}
