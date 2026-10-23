import { useEffect, useRef, useState } from "react";
import { call, message } from "../api";
import type { CommandResult } from "../commands";

export function useVideoLearning(
  state: CommandResult<"video_state"> | null,
  ready: boolean,
  blocked: boolean,
) {
  const [learning, setLearning] =
    useState<CommandResult<"video_learning_get"> | null>(null);
  const [error, setError] = useState("");
  const card = useRef<HTMLElement>(null);
  const latest = useRef({ state, ready, blocked, learning });
  latest.current = { state, ready, blocked, learning };
  const painted = useRef(0),
    reading = useRef(false),
    active = useRef(false);

  function visible() {
    const current = latest.current,
      context = current.learning?.context;
    const text = card.current?.querySelector("[data-translation]");
    if (
      !text ||
      !context?.translation ||
      current.blocked ||
      !current.ready ||
      !current.state?.paused ||
      !current.state.foreground ||
      !current.state.windowFocused ||
      current.state.fullscreen ||
      painted.current !== context.revision ||
      document.visibilityState !== "visible" ||
      document.querySelector(".mobile-learning-options[open]") ||
      text?.textContent?.trim() !== context.translation.trim()
    )
      return false;
    const rect = text.getBoundingClientRect();
    const bottom =
      document.querySelector(".mobile-navigation")?.getBoundingClientRect()
        .top ?? innerHeight;
    return (
      rect.width > 0 &&
      rect.height > 0 &&
      rect.top >= 0 &&
      rect.bottom <= Math.min(bottom, visualViewport?.height ?? innerHeight)
    );
  }
  async function refresh() {
    const id = latest.current.state?.sessionId;
    if (!active.current || !id || !latest.current.ready || reading.current)
      return;
    reading.current = true;
    try {
      const context = latest.current.learning?.context;
      if (context)
        await call("video_learning_visible", {
          revision: context.revision,
          visible: visible(),
        });
      const next = await call("video_learning_get");
      if (
        active.current &&
        id === latest.current.state?.sessionId &&
        next.sessionId === id
      )
        setLearning(next);
    } catch (e) {
      if (active.current) setError(message(e));
    } finally {
      reading.current = false;
    }
  }
  useEffect(() => {
    active.current = true;
    setLearning(null);
    setError("");
    painted.current = 0;
    const timer = window.setInterval(refresh, 400);
    void refresh();
    const interrupt = () => {
      const context = latest.current.learning?.context;
      if (context && !visible())
        void call("video_learning_visible", {
          revision: context.revision,
          visible: false,
        }).catch(() => {});
    };
    window.addEventListener("blur", interrupt);
    window.addEventListener("scroll", interrupt, true);
    document.addEventListener("visibilitychange", interrupt);
    return () => {
      active.current = false;
      window.clearInterval(timer);
      window.removeEventListener("blur", interrupt);
      window.removeEventListener("scroll", interrupt, true);
      document.removeEventListener("visibilitychange", interrupt);
      const context = latest.current.learning?.context;
      if (context)
        void call("video_learning_visible", {
          revision: context.revision,
          visible: false,
        }).catch(() => {});
    };
  }, [state?.sessionId, ready]);
  useEffect(() => {
    painted.current = 0;
    const context = learning?.context;
    if (!context) return;
    if (blocked || state?.fullscreen) {
      void call("video_learning_skip", { revision: context.revision }).catch(
        () => {},
      );
      return;
    }
    card.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    let second = 0;
    const first = requestAnimationFrame(() => {
      second = requestAnimationFrame(() => {
        painted.current = context.revision;
        void refresh();
      });
    });
    return () => {
      cancelAnimationFrame(first);
      cancelAnimationFrame(second);
    };
  }, [learning?.context?.revision, blocked, state?.fullscreen]);

  useEffect(() => {
    const interrupt = (event: Event) => {
      const target = event.target as HTMLElement;
      if (
        (event.type === "focusin" && target.matches("select,input,textarea")) ||
        (event.type === "toggle" &&
          target instanceof HTMLDetailsElement &&
          target.open)
      ) {
        const context = latest.current.learning?.context;
        if (context)
          void call("video_learning_skip", {
            revision: context.revision,
          }).catch(() => {});
      }
    };
    document.addEventListener("focusin", interrupt, true);
    document.addEventListener("toggle", interrupt, true);
    return () => {
      document.removeEventListener("focusin", interrupt, true);
      document.removeEventListener("toggle", interrupt, true);
    };
  }, []);

  async function skip() {
    const context = latest.current.learning?.context;
    if (!context) return;
    try {
      await call("video_learning_skip", { revision: context.revision });
      await refresh();
    } catch (e) {
      setError(message(e));
    }
  }
  return { learning, card, error, skip };
}
