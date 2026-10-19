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
import { useStudio } from "./store";
import "./styles.css";
import "./studio.css";

// Register before React mounts, including browser previews and detached routes.
// Cancel the default action without stopping our piano/menu handlers.
document.addEventListener("contextmenu", (event) => event.preventDefault(), { capture: true });

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

ReactDOM.createRoot(root).render(<React.StrictMode>{body}</React.StrictMode>);
