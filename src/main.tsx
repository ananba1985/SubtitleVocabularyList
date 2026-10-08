import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { CaptureOverlay } from "./components/CaptureOverlay";
import "./styles.css";

class AppBoundary extends React.Component<
  { children: React.ReactNode },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(error: Error) {
    console.error("Application rendering failed", error);
  }
  render() {
    return this.state.failed ? (
      <div className="startup-view" role="alert">
        <h1>单词本暂时无法显示</h1>
        <p>请重新加载；你的已保存资料仍然保留。</p>
        <button onClick={() => location.reload()}>重新加载</button>
      </div>
    ) : (
      this.props.children
    );
  }
}

function ReadyView() {
  React.useEffect(() => {
    window.dispatchEvent(new Event("svl-app-ready"));
  }, []);
  return new URLSearchParams(location.search).get("capture") ? (
    <CaptureOverlay
      sessionId={new URLSearchParams(location.search).get("capture")!}
    />
  ) : (
    <App />
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AppBoundary>
      <ReadyView />
    </AppBoundary>
  </React.StrictMode>,
);
