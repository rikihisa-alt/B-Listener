import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import "./styles/global.css";
import "./components/ui/ui.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("ルート要素が見つかりません");
}

ReactDOM.createRoot(container).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
