// テスト用の台本の機械。`createFakeBackend` に渡す FakeScript を作り、実サービス(brainfuck-gui-api の
// DebuggerService)と同じ応答規則で動く。実 CLI は使わない(9.3 でバックエンド側を検証済み)。
//
// 再現する範囲(少数のプログラムに限る。核の意味論の完全な再実装ではない):
// - 命令: > < + - . , [ ]。テープ 30,000 セル(0..=255 の折り返し)、左端・右端の範囲外は pointerOutOfBounds
// - 入力: 尽きていれば 0 を入れて読取位置は進めない。出力は現在セルの 1 バイト
// - 停止: 正常終了(halted)・ブレークポイント・一時停止(cancelled)・バッチ到達(batchReached)・実行エラー
//   ブレークポイントの再開規則は brainfuck-core の `run_batch` と同じ(停止した位置は次の 1 命令だけ照合しない)
// - Step は core の `step_into` と同じ(実行後は次の Run でブレークポイントを照合しない)
// - 上限: 命令数の上限到達で stepLimitExceeded(pc は試行時点)。上限なしは null
// - 表示窓: 出力は末尾 65,536 バイトまで。`outputFrom` が総量以下なら max(outputFrom, total − 65,536)、
//   超えるなら max(0, total − 65,536)(convert.rs と同じ)
// - Run ループは一定命令数ごとに `onYield` を呼んでマイクロタスクへ譲る。タイマーは使わず、テストは機械の進行を
//   明示的に制御できる(一時停止を実行の途中で届ける台本に使う)
// - 位置は文書先頭からの UTF-16 単位・UTF-8 バイト・行(1 始まり)・列(スカラ値 + 1)で返す

import type { ApiError } from "../generated/ApiError";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { BatchStopDto } from "../generated/BatchStopDto";
import type { BreakpointsDto } from "../generated/BreakpointsDto";
import type { LoadSourceResult } from "../generated/LoadSourceResult";
import type { ParseErrorDto } from "../generated/ParseErrorDto";
import type { RuntimeErrorDto } from "../generated/RuntimeErrorDto";
import type { SessionStartedDto } from "../generated/SessionStartedDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { SourcePositionDto } from "../generated/SourcePositionDto";
import type { SourceRangeDto } from "../generated/SourceRangeDto";
import type { StepResultDto } from "../generated/StepResultDto";
import type { StopReasonDto } from "../generated/StopReasonDto";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import type { ViewRequestDto } from "../generated/ViewRequestDto";
import { decodeBase64, encodeBase64 } from "../domain/base64";
import type { FakeScript } from "../ipc/fakeBackend";

/** テープのセル数(brainfuck-core の TAPE_SIZE) */
export const TAPE_SIZE = 30_000;
/** 出力の保持上限(末尾バイト数) */
export const OUTPUT_TAIL_LIMIT = 65_536;
/** バッチ幅の既定と上限(GUI の既定 1,000、サービスの受理範囲 1..=1,000,000) */
const DEFAULT_BATCH = 1_000;
const MAX_BATCH = 1_000_000;
/** テープ窓の半径の上限 */
const MAX_RADIUS = 64;
/** ソースと入力の上限(バイト) */
const MAX_SOURCE_BYTES = 1_048_576;
const MAX_INPUT_BYTES = 1_048_576;
/** Run ループが譲る既定の間隔(命令数) */
const DEFAULT_YIELD_EVERY = 1_024;
/** 命令数上限の上限(2^53 − 1) */
const MAX_STEP_LIMIT = Number.MAX_SAFE_INTEGER;

type Instr = { op: string; offset: number; jump: number };

type Loaded = { programId: number; source: string; instrs: Instr[] };

