import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useWorkspace, workspaceStore } from "../state/workspace";
import { App } from "./App";
import { screens, type ScreenDefinition } from "./screens";

describe("画面レジストリ(要件 1.1・12.14)", () => {
  it("既定の登録はデバッガ画面の 1 件だけ", () => {
    expect(screens.map((screen) => screen.id)).toEqual(["debugger"]);
  });
});

describe("App シェル", () => {
  it("画面が 1 つの間はタブを出さず、デバッガ画面を表示する", () => {
    render(<App />);

    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByRole("toolbar", { name: "操作" })).toBeTruthy();
  });

  it("画面が複数あるときだけタブを出し、選んだ画面へ切り替える", () => {
    const extra: readonly ScreenDefinition[] = [
      { id: "first", title: "最初", component: () => <p>最初の画面</p> },
      { id: "second", title: "次", component: () => <p>次の画面</p> },
    ];
    render(<App screens={extra} />);

    expect(screen.getByRole("tablist")).toBeTruthy();
    expect(screen.getByText("最初の画面")).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "次" }));

    expect(screen.getByText("次の画面")).toBeTruthy();
    expect(screen.queryByText("最初の画面")).toBeNull();
  });

  it("後続の画面はデバッガと同じ Workspace を共有する(別の画面で変えた半径がデバッガ画面に反映される)", () => {
    const initialRadius = workspaceStore.getSnapshot().tapeRadius;
    function Sibling() {
      const snapshot = useWorkspace(workspaceStore);
      return (
        <div>
          <p>後続の半径 {snapshot.tapeRadius}</p>
          <button type="button" onClick={() => workspaceStore.setTapeRadius(3)}>
            半径を 3 にする
          </button>
        </div>
      );
    }
    try {
      render(<App screens={[...screens, { id: "sibling", title: "後続", component: Sibling }]} />);

      fireEvent.click(screen.getByRole("tab", { name: "後続" }));
      fireEvent.click(screen.getByRole("button", { name: "半径を 3 にする" }));
      expect(screen.getByText("後続の半径 3")).toBeTruthy();

      fireEvent.click(screen.getByRole("tab", { name: "デバッガ" }));
      expect((screen.getByLabelText("表示半径") as HTMLSelectElement).value).toBe("3");
    } finally {
      act(() => workspaceStore.setTapeRadius(initialRadius));
    }
  });
});
