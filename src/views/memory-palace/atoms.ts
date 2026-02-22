// Memory Palace — Atoms (pure types, constants, validation)
// Zero DOM, zero IPC, zero canvas

// ── Types ──────────────────────────────────────────────────────────────────

/** Form data returned by readMemoryForm after validation */
export interface MemoryFormData {
  apiKey: string;
  baseUrl: string;
  modelName: string;
  apiVersion: string;
  provider: string;
}

/** Raw form input values before validation */
export interface MemoryFormInputs {
  apiKey: string;
  azureBaseUrl: string;
  openaiBaseUrl: string;
  modelName: string;
  apiVersion: string;
  provider: string;
}

/** Recall card data */
export interface RecallCardData {
  id?: string;
  text?: string;
  category?: string;
  importance?: number;
  score?: number;
  agent_id?: string;
}

// ── Constants ──────────────────────────────────────────────────────────────

export const CATEGORY_COLORS: Record<string, string> = {
  other: '#676879',
  preference: '#0073EA',
  fact: '#00CA72',
  decision: '#FDAB3D',
  procedure: '#E44258',
  concept: '#A25DDC',
  code: '#579BFC',
  person: '#FF642E',
  project: '#CAB641',
};

// ── Pure validation ────────────────────────────────────────────────────────

export type FormValidationError =
  | { kind: 'url_in_key'; swapUrl: string }
  | { kind: 'url_in_key_dup' }
  | { kind: 'azure_no_url' }
  | { kind: 'no_key' };

export type FormValidationResult =
  | { ok: true; data: MemoryFormData }
  | { ok: false; error: FormValidationError };

/**
 * Pure form validation. Takes raw input values, returns validated data or error.
 * Caller handles DOM feedback (border highlights, focus, etc.).
 */
export function validateMemoryForm(inputs: MemoryFormInputs): FormValidationResult {
  const apiKey = inputs.apiKey;
  const provider = inputs.provider;
  const baseUrl = provider === 'azure' ? inputs.azureBaseUrl : inputs.openaiBaseUrl;
  const modelName = inputs.modelName;
  const apiVersion = inputs.apiVersion;

  // Detect URL pasted into API key field
  if (apiKey.startsWith('http://') || apiKey.startsWith('https://')) {
    if (!baseUrl) {
      return { ok: false, error: { kind: 'url_in_key', swapUrl: apiKey } };
    }
    return { ok: false, error: { kind: 'url_in_key_dup' } };
  }

  if (provider === 'azure' && !baseUrl) {
    return { ok: false, error: { kind: 'azure_no_url' } };
  }

  if (!apiKey) {
    return { ok: false, error: { kind: 'no_key' } };
  }

  return { ok: true, data: { apiKey, baseUrl, modelName, apiVersion, provider } };
}

// ── Agent label ────────────────────────────────────────────────────────────

/** Format an agent_id for display — 'system' for empty/undefined, otherwise the agent name. */
export function agentLabel(agentId?: string): string {
  if (!agentId || agentId === '') return 'system';
  return agentId;
}

/** Build agent filter options HTML from a list of known agent ids. */
export function buildAgentFilterOptions(agentIds: string[]): string {
  const unique = [...new Set(agentIds.filter((id) => id && id.length > 0))];
  return unique.map((id) => `<option value="${id}">${id}</option>`).join('');
}
