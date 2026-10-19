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

const label = docLabelFromUrl();
if (label !== "main") {
  useStudio.setState({ docLabel: label });
}

/** SFM-009: share note clipboard across Genos windows. */
void listenClipboardSync((p) => {
  useStudio.setState({ clipboard: p.notes });
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
