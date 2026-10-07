import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { CaptureOverlay } from "./components/CaptureOverlay";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {new URLSearchParams(location.search).get("capture") ? (
      <CaptureOverlay
        sessionId={new URLSearchParams(location.search).get("capture")!}
      />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
