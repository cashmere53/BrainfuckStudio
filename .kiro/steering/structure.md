# Project Structure

## Organization Philosophy

責務ごとに crate/アプリを分離した Cargo workspace + 独立フロントエンド。依存は一方向(UI/CLI → core)で、core は他に依存しない。

## Directory Patterns

### Rust crates
**Location**: `crates/<name>/`  
**Purpose**: ライブラリ/バイナリ crate。`brainfuck-core`(`src/{lib,instruction,parser,machine,error}.rs` + `tests/`)、`brainfuck-cli`(`src/main.rs`)

### Crate の一覧(依存は一方向)
- `brainfuck-core`: 実行エンジン。debugger-state のモジュール(`breakpoint`/`cancel`/`view`/`source_map`/`session`)も同 crate 内に追加される
- `brainfuck-cli`: CLI
- `brainfuck-optimized`: IR と最適化インタプリタ(core のみに依存)
- `brainfuck-gui-api`: Tauri 非依存の DTO と `DebuggerService`(GUI 用 API)
- `brainfuck-wasm` / `brainfuck-compare`: WASM バックエンドと方式比較(compiler-comparison)
- `brainfuck-compare-api`: 比較用の DTO と `ComparisonService`(compiler-comparison)。`brainfuck-compare` と `brainfuck-gui-api` に一方向で依存し、`brainfuck-gui-api` を WASM 系の重い依存から隔離する
- 依存方向: core ← optimized ← wasm ← compare ← compare-api ← `apps/desktop/src-tauri`、core ← gui-api ← compare-api(`src-tauri` は gui-api と compare-api の双方に依存。gui-api は compare/wasm/optimized に依存しない)

### Desktop app
**Location**: `apps/desktop/`  
**Purpose**: Tauri 2 + React + TypeScript + Vite(Phase 2 以降)。`src-tauri` はルート workspace の `members` に含めない独立 workspace(`brainfuck-gui-api` へ path 依存)

### Examples & docs
**Location**: `examples/`, `docs/`  
**Purpose**: サンプル `.bf`(例: `hello.bf` は `Hello World!\n` を出力)と文書

## Naming Conventions

- **Rust crates**: kebab-case(`brainfuck-core`)、モジュール/関数は snake_case、型は PascalCase
- **Brainfuck サンプル/回帰**: 拡張子 `.bf`、回帰は最小プログラム + 対応テスト
- **TypeScript**: コンポーネント PascalCase、その他は慣習に従う

## Code Organization Principles

- `brainfuck-core` の公開 API(`Program` / `Machine` / `MachineSnapshot` / `StopReason`)が全 spec の契約。拡張は後方互換で行う
- 命令添字 ↔ ソース範囲の対応は core が保持し、UI は変換結果を使う
- フロントは Rust 内部型を複製せず、JSON シリアライズ可能な DTO を介す
- 最適化/コンパイラ実装は core の意味論を変更せず、別実装として追加する
- 仕様は `.kiro/specs/`、プロジェクト規約は `.kiro/steering/`(分解は `roadmap.md`)

---
_Document patterns, not file trees. New files following patterns shouldn't require updates_
