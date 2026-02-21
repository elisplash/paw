// Settings: Browser & Sandbox — Pure types (no DOM, no IPC)

export interface BrowserProfile {
  id: string;
  name: string;
  size_bytes: number;
  created_at: string;
}

export interface BrowserConfig {
  profiles: BrowserProfile[];
  default_profile: string;
  headless: boolean;
  auto_close_tabs: boolean;
  idle_timeout_secs: number;
}

export interface Screenshot {
  filename: string;
  size_bytes: number;
  created_at: string;
}

export interface WorkspaceInfo {
  agent_id: string;
  total_files: number;
  total_size_bytes: number;
}

export interface WorkspaceFile {
  name: string;
  is_dir: boolean;
  size_bytes: number;
  modified_at?: string;
}

export interface NetworkPolicy {
  enabled: boolean;
  allowed_domains: string[];
  blocked_domains: string[];
  log_requests: boolean;
  recent_requests: unknown[];
}