type Session = {
  id: number;
  loaded: Loaded;
  input: Uint8Array;
  inputPos: number;
  limit: number | null;
  tape: Uint8Array;
  dp: number;
  pc: number;
  steps: number;
  output: number[];
  /** ブレークポイント(命令添字) */
  breakpoints: Set<number>;
  /** 直前の停止位置を次の 1 命令だけ照合しない(core の skip_breakpoint_once) */
  skipBreakpoint: boolean;
  lastStop: StopReasonDto | null;
  running: boolean;
  cancel: boolean;
};

type StepOutcome = { kind: "executed" } | { kind: "halted" } | { kind: "error"; error: RuntimeErrorDto };

/** 台本の機械の検査口(テストが内部の値を確かめるため。応答には出さない) */
export type MachineInspection = {
  pc: number;
  dp: number;
  steps: number;
  halted: boolean;
  running: boolean;
  output: number[];
  breakpoints: number[];
} | null;

export type MachineScript = {
  /** `createFakeBackend` に渡す台本 */
  script: FakeScript;
  /** 現在のセッションの内部の値(セッションが無ければ null) */
  inspect(): MachineInspection;
};

function apiError(kind: ApiError["kind"], message: string): ApiError {
  return { kind, message, field: null };
}

const OPS = "><+-.,[]";

/** UTF-16 位置 `utf16` の文書内位置(バイト・行・列を含む) */
function positionAt(source: string, utf16: number): SourcePositionDto {
  const prefix = source.slice(0, utf16);
  const lineStart = prefix.lastIndexOf("\n") + 1;
  return {
    byteOffset: new TextEncoder().encode(prefix).length,
    utf16Offset: utf16,
    line: prefix.split("\n").length,
    column: Array.from(prefix.slice(lineStart)).length + 1,
  };
}

function rangeOf(source: string, instr: Instr): SourceRangeDto {
  return { start: positionAt(source, instr.offset), end: positionAt(source, instr.offset + 1) };
}

/** 命令列へ解析する。括弧の不整合は解析エラーとして返す(先に見つかった方) */
function parseSource(source: string): { ok: true; instrs: Instr[] } | { ok: false; error: ParseErrorDto } {
  const instrs: Instr[] = [];
  const open: number[] = [];
  for (let i = 0; i < source.length; i += 1) {
    const ch = source.charAt(i);
    if (!OPS.includes(ch)) continue;
    const index = instrs.length;
    instrs.push({ op: ch, offset: i, jump: -1 });
    if (ch === "[") {
      open.push(index);
    } else if (ch === "]") {
      const matched = open.pop();
      if (matched === undefined) {
        return {
          ok: false,
          error: {
            kind: "unmatchedClosingBracket",
            instructionIndex: index,
            position: positionAt(source, i),
          },
        };
      }
      instrs[matched]!.jump = index;
      instrs[index]!.jump = matched;
    }
  }
  const unclosed = open[0];
  if (unclosed !== undefined) {
    return {
      ok: false,
      error: {
        kind: "unmatchedOpeningBracket",
        instructionIndex: unclosed,
        position: positionAt(source, instrs[unclosed]!.offset),
      },
    };
  }
  return { ok: true, instrs };
}

