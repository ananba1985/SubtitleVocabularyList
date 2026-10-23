import { useEffect, useRef, useState } from "react";
import type { Dispatch } from "react";
import { call, message } from "../api";
import type { CollectionSeed } from "../types";
import type {
  CollectionQueue,
  MutableCollectionQueueAction,
} from "../collectionQueue";
import { listen } from "@tauri-apps/api/event";

const ids = (queue: CollectionQueue) =>
  new Set(
    [queue.current, ...queue.pending].flatMap((s) =>
      s?.inputId ? [s.inputId] : [],
    ),
  );

export function useMobileInputs(
  queue: CollectionQueue,
  dispatch: Dispatch<MutableCollectionQueueAction>,
  ready: boolean,
) {
  const [loaded, setLoaded] = useState(false),
    [error, setError] = useState("");
  const seen = useRef(new Set<string>()),
    handled = useRef(new Set<string>()),
    previous = useRef(new Set<string>());
  const writes = useRef(Promise.resolve());
  useEffect(() => {
    if (!ready) return;
    let disposed = false;
    call("mobile_drafts_get")
      .then((raw) => {
        if (disposed) return;
        if (raw) {
          const saved = JSON.parse(raw);
          if (
            saved.schemaVersion !== 1 ||
            !saved.queue ||
            !Array.isArray(saved.queue.pending)
          )
            throw new Error("手机草稿格式暂不支持，已保存词库不受影响。");
          dispatch({ type: "restore", state: saved.queue });
          previous.current = ids(saved.queue);
          seen.current = new Set(previous.current);
          handled.current = new Set(
            (saved.handledInputs ?? []).filter(
              (id: unknown) => typeof id === "string",
            ),
          );
        }
        setLoaded(true);
      })
      .catch((e) => setError(message(e)));
    return () => {
      disposed = true;
    };
  }, [ready, dispatch]);
  useEffect(() => {
    if (!loaded) return;
    const current = ids(queue);
    const closed = [...previous.current].filter((id) => !current.has(id));
    previous.current = current;
    closed.forEach((id) => handled.current.add(id));
    const json = JSON.stringify({
      schemaVersion: 1,
      queue,
      handledInputs: [...handled.current],
    });
    writes.current = writes.current
      .catch(() => {})
      .then(async () => {
        await call("mobile_drafts_save", { json });
        for (const id of closed) {
          await call("mobile_incoming_ack", { id });
          handled.current.delete(id);
        }
      })
      .catch((e) => setError(message(e)));
  }, [queue, loaded]);
  useEffect(() => {
    if (!loaded || !ready) return;
    let disposed = false,
      reading = false;
    async function receive() {
      if (reading) return;
      reading = true;
      try {
        const inputs = await call("mobile_incoming_get");
        for (const input of inputs) {
          if (disposed) return;
          if (handled.current.has(input.id)) {
            await call("mobile_incoming_ack", { id: input.id });
            handled.current.delete(input.id);
            continue;
          }
          if (seen.current.has(input.id)) continue;
          seen.current.add(input.id);
          let seed: CollectionSeed = {
            draftId: input.id,
            inputId: input.id,
            text: "",
            kind: "word",
            context: "",
            sourceTitle: input.title,
          };
          if (input.kind === "image") {
            try {
              seed.image = await call("mobile_image_prepare", {
                id: input.id,
                uri: input.uri,
              });
            } catch (e) {
              seed.imageSource = { id: input.id, uri: input.uri };
              setError(message(e));
            }
          } else if (input.kind === "video") seed.videoUri = input.uri;
          else {
            try {
              const draft = await call("mobile_text_draft", {
                text: input.text,
                title: input.title,
              });
              seed = { ...seed, ...draft, example: draft.example ?? undefined };
            } catch (e) {
              seed = {
                ...seed,
                text: input.text,
                context: input.text,
                kind: "sentence",
              };
              setError(message(e));
            }
          }
          if (!disposed) dispatch({ type: "receive", seed });
        }
      } catch (e) {
        if (!disposed) setError(message(e));
      } finally {
        reading = false;
      }
    }
    void receive();
    const timer = window.setInterval(receive, 1200);
    const resumed = listen("mobile_resumed", () => {
      void receive();
    });
    return () => {
      disposed = true;
      window.clearInterval(timer);
      void resumed.then((stop) => stop());
    };
  }, [ready, loaded, dispatch]);
  return { loaded, error };
}
