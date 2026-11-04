//! Tauri 非依存の GUI 用 API 層。DTO・制限値・コーデック・サービスを提供する。
//!
//! 依存は一方向(`brainfuck-core` ← 本 crate)。UI・ファイル I/O・スレッド生成には依存しない。

pub mod codec;
pub mod convert;
pub mod dto;
pub mod limits;
pub mod service;
pub mod source_file;