/** 1 命令を実行する(brainfuck-core の Machine::step と同じ判定順序: 停止 → 上限 → 実行) */
function executeOne(session: Session): StepOutcome {
  const { instrs } = session.loaded;
  if (session.pc >= instrs.length) return { kind: "halted" };
  if (session.limit !== null && session.steps >= session.limit) {
    return { kind: "error", error: { kind: "stepLimitExceeded", limit: session.limit, pc: session.pc } };
  }
  const instr = instrs[session.pc]!;
  switch (instr.op) {
    case "+":
      session.tape[session.dp] = (session.tape[session.dp]! + 1) & 0xff;
      break;
    case "-":
      session.tape[session.dp] = (session.tape[session.dp]! + 0xff) & 0xff;
      break;
    case ">":
      if (session.dp >= TAPE_SIZE - 1) {
        return {
          kind: "error",
          error: { kind: "pointerOutOfBounds", pc: session.pc, dp: session.dp, direction: "right" },
        };
      }
      session.dp += 1;
      break;
    case "<":
      if (session.dp === 0) {
        return {
          kind: "error",
          error: { kind: "pointerOutOfBounds", pc: session.pc, dp: session.dp, direction: "left" },
        };
      }
      session.dp -= 1;
      break;
    case ".":
      session.output.push(session.tape[session.dp]!);
      break;
    case ",":
      if (session.inputPos < session.input.length) {
        session.tape[session.dp] = session.input[session.inputPos]!;
        session.inputPos += 1;
      } else {
        session.tape[session.dp] = 0;
      }
      break;
    case "[":
      if (session.tape[session.dp] === 0) session.pc = instr.jump;
      break;
    case "]":
      if (session.tape[session.dp] !== 0) session.pc = instr.jump;
      break;
  }
  session.steps += 1;
  session.pc += 1;
  return { kind: "executed" };
}

/** ブレークポイント位置を命令へ解決する(位置以降で最初の命令。無ければ未解決) */
function resolveBreakpoints(
  loaded: Loaded,
  offsets: readonly number[],
): { dto: BreakpointsDto; indices: Set<number> } {
  const byIndex = new Map<number, number>();
  const unresolved = new Set<number>();
  for (const offset of offsets) {
    const index = loaded.instrs.findIndex((instr) => instr.offset >= offset);
    if (index < 0) {
      unresolved.add(offset);
    } else if (!byIndex.has(index)) {
      byIndex.set(index, offset);
    }
  }
  const sorted = [...byIndex.keys()].sort((a, b) => a - b);
  return {
    dto: {
      breakpoints: sorted.map((index) => ({
        utf16Offset: byIndex.get(index)!,
        instructionIndex: index,
        range: rangeOf(loaded.source, loaded.instrs[index]!),
      })),
      unresolved: [...unresolved].sort((a, b) => a - b),
    },
    indices: new Set(sorted),
  };
}

function tapeWindowOf(session: Session, radius: number): TapeWindowDto {
  if (!Number.isInteger(radius) || radius < 0 || radius > MAX_RADIUS) {
    throw apiError("invalidArgument", `tape radius out of range: ${radius}`);
  }
  const start = Math.max(0, session.dp - radius);
  const end = Math.min(TAPE_SIZE - 1, session.dp + radius);
  return {
    start,
    cells: Array.from(session.tape.subarray(start, end + 1)),
    pointerOffset: session.dp - start,
  };
}

function stateOf(session: Session, view: ViewRequestDto): SessionStateDto {
  const total = session.output.length;
  const dataStart =
    view.outputFrom <= total
      ? Math.max(view.outputFrom, total - OUTPUT_TAIL_LIMIT)
      : Math.max(0, total - OUTPUT_TAIL_LIMIT);
  const { instrs, source } = session.loaded;
  return {
    sessionId: session.id,
    snapshot: {
      pc: session.pc,
      dp: session.dp,
      currentCell: session.tape[session.dp]!,
      stepsExecuted: session.steps,
      halted: session.pc >= instrs.length,
    },
    tapeWindow: tapeWindowOf(session, view.tapeRadius),
    output: {
      dataStart,
      totalLen: total,
      dataBase64: encodeBase64(Uint8Array.from(session.output.slice(dataStart))),
    },
    inputConsumed: session.inputPos,
    inputTotal: session.input.length,
    lastStop: session.lastStop,
    currentRange: session.pc < instrs.length ? rangeOf(source, instrs[session.pc]!) : null,
  };
}

/** 譲りの設定。`onYield` は譲る直前に同期で呼ぶ(中で一時停止を発行すると、次の判定で止まる) */
export type MachineOptions = {
  /** 譲る間隔(命令数)。0 以下なら譲らない */
  yieldEvery?: number;
  onYield?: () => void;
};

