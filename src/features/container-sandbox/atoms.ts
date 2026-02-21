// ─── Container Sandbox · Atoms ─────────────────────────────────────────
// Pure types and functions for container sandbox configuration.
// No side effects — no Tauri IPC, no DOM, no localStorage.

// ── Types ──────────────────────────────────────────────────────────────

export interface SandboxConfig {
  /** Whether sandboxing is enabled (default: false) */
  enabled: boolean;
  /** Docker image to use */
  image: string;
  /** Timeout in seconds */
  timeoutSecs: number;
  /** Memory limit in bytes */
  memoryLimit: number;
  /** CPU shares (relative weight) */
  cpuShares: number;
  /** Whether network is allowed inside container */
  networkEnabled: boolean;
  /** Working directory inside container */
  workdir: string;
  /** Bind mounts (host:container:mode) */
  bindMounts: string[];
}

export interface SandboxStatus {
  dockerAvailable: boolean;
  lastChecked: number;
  containerCount: number;
}

export interface SandboxResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut: boolean;
  containerId: string;
}

// ── Constants ──────────────────────────────────────────────────────────

export const DEFAULT_SANDBOX_CONFIG: SandboxConfig = {
  enabled: false,
  image: 'alpine:latest',
  timeoutSecs: 30,
  memoryLimit: 256 * 1024 * 1024, // 256 MB
  cpuShares: 512,
  networkEnabled: false,
  workdir: '/workspace',
  bindMounts: [],
};

/** Pre-built sandbox profiles for common use cases */
export const SANDBOX_PRESETS = {
  minimal: {
    ...DEFAULT_SANDBOX_CONFIG,
    enabled: true,
    image: 'alpine:latest',
    memoryLimit: 128 * 1024 * 1024,
    timeoutSecs: 15,
    networkEnabled: false,
  } as SandboxConfig,

  development: {
    ...DEFAULT_SANDBOX_CONFIG,
    enabled: true,
    image: 'node:20-alpine',
    memoryLimit: 512 * 1024 * 1024,
    timeoutSecs: 60,
    networkEnabled: true,
  } as SandboxConfig,

  python: {
    ...DEFAULT_SANDBOX_CONFIG,
    enabled: true,
    image: 'python:3.12-alpine',
    memoryLimit: 512 * 1024 * 1024,
    timeoutSecs: 60,
    networkEnabled: true,
  } as SandboxConfig,

  restricted: {
    ...DEFAULT_SANDBOX_CONFIG,
    enabled: true,
    image: 'alpine:latest',
    memoryLimit: 64 * 1024 * 1024,
    timeoutSecs: 10,
    networkEnabled: false,
    cpuShares: 256,
  } as SandboxConfig,
} as const;

// ── Validation ─────────────────────────────────────────────────────────

export interface SandboxValidation {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

/** Validate a sandbox config for correctness */
export function validateSandboxConfig(config: SandboxConfig): SandboxValidation {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!config.image || config.image.trim() === '') {
    errors.push('Docker image name is required');
  }

  if (config.timeoutSecs < 1) {
    errors.push('Timeout must be at least 1 second');
  }
  if (config.timeoutSecs > 3600) {
    warnings.push('Timeout exceeds 1 hour — consider a shorter limit');
  }

  if (config.memoryLimit < 16 * 1024 * 1024) {
    errors.push('Memory limit must be at least 16 MB');
  }
  if (config.memoryLimit > 4 * 1024 * 1024 * 1024) {
    warnings.push('Memory limit exceeds 4 GB — this is very high');
  }

  if (config.cpuShares < 1) {
    errors.push('CPU shares must be at least 1');
  }

  if (config.networkEnabled) {
    warnings.push('Network access is enabled — sandboxed commands can reach the internet');
  }

  // Validate bind mounts format
  for (const mount of config.bindMounts) {
    const parts = mount.split(':');
    if (parts.length < 2) {
      errors.push(`Invalid bind mount format: "${mount}" (expected host:container[:mode])`);
    }
    if (parts.length >= 3 && !['ro', 'rw'].includes(parts[2])) {
      errors.push(`Invalid mount mode in "${mount}" (must be "ro" or "rw")`);
    }
  }

  return {
    valid: errors.length === 0,
    errors,
    warnings,
  };
}

// ── Helpers ────────────────────────────────────────────────────────────

/** Format memory limit for display */
export function formatMemoryLimit(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

/** Describe sandbox config in human-readable form */
export function describeSandboxConfig(config: SandboxConfig): string {
  if (!config.enabled) return 'Sandbox: Disabled (exec runs on host)';

  const parts = [
    `Image: ${config.image}`,
    `Memory: ${formatMemoryLimit(config.memoryLimit)}`,
    `Timeout: ${config.timeoutSecs}s`,
    `Network: ${config.networkEnabled ? 'Yes' : 'No'}`,
  ];
  if (config.bindMounts.length > 0) {
    parts.push(`Mounts: ${config.bindMounts.length}`);
  }
  return `Sandbox: Enabled — ${parts.join(', ')}`;
}

/** Check if a command looks potentially dangerous (heuristic) */
export function assessCommandRisk(command: string): 'low' | 'medium' | 'high' | 'critical' {
  const lower = command.toLowerCase();

  // Critical: system-altering commands
  const criticalPatterns = [
    /\brm\s+-rf\s+\/(?!\w)/, // rm -rf / (root)
    /\bmkfs\b/, // format filesystem
    /\bdd\s+.*of=\/dev/, // write to device
    /\b:(){.*};:/, // fork bomb
  ];
  if (criticalPatterns.some((p) => p.test(lower))) return 'critical';

  // High: privilege escalation, sensitive paths
  const highPatterns = [
    /\bsudo\b/,
    /\bsu\s/,
    /\bchmod\s.*777/,
    /\/etc\/passwd/,
    /\/etc\/shadow/,
    /\bcurl\b.*\|\s*sh/,
    /\bwget\b.*\|\s*sh/, // pipe to shell
    /\beval\b/,
    /\bexec\b/,
  ];
  if (highPatterns.some((p) => p.test(lower))) return 'high';

  // Medium: network, file changes
  const mediumPatterns = [
    /\bcurl\b/,
    /\bwget\b/,
    /\bnc\b/,
    /\bnetcat\b/,
    /\brm\b/,
    /\bmv\b.*\//,
    /\bchmod\b/,
    /\bchown\b/,
    /\bpip\s+install\b/,
    /\bnpm\s+install\b/,
    /\bapt\b/,
  ];
  if (mediumPatterns.some((p) => p.test(lower))) return 'medium';

  return 'low';
}
