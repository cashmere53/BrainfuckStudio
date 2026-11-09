//! デバッガ API クレート(`brainfuck-gui-api`)の依存木に、比較・WASM 生成と実行に関わるクレートが
//! 含まれないことを確認する(要件 1.8)。
//!
//! 行単位の宣言の文字列処理では、rename(`package = "..."`)、見出し形式(`[dependencies.x]`)、
//! 引用符付きキー、インラインテーブルを取りこぼすため、`cargo metadata --locked` の解決グラフを使う。
//! `brainfuck-gui-api` のノードから推移的に到達できるパッケージを、パッケージ名(`package.name`)で集める。
//!
//! 依存の種類について: 通常・dev・build のすべての辺をたどる(`resolve` の `dependencies` は種類を
//! 区別せず全辺を持つ)。禁止対象の混入は種類を問わず許さない方が安全なため、dev/build も含める。

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::process::Command;

use serde_json::Value;

const DEBUGGER_API: &str = "brainfuck-gui-api";

const FORBIDDEN: &[&str] = &[
    "brainfuck-compare",
    "brainfuck-wasm",
    "brainfuck-optimized",
    "wasmi",
    "wasm-encoder",
    "wasmparser",
];

/// `cargo metadata --format-version 1 --locked` を本クレートのマニフェストで実行し、JSON を返す。
/// 実行の失敗・JSON の不正はテストの失敗にする。
fn cargo_metadata() -> Value {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .unwrap_or_else(|e| panic!("failed to start cargo metadata: {e}"));
    assert!(
        output.status.success(),
        "cargo metadata --locked failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata output is not valid JSON")
}

/// `root_name` のパッケージから推移的に到達できる、依存パッケージ名(`package.name`)の集合。
/// 根のパッケージ自身は含めない。
fn transitive_dependency_names(meta: &Value, root_name: &str) -> BTreeSet<String> {
    let packages = meta["packages"]
        .as_array()
        .expect("metadata has no packages array");
    let id_to_name: BTreeMap<&str, &str> = packages
        .iter()
        .map(|p| {
            (
                p["id"].as_str().expect("package id"),
                p["name"].as_str().expect("package name"),
            )
        })
        .collect();

    let nodes = meta["resolve"]["nodes"]
        .as_array()
        .expect("metadata has no resolve nodes (is this a workspace?)");
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in nodes {
        let id = node["id"].as_str().expect("node id");
        let deps = node["dependencies"]
            .as_array()
            .expect("node dependencies")
            .iter()
            .map(|d| d.as_str().expect("dependency id"))
            .collect();
        edges.insert(id, deps);
    }

    let roots: Vec<&str> = id_to_name
        .iter()
        .filter(|(_, name)| **name == root_name)
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        roots.len(),
        1,
        "expected exactly one package named {root_name}"
    );
    let root = roots[0];

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::from([root]);
    seen.insert(root);
    while let Some(id) = queue.pop_front() {
        for next in edges.get(id).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }

    seen.remove(root);
    seen.iter()
        .map(|id| {
            id_to_name
                .get(id)
                .unwrap_or_else(|| panic!("resolve node {id} has no package entry"))
                .to_string()
        })
        .collect()
}

#[test]
fn debugger_api_dependency_tree_has_no_comparison_or_wasm_crate() {
    let meta = cargo_metadata();
    let names = transitive_dependency_names(&meta, DEBUGGER_API);

    // 集合が空でないこと(検査が空振りしていないこと)の確認。
    assert!(
        names.contains("brainfuck-core") && names.contains("serde"),
        "dependency set of {DEBUGGER_API} looks wrong (expected brainfuck-core and serde): {names:?}"
    );

    for forbidden in FORBIDDEN {
        assert!(
            !names.contains(*forbidden),
            "{DEBUGGER_API} must not depend (directly or transitively) on {forbidden} (requirement 1.8). \
             resolved dependencies: {names:?}"
        );
    }
}