/** バッチ実行(brainfuck-core の Session::run_batch と同じ判定順序: 停止 → 一時停止 → ブレークポイント → 幅 → 実行) */
async function runBatch(
  session: Session,
  size: number,
  pacing: Required<MachineOptions>,
): Promise<{ stop: BatchStopDto; executed: number }> {
  session.running = true;
  session.cancel = false;
  let executed = 0;
  let iterations = 0;
  const finish = (stop: BatchStopDto, last: StopReasonDto | null): { stop: BatchStopDto; executed: number } => {
    session.lastStop = last;
    return { stop, executed };
  };
  try {
    for (;;) {
      iterations += 1;
      if (pacing.yieldEvery > 0 && iterations % pacing.yieldEvery === 0) {
        pacing.onYield();
        await Promise.resolve();
      }
      if (session.pc >= session.loaded.instrs.length) {
        return finish({ kind: "halted" }, "halted");
      }
      if (session.cancel) {
        return finish({ kind: "cancelled" }, "cancelled");
      }
      if (!session.skipBreakpoint && session.breakpoints.has(session.pc)) {
        session.skipBreakpoint = true;
        return finish({ kind: "breakpoint" }, "breakpoint");
      }
      if (executed >= size) {
        return finish({ kind: "batchReached" }, "batchReached");
      }
      const outcome = executeOne(session);
      if (outcome.kind === "error") {
        return finish({ kind: "error", error: outcome.error }, null);
      }
      if (outcome.kind === "executed") {
        executed += 1;
        session.skipBreakpoint = false;
      }
    }
  } finally {
    session.running = false;
    session.cancel = false;
  }
}

