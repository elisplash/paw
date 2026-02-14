// Claw Desktop - Gateway API Client

import type { GatewayStatus, Agent, Channel, CronJob } from './types';

let gatewayUrl = 'http://localhost:5757';
let gatewayToken = '';

export function setGatewayConfig(url: string, token: string) {
  gatewayUrl = url;
  gatewayToken = token;
}

async function apiCall<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string> || {}),
  };

  if (gatewayToken) {
    headers['Authorization'] = `Bearer ${gatewayToken}`;
  }

  const response = await fetch(`${gatewayUrl}${endpoint}`, {
    ...options,
    headers,
    signal: AbortSignal.timeout(10000),
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({}));
    throw new Error(error.message || `API error: ${response.status}`);
  }

  return response.json();
}

// Gateway Status
export async function getGatewayStatus(): Promise<GatewayStatus> {
  try {
    const response = await fetch(`${gatewayUrl}/health`, {
      signal: AbortSignal.timeout(2000),
    });
    
    if (response.ok) {
      return { running: true };
    }
    return { running: false };
  } catch {
    return { running: false };
  }
}

// Agents
export async function getAgents(): Promise<Agent[]> {
  try {
    const data = await apiCall<{ agents: Agent[] }>('/api/agents');
    return data.agents || [];
  } catch {
    return [];
  }
}

export async function createAgent(name: string, config: Record<string, unknown>): Promise<Agent> {
  return apiCall('/api/agents', {
    method: 'POST',
    body: JSON.stringify({ name, ...config }),
  });
}

// Channels
export async function getChannels(): Promise<Channel[]> {
  try {
    const data = await apiCall<{ channels: Channel[] }>('/api/channels');
    return data.channels || [];
  } catch {
    return [];
  }
}

// Chat
export async function sendChatMessage(message: string, sessionKey?: string): Promise<string> {
  const data = await apiCall<{ response: string }>('/api/chat', {
    method: 'POST',
    body: JSON.stringify({ message, sessionKey }),
  });
  return data.response;
}

// Cron Jobs
export async function getCronJobs(): Promise<CronJob[]> {
  try {
    const data = await apiCall<{ jobs: CronJob[] }>('/api/cron');
    return data.jobs || [];
  } catch {
    return [];
  }
}

// Config
export async function getConfig(): Promise<Record<string, unknown>> {
  return apiCall('/api/config');
}

export async function updateConfig(config: Record<string, unknown>): Promise<void> {
  await apiCall('/api/config', {
    method: 'PATCH',
    body: JSON.stringify(config),
  });
}

// Sessions
export async function getSessions(): Promise<unknown[]> {
  try {
    const data = await apiCall<{ sessions: unknown[] }>('/api/sessions');
    return data.sessions || [];
  } catch {
    return [];
  }
}
