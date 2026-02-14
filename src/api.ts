// Paw — HTTP helpers (used only for pre-connection health probes)
// All runtime communication goes through the WebSocket gateway in gateway.ts

let gatewayUrl = '';
let gatewayToken = '';

export function setGatewayConfig(url: string, token: string) {
  gatewayUrl = url;
  gatewayToken = token;
}

export function getGatewayUrl(): string {
  return gatewayUrl;
}

export function getGatewayToken(): string {
  return gatewayToken;
}

/**
 * Quick HTTP health probe — works before the WebSocket is up.
 * Returns true if the gateway HTTP endpoint responds at all.
 */
export async function probeHealth(): Promise<boolean> {
  if (!gatewayUrl) return false;
  try {
    const response = await fetch(`${gatewayUrl}/health`, {
      headers: gatewayToken ? { Authorization: `Bearer ${gatewayToken}` } : {},
      signal: AbortSignal.timeout(3000),
    });
    return response.ok;
  } catch {
    return false;
  }
}
