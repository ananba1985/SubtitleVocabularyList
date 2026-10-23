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
// replace is scoped to the current draft identity, so an old OCR result cannot replace a newer draft.
export type MutableCollectionQueueAction =
  | CollectionQueueAction
  | { type: "replace"; seed: CollectionSeed }
  | { type: "restore"; state: CollectionQueue };
export function collectionQueue(
  state: CollectionQueue,
  action: MutableCollectionQueueAction,
): CollectionQueue {
  switch (action.type) {
    case "restore":
      return action.state;
    case "replace":
      return state.current?.draftId === action.seed.draftId
        ? { ...state, current: action.seed }
        : state;
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
