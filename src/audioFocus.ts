import { useEffect } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { call } from "./api";

/** One active audio source across learning views, query windows and native video. */
export function useAudioFocus() {
  useEffect(() => {
    const owner = `${getCurrentWindow().label}:${crypto.randomUUID()}`;
    const pauseOthers = (except?: HTMLMediaElement) => {
      document
        .querySelectorAll<HTMLMediaElement>("audio,video")
        .forEach((media) => {
          if (media !== except && !media.paused) media.pause();
        });
    };
    const stop = listen<{ owner: string }>("audio_focus", (event) => {
      if (event.payload.owner !== owner) pauseOthers();
    });
    const onPlay = (event: Event) => {
      if (!(event.target instanceof HTMLMediaElement)) return;
      pauseOthers(event.target);
      void emit("audio_focus", { owner });
      void call("watch_control", {
        control: {
          action: "audio_focus",
          value: null,
          text: null,
          expectedRevision: null,
        },
      }).catch((error) =>
        console.error("Unable to pause video for learning audio", error),
      );
    };
    document.addEventListener("play", onPlay, true);
    return () => {
      document.removeEventListener("play", onPlay, true);
      void stop.then((unlisten) => unlisten());
    };
  }, []);
}
