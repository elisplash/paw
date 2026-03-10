"use strict";
/**
 * connection-state.ts — Connection health tracking and status bar indicator.
 *
 * Monitors the pawz-code daemon health endpoint, shows connection status
 * in the VS Code status bar, and provides reconnect logic.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.ConnectionStateManager = void 0;
const vscode = require("vscode");
class ConnectionStateManager {
    constructor(context) {
        this.context = context;
        this.status = 'unknown';
        this.daemonInfo = null;
        this.heartbeatTimer = null;
        this.consecutiveFailures = 0;
        this.BASE_INTERVAL_MS = 15000;
        this.MAX_INTERVAL_MS = 120000;
        this.statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
        this.statusBarItem.command = 'pawz-code.showStatus';
        this.statusBarItem.show();
        context.subscriptions.push(this.statusBarItem);
        this.setStatus('unknown');
    }
    /** Start polling the health endpoint with exponential backoff on failures. */
    startHeartbeat() {
        this.stopHeartbeat();
        this.consecutiveFailures = 0;
        void this.runHeartbeatCycle();
    }
    scheduleNext() {
        this.stopHeartbeat();
        const interval = this.consecutiveFailures === 0
            ? this.BASE_INTERVAL_MS
            : Math.min(this.BASE_INTERVAL_MS * Math.pow(2, this.consecutiveFailures - 1), this.MAX_INTERVAL_MS);
        this.heartbeatTimer = setTimeout(() => void this.runHeartbeatCycle(), interval);
    }
    async runHeartbeatCycle() {
        const ok = await this.checkHealth();
        if (ok) {
            this.consecutiveFailures = 0;
        }
        else {
            this.consecutiveFailures++;
        }
        this.scheduleNext();
    }
    stopHeartbeat() {
        if (this.heartbeatTimer !== null) {
            clearTimeout(this.heartbeatTimer);
            this.heartbeatTimer = null;
        }
    }
    dispose() {
        this.stopHeartbeat();
        this.statusBarItem.dispose();
    }
    getStatus() {
        return this.status;
    }
    getDaemonInfo() {
        return this.daemonInfo;
    }
    getConfig() {
        const cfg = vscode.workspace.getConfiguration('pawzCode');
        return {
            serverUrl: cfg.get('serverUrl') ?? 'http://127.0.0.1:3941',
            authToken: cfg.get('authToken') ?? '',
        };
    }
    async checkHealth() {
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
                const data = (await resp.json());
                this.daemonInfo = data;
                this.setStatus('connected');
                return true;
            }
            else {
                this.daemonInfo = null;
                this.setStatus('disconnected');
                return false;
            }
        }
        catch {
            this.daemonInfo = null;
            this.setStatus('disconnected');
            return false;
        }
    }
    setStatus(status) {
        this.status = status;
        this.updateStatusBar();
    }
    updateStatusBar() {
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
                this.statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
                break;
            default:
                this.statusBarItem.text = `$(circle-outline) Pawz CODE`;
                this.statusBarItem.tooltip = 'Pawz CODE — status unknown';
                this.statusBarItem.backgroundColor = undefined;
                break;
        }
    }
}
exports.ConnectionStateManager = ConnectionStateManager;
//# sourceMappingURL=connection-state.js.map
