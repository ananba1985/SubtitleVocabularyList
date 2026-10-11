export interface QueryOrigin {
  kind: string;
  sourceId?: string | null;
  sourceTitle: string;
  locationKey: string;
}
export interface QueryInput {
  target: string;
  context: string;
  mode: "explain" | "translate";
  sourceLanguage: string;
  targetLanguage: string;
  origin: QueryOrigin;
}
export interface QueryValue {
  meaning: string;
  translation: string;
  notes: string;
  sourceUrl: string;
}
export interface ProviderResult {
  providerId: string;
  name: string;
  state: "queued" | "running" | "ready" | "failed" | "cancelled";
  cacheSource: string;
  value: QueryValue | null;
  error: string | null;
}
export interface QuerySession {
  sessionId: string;
  inputRevision: number;
  queryId: string | null;
  input: QueryInput;
  automatic: boolean;
  results: ProviderResult[];
}
export interface ClipboardStatus {
  enabled: boolean;
  debounceMs: number;
  persistResults: boolean;
  error: string | null;
}
export interface PluginField {
  id: string;
  label: string;
  secret: boolean;
  required: boolean;
}
export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  protocolVersion: number;
  minHostVersion: string;
  license: string;
  capabilities: string[];
  allowedHosts: string[];
  endpoint: string;
  method: string;
  fields: PluginField[];
}
export interface Provider {
  id: string;
  name: string;
  kind: string;
  enabled: boolean;
  autoQuery: boolean;
  sendContext: boolean;
  order: number;
  maxConcurrency: number;
  networkScope: string;
  values: Record<string, string>;
  credentialFields: string[];
  manifest: PluginManifest | null;
}
