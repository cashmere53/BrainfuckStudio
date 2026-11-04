import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import "./styles/global.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("root 要素が見つかりません");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
