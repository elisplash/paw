/**
 * connection-state.ts — Connection health tracking and status bar indicator.
 *
 * Monitors the pawz-code daemon health endpoint, shows connection status
 * in the VS Code status bar, and provides reconnect logic.
 */

import * as vscode from 'vscode';

export type ConnectionStatus = 'connected' | 'disconnected' | 'connecting' | 'unknown';

export interface DaemonStatus {
  status: string;
  service: string;
  version?: string;
  model?: string;
  provider?: string;
  active_runs?: number;
  memory_entries?: number;
  engram_entries?: number;
  protocols?: string[];
}

export class ConnectionStateManager implements vscode.Disposable {
  private statusBarItem: vscode.StatusBarItem;
  private status: ConnectionStatus = 'unknown';
  private daemonInfo: DaemonStatus | null = null;
  private heartbeatTimer: ReturnType<typeof setTimeout> | null = null;
  private consecutiveFailures = 0;
  private readonly BASE_INTERVAL_MS = 15_000;
  private readonly MAX_INTERVAL_MS = 120_000;

  constructor(private readonly context: vscode.ExtensionContext) {
    this.statusBarItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      100,
    );
    this.statusBarItem.command = 'pawz-code.showStatus';
    this.statusBarItem.show();
    context.subscriptions.push(this.statusBarItem);

    this.setStatus('unknown');
  }

  /** Start polling the health endpoint with exponential backoff on failures. */
  startHeartbeat(): void {
    this.stopHeartbeat();
    this.consecutiveFailures = 0;
    void this.runHeartbeatCycle();
  }

  private scheduleNext(): void {
    this.stopHeartbeat();
    const interval =
      this.consecutiveFailures === 0
        ? this.BASE_INTERVAL_MS
        : Math.min(
            this.BASE_INTERVAL_MS * Math.pow(2, this.consecutiveFailures - 1),
            this.MAX_INTERVAL_MS,
          );
    this.heartbeatTimer = setTimeout(() => void this.runHeartbeatCycle(), interval);
  }

  private async runHeartbeatCycle(): Promise<void> {
    const ok = await this.checkHealth();
    if (ok) {
      this.consecutiveFailures = 0;
    } else {
      this.consecutiveFailures++;
    }
    this.scheduleNext();
  }

  stopHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      clearTimeout(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  dispose(): void {
    this.stopHeartbeat();
    this.statusBarItem.dispose();
  }

  getStatus(): ConnectionStatus {
    return this.status;
  }

  getDaemonInfo(): DaemonStatus | null {
    return this.daemonInfo;
  }

  private getConfig(): { serverUrl: string; authToken: string } {
    const cfg = vscode.workspace.getConfiguration('pawzCode');
    return {
      serverUrl: cfg.get<string>('serverUrl') ?? 'http://127.0.0.1:3941',
      authToken: cfg.get<string>('authToken') ?? '',
    };
  }

  async checkHealth(): Promise<boolean> {
    const { serverUrl, authToken } = this.getConfig();
    if (!authToken) {
      this.setStatus('disconnected');
      return false;
    }

    try {
      this.setStatus('connecting');
      const resp = await fetch(`${serverUrl}/status`, {
        headers: { Authorization: `Bearer ${authToken}` },
        signal: AbortSignal.timeout(5000),
      });

      if (resp.ok) {
        const data = (await resp.json()) as DaemonStatus;
        this.daemonInfo = data;
        this.setStatus('connected');
        return true;
      } else {
        this.daemonInfo = null;
        this.setStatus('disconnected');
        return false;
      }
    } catch {
      this.daemonInfo = null;
      this.setStatus('disconnected');
      return false;
    }
  }

  private setStatus(status: ConnectionStatus): void {
    this.status = status;
    this.updateStatusBar();
  }

  private updateStatusBar(): void {
    switch (this.status) {
      case 'connected': {
        const model = this.daemonInfo?.model ?? 'unknown';
        const activeRuns = this.daemonInfo?.active_runs ?? 0;
        const runsSuffix = activeRuns > 0 ? ` (${activeRuns} running)` : '';
        this.statusBarItem.text = `$(check) Pawz CODE${runsSuffix}`;
        this.statusBarItem.tooltip = `Connected • ${model}\nMemory: ${this.daemonInfo?.memory_entries ?? 0} entries • Engram: ${this.daemonInfo?.engram_entries ?? 0} entries\nClick to view status`;
        this.statusBarItem.backgroundColor = undefined;
        break;
      }
      case 'connecting':
        this.statusBarItem.text = `$(sync~spin) Pawz CODE`;
        this.statusBarItem.tooltip = 'Connecting to pawz-code daemon...';
        this.statusBarItem.backgroundColor = undefined;
        break;
      case 'disconnected':
        this.statusBarItem.text = `$(error) Pawz CODE`;
        this.statusBarItem.tooltip = 'Not connected. Click to check settings.';
        this.statusBarItem.backgroundColor = new vscode.ThemeColor(
          'statusBarItem.warningBackground',
        );
        break;
      default:
        this.statusBarItem.text = `$(circle-outline) Pawz CODE`;
        this.statusBarItem.tooltip = 'Pawz CODE — status unknown';
        this.statusBarItem.backgroundColor = undefined;
        break;
    }
  }
}
