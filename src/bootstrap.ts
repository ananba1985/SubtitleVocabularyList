const started = performance.now();
const capture = new URLSearchParams(location.search).has("capture");
if (capture) {
  document.body.style.background = "transparent";
  document.getElementById("root")?.replaceChildren();
}

function reportStartup(message: string) {
  const status = document.getElementById("startup-status");
  if (!status) return;
  status.textContent = message;
  const actions = document.getElementById("startup-actions");
  if (actions && !actions.childElementCount) {
    const retry = document.createElement("button");
    retry.textContent = "重新加载";
    retry.addEventListener("click", () => location.reload());
    actions.append(retry);
  }
}

const timer = window.setTimeout(() => {
  reportStartup("启动仍在进行，请稍候；长时间没有变化时可重新加载。");
}, 10000);
window.addEventListener(
  "svl-app-ready",
  () => {
    clearTimeout(timer);
    if (import.meta.env.DEV)
      console.info(
        `[startup] first view: ${Math.round(performance.now() - started)} ms`,
      );
  },
  { once: true },
);
import("./main")
  .then(() => {
    if (import.meta.env.DEV)
      console.info(
        `[startup] modules loaded: ${Math.round(performance.now() - started)} ms`,
      );
  })
  .catch((error: unknown) => {
    clearTimeout(timer);
    console.error("Application module loading failed", error);
    reportStartup("启动未完成，请重新加载。你的本地资料仍然保留。");
  });