/** 台本の機械を作る。セッションは 1 つだけ保持し、新しい開始は前のセッションを置き換える */
export function createMachineScript(options: MachineOptions = {}): MachineScript {
  const pacing: Required<MachineOptions> = {
    yieldEvery: options.yieldEvery ?? DEFAULT_YIELD_EVERY,
    onYield: options.onYield ?? (() => undefined),
  };
  let loaded: Loaded | null = null;
  let session: Session | null = null;
  let nextProgramId = 1;
  let nextSessionId = 1;

  // 要求のセッション ID が現在のセッションと一致することを確かめる
  const requireSession = (sessionId: number): Session => {
    if (session === null || session.id !== sessionId) {
      throw apiError("sessionNotFound", `session ${sessionId} is not current`);
    }
    return session;
  };
  // 実行中に変えてはならない操作(フロントは実行中の編集を禁じるため、ここに来たら試験の書き方の誤り)
  const requireIdle = (sessionId: number): Session => {
    const current = requireSession(sessionId);
    if (current.running) {
      throw apiError("internal", "fake machine: command issued while the batch is running");
    }
    return current;
  };

  const script: FakeScript = {
    loadSource: (req): LoadSourceResult => {
      if (new TextEncoder().encode(req.source).length > MAX_SOURCE_BYTES) {
        throw apiError("sourceTooLarge", "source exceeds 1 MiB");
      }
      const parsed = parseSource(req.source);
      if (!parsed.ok) {
        // 解析エラーではプログラムは登録されない(直前に読み込んだプログラムは残る)
        return { kind: "parseError", error: parsed.error };
      }
      loaded = { programId: nextProgramId, source: req.source, instrs: parsed.instrs };
      nextProgramId += 1;
      return {
        kind: "ok",
        programId: loaded.programId,
        instructionCount: parsed.instrs.length,
        lineCount: req.source.split("\n").length,
      };
    },

    startSession: (req): SessionStartedDto => {
      if (loaded === null) throw apiError("programNotLoaded", "no program loaded");
      if (req.programId !== loaded.programId) throw apiError("staleProgram", "program was replaced");
      const input = decodeBase64(req.inputBase64);
      if (!input.ok) throw apiError("invalidArgument", "input is not valid base64");
      if (input.bytes.length > MAX_INPUT_BYTES) throw apiError("inputTooLarge", "input exceeds 1 MiB");
      const limit = req.limit.kind === "limited" ? req.limit.maxSteps : null;
      if (limit !== null && (!Number.isInteger(limit) || limit < 1 || limit > MAX_STEP_LIMIT)) {
        throw apiError("invalidArgument", "step limit out of range");
      }
      const resolved = resolveBreakpoints(loaded, req.breakpointOffsets);
      const created: Session = {
        id: nextSessionId,
        loaded,
        input: input.bytes,
        inputPos: 0,
        limit,
        tape: new Uint8Array(TAPE_SIZE),
        dp: 0,
        pc: 0,
        steps: 0,
        output: [],
        breakpoints: resolved.indices,
        skipBreakpoint: false,
        lastStop: null,
        running: false,
        cancel: false,
      };
      nextSessionId += 1;
      session = created;
      return { state: stateOf(created, req.view), breakpoints: resolved.dto };
    },

    runBatch: async (req): Promise<BatchResultDto> => {
      const current = requireIdle(req.sessionId);
      const size = req.batchSize ?? DEFAULT_BATCH;
      if (!Number.isInteger(size) || size < 1 || size > MAX_BATCH) {
        throw apiError("invalidArgument", "batch size out of range");
      }
      const result = await runBatch(current, size, pacing);
      return { stop: result.stop, executed: result.executed, state: stateOf(current, req.view) };
    },

    step: (req): StepResultDto => {
      const current = requireIdle(req.sessionId);
      const outcome = executeOne(current);
      if (outcome.kind === "halted") {
        current.lastStop = "halted";
        return { outcome: { kind: "halted" }, state: stateOf(current, req.view) };
      }
      if (outcome.kind === "error") {
        current.lastStop = null;
        return { outcome, state: stateOf(current, req.view) };
      }
      current.skipBreakpoint = true;
      current.lastStop = current.pc >= current.loaded.instrs.length ? "halted" : null;
      return { outcome: { kind: "executed" }, state: stateOf(current, req.view) };
    },

    reset: (req): SessionStateDto => {
      const current = requireIdle(req.sessionId);
      // 機械を初期状態へ戻す。ブレークポイントは保持する
      current.tape = new Uint8Array(TAPE_SIZE);
      current.dp = 0;
      current.pc = 0;
      current.steps = 0;
      current.output = [];
      current.inputPos = 0;
      current.lastStop = null;
      current.skipBreakpoint = false;
      return stateOf(current, req.view);
    },

    getSnapshot: (req): SessionStateDto => stateOf(requireSession(req.sessionId), req.view),

    getTapeWindow: (req): TapeWindowDto => tapeWindowOf(requireSession(req.sessionId), req.radius),

    setBreakpoints: (req): BreakpointsDto => {
      const current = requireIdle(req.sessionId);
      const resolved = resolveBreakpoints(current.loaded, req.offsets);
      current.breakpoints = resolved.indices;
      return resolved.dto;
    },

    // 一時停止: 実行中の batch だけを止める。実行中でなければ無視する(core の cancel 規則)
    pause: (req): void => {
      if (session !== null && session.id === req.sessionId && session.running) {
        session.cancel = true;
      }
    },

    newDocument: () => undefined,
    openSourceFile: () => ({ kind: "cancelled" }),
    saveSourceFile: () => ({ kind: "cancelled" }),
    setDocumentDirty: () => undefined,
    confirmExit: () => undefined,
  };

  return {
    script,
    inspect(): MachineInspection {
      if (session === null) return null;
      return {
        pc: session.pc,
        dp: session.dp,
        steps: session.steps,
        halted: session.pc >= session.loaded.instrs.length,
        running: session.running,
        output: [...session.output],
        breakpoints: [...session.breakpoints].sort((a, b) => a - b),
      };
    },
  };
}
