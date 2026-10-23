import { createContext, useContext } from "react";

// Presentation options; the native capability snapshot is authoritative on mobile.
export const RuntimeFeatures = createContext({
  systemSpeech: true,
  knownTargetPreferences: true,
  mobile: false,
});
export const useRuntimeFeatures = () => useContext(RuntimeFeatures);
