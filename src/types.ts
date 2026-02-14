// Claw Desktop Types

export interface Config {
  configured: boolean;
  gateway: {
    mode: 'local' | 'remote';
    url: string;
    token: string;
  };
  models: {
    provider: string;
    apiKey: string;
    model: string;
  };
}

export interface Agent {
  id: string;
  name: string;
  model: string;
  status: 'online' | 'offline';
  lastActive?: Date;
}

export interface Channel {
  id: string;
  type: 'telegram' | 'discord' | 'whatsapp' | 'signal' | 'webchat';
  name: string;
  status: 'connected' | 'disconnected' | 'pending';
  config?: Record<string, unknown>;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;
}

export interface CronJob {
  id: string;
  name: string;
  schedule: string;
  enabled: boolean;
  lastRun?: Date;
  nextRun?: Date;
}

export interface GatewayStatus {
  running: boolean;
  version?: string;
  uptime?: number;
  agents?: number;
  channels?: number;
}
