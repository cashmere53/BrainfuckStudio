//! 比較用 API 層(`brainfuck-compare-api`)。比較の DTO・要求検証・サービスを提供する。
//!
//! デバッガ API(`brainfuck-gui-api`)を重い依存から隔離するため独立したクレートに置く。
//! 依存は一方向: 本クレート → `brainfuck-compare` / `brainfuck-gui-api` / `brainfuck-core`。
//! `brainfuck-gui-api` は本クレートに依存しない。

#![forbid(unsafe_code)]

pub mod dto;
