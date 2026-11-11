import React from "react";
import ReactDOM from "react-dom/client";
import "./tokens/tokens.css";
import "./kit/kit.css";
import { HintProvider } from "./kit/hint";
import { Gallery } from "./gallery/Gallery";

// New UI entry. Phase 0 shows only the control gallery; the shell arrives in Phase 1.
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <HintProvider>
      <Gallery />
    </HintProvider>
  </React.StrictMode>,
);
