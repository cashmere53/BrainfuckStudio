// examples/hello.bf をビルド時に文字列として取り込む(複製しない。vite.config.ts の fs.allow で参照を許可)
import helloSource from "../../../../examples/hello.bf?raw";

export function getHelloSource(): string {
  return helloSource;
}
