import type { CollectionSeed } from "./types";
export interface CollectionQueue {
  current: CollectionSeed | null;
  pending: CollectionSeed[];
}
export type CollectionQueueAction =
  | { type: "receive"; seed: CollectionSeed }
  | { type: "open"; seed: CollectionSeed }
  | { type: "close" }
  | { type: "next" }
  | { type: "discard" };
export function collectionQueue(
  state: CollectionQueue,
  action: CollectionQueueAction,
): CollectionQueue {
  switch (action.type) {
    case "receive":
      return state.current || state.pending.length > 0
        ? { ...state, pending: [...state.pending, action.seed] }
        : { ...state, current: action.seed };
    case "open":
      return state.current
        ? { ...state, pending: [...state.pending, action.seed] }
        : { ...state, current: action.seed };
    case "close":
      return { ...state, current: null };
    case "next":
      return state.current || !state.pending.length
        ? state
        : { current: state.pending[0], pending: state.pending.slice(1) };
    case "discard":
      return { ...state, pending: state.pending.slice(1) };
  }
}
