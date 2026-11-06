import "./App.css";
import { useState } from "react";
import { screens as defaultScreens, type ScreenDefinition } from "./screens";

export interface AppProps {
  /** 表示する画面の一覧。既定はアプリの画面レジストリ */
  screens?: readonly ScreenDefinition[];
}

/**
 * アプリシェル(要件 1.1)。画面レジストリを描画し、画面が 2 つ以上あるときだけタブで切り替える。
 * 画面が 1 つの間はタブを出さず、その画面をそのまま表示する。
 */
export function App({ screens = defaultScreens }: AppProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const active = screens.find((screen) => screen.id === selectedId) ?? screens[0];

  if (active === undefined) {
    return <div className="app-shell" />;
  }

  const ActiveScreen = active.component;
  return (
    <div className="app-shell">
      {screens.length > 1 ? (
        <div className="app-tabs" role="tablist" aria-label="画面">
          {screens.map((screen) => (
            <button
              key={screen.id}
              type="button"
              role="tab"
              aria-selected={screen.id === active.id}
              className="app-tab"
              onClick={() => setSelectedId(screen.id)}
            >
              {screen.title}
            </button>
          ))}
        </div>
      ) : null}
      <ActiveScreen />
    </div>
  );
}
