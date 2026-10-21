import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { DetachedPiano } from "./DetachedPiano";
import { DetachedMixer } from "./DetachedMixer";
import {
  docLabelFromUrl,
  isDetachedMixerRoute,
  isDetachedPianoRoute,
  listenClipboardSync,
} from "./detach";
import { ConfirmHost } from "./ConfirmHost";
import { useStudio } from "./store";
import { loadInstalledPacks } from "./installedPacks";
import "./designTokens.css";
import "./styles.css";
import "./studio.css";
import "./flChrome.css";
import "./flDialog.css";
import "./flButtons.css";
import "./flInspector.css";

// Register before React mounts, including browser previews and detached routes.
// Cancel the default action without stopping our piano/menu handlers.
document.addEventListener("contextmenu", (event) => event.preventDefault(), {
  capture: true,
});

// A desktop app, not a web page: refresh shortcuts would reload the webview, drop the open project
// and replay the start-up splash.
document.addEventListener(
  "keydown",
  (event) => {
    const reload =
      event.key === "F5" ||
      ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "r");
    if (reload) event.preventDefault();
  },
  { capture: true },
);

const label = docLabelFromUrl();
if (label !== "main") {
  useStudio.setState({ docLabel: label });
}

/** SFM-009: share note clipboard across Genos windows. */
void listenClipboardSync((p) => {
  useStudio.setState({ clipboard: p.notes, clipboardPpq: p.ppq });
});

const root = document.getElementById("root")!;
const body = isDetachedPianoRoute() ? (
  <DetachedPiano />
) : isDetachedMixerRoute() ? (
  <DetachedMixer />
) : (
  <App />
);

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    {body}
    <ConfirmHost />
  </React.StrictMode>,
);

// Installed expansion-pack voices and kits (remembered from the last import) join the voice list.
void loadInstalledPacks();

// Let the start-up splash finish its animation, then fade it away over the mounted UI.
requestAnimationFrame(() =>
  requestAnimationFrame(() => window.__genosSplash?.dismiss()),
);
