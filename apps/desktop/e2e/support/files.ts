// テスト用のファイルの補助。各関数は呼び出し側が渡したディレクトリ(通常は testInfo.outputPath)の下に
// ファイルを作り、その絶対パスを返す。ファイルの中身はバイト列で固定し、改行コードや文字コードを
// エディタの読み書きの検証に使う。
import { accessSync, constants, chmodSync, unlinkSync, writeFileSync } from "node:fs";
import path from "node:path";

const ONE_MIB = 1024 * 1024;

/** 改行コードが CRLF のソース("+\r\n+\r\n.")。開いて保存したときに CRLF が保たれることを見る */
export function writeCrlfSource(dir: string): string {
  const file = path.join(dir, "crlf.bf");
  writeFileSync(file, Buffer.from("+\r\n+\r\n.", "latin1"));
  return file;
}

/** UTF-8 として不正なバイト列を含むソース(Shift_JIS の「テスト」+ 命令)。不正なバイトが置換文字で表示される */
export function writeShiftJisSource(dir: string): string {
  const file = path.join(dir, "shift-jis.bf");
  const sjisText = Buffer.from([0x83, 0x65, 0x83, 0x58, 0x83, 0x67]); // テ ス ト
  writeFileSync(file, Buffer.concat([sjisText, Buffer.from("\n+.", "latin1")]));
  return file;
}

/** 1 MiB を 1 バイト超えるファイル。読み込みが上限超過で拒否されることを見る */
export function writeOversizedSource(dir: string): string {
  const file = path.join(dir, "oversized.bf");
  writeFileSync(file, Buffer.alloc(ONE_MIB + 1, "+"));
  return file;
}

/**
 * 読み取り専用のファイル。書き込めてしまう環境(root で動く環境など)では、読み取り専用の意味が無いため
 * ファイルを作らずに null を返す。呼び出し側は理由つきで skip する。
 */
export function writeReadOnlyFile(dir: string): string | null {
  // POSIX の root は読み取り専用のファイルにも書けるため、属性では拒否されない
  if (process.getuid?.() === 0) return null;

  const file = path.join(dir, "read-only.bf");
  writeFileSync(file, "+.");
  chmodSync(file, 0o444);
  try {
    // 属性が効いていれば書き込みの権限が無い。効いていなければ(書けてしまう環境)は作ったファイルを消して null
    accessSync(file, constants.W_OK);
    unlinkSync(file);
    return null;
  } catch {
    return file;
  }
}

/** 存在しないディレクトリの下のパス(保存先に指定して、原因つきのエラーを確かめる)。ディレクトリは作らない */
export function missingDirectoryPath(dir: string): string {
  return path.join(dir, "no-such-directory", "out.bf");
}
