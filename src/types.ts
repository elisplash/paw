// Claw Desktop Types

export interface Config {
  configured: boolean;
  gateway: {
    url: string;
    token: string;
  };
}

export interface GatewayConfig {
  url: string;
  token: string;
}

export interface GatewayStatus {
  connected: boolean;
  running: boolean;
  version?: string;
  uptime?: number;
  agents?: number;
  channels?: number;
}

export interface Agent {
  id: string;
  name: string;
  model?: string;
  status: 'online' | 'offline';
  lastActive?: Date;
}

export interface Channel {
  id: string;
  type: 'telegram' | 'discord' | 'whatsapp' | 'signal' | 'webchat';
  name: string;
  status: 'connected' | 'disconnected' | 'pending' | 'qr_required';
  linked?: boolean;
  config?: Record<string, unknown>;
}

export interface Message {
  id?: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;
}

export interface Session {
  key: string;
  kind: string;
  model?: string;
  lastMessage?: string;
  lastActive?: string;
}

export interface CronJob {
  id: string;
  name?: string;
  schedule: string | unknown;
  payload?: unknown;
  enabled: boolean;
  lastRun?: Date | string;
  nextRun?: Date | string;
}

export interface Skill {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
  installed: boolean;
}

export interface Node {
  id: string;
  name: string;
  connected: boolean;
  paired: boolean;
  caps?: string[];
}

export interface InstallProgress {
  stage: string;
  percent: number;
  message: string;
}
