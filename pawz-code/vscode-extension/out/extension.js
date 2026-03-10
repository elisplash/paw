"use strict";
/**
 * extension.ts — Pawz CODE VS Code Extension.
 *
 * Registers the @code chat participant. Points at the standalone pawz-code
 * server (default port 3941) — completely separate from Pawz Desktop.
 *
 * Usage: @code <your message>
 *
 * Configure via VS Code settings:
 *   pawzCode.serverUrl  — default http://127.0.0.1:3941
 *   pawzCode.authToken  — from ~/.pawz-code/config.toml
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = require("vscode");
const pawz_client_1 = require("./pawz-client");
const tool_renderer_1 = require("./tool-renderer");
const connection_state_1 = require("./connection-state");
const PARTICIPANT_ID = 'pawzcode';
// Session management - map VS Code history ID to pawz-code session ID
const sessionMap = new Map();
class MemoryContentProvider {
    constructor() {
        this.store = new Map();
    }
    setContent(uri, content) {
        this.store.set(uri.toString(), content);
    }
    provideTextDocumentContent(uri) {
        return this.store.get(uri.toString()) ?? '';
    }
}
let connectionManager;
function activate(context) {
    // Connection health monitor + status bar
    connectionManager = new connection_state_1.ConnectionStateManager(context);
    context.subscriptions.push(connectionManager);
    connectionManager.startHeartbeat();
    const participant = vscode.chat.createChatParticipant(PARTICIPANT_ID, handleChatRequest);
    participant.iconPath = vscode.Uri.joinPath(context.extensionUri, 'images', 'icon.png');
    context.subscriptions.push(participant);
    const diffProvider = new MemoryContentProvider();
    context.subscriptions.push(vscode.workspace.registerTextDocumentContentProvider('pawz-code-diff', diffProvider));
    // Show diff command
    context.subscriptions.push(vscode.commands.registerCommand('pawz-code.showDiff', async (filePath, oldContent, newContent) => {
        const label = filePath.split(/[\\/]/).pop() ?? filePath;
        const beforeUri = vscode.Uri.parse(`pawz-code-diff:before/${encodeURIComponent(filePath)}`);
        const afterUri = vscode.Uri.parse(`pawz-code-diff:after/${encodeURIComponent(filePath)}`);
        diffProvider.setContent(beforeUri, oldContent);
        diffProvider.setContent(afterUri, newContent);
        await vscode.commands.executeCommand('vscode.diff', beforeUri, afterUri, `Pawz CODE → ${label}`, { preview: true });
    }));
    context.subscriptions.push(vscode.commands.registerCommand('pawz-code.openSettings', () => {
        void vscode.commands.executeCommand('workbench.action.openSettings', 'pawzCode');
    }));
    // Show status command (triggered by status bar click)
    context.subscriptions.push(vscode.commands.registerCommand('pawz-code.showStatus', async () => {
        const mgr = connectionManager;
        if (!mgr)
            return;
        const isConnected = mgr.getStatus() === 'connected';
        const info = mgr.getDaemonInfo();
        if (!isConnected) {
            const action = await vscode.window.showWarningMessage('Pawz CODE is not connected to the daemon.', 'Open Settings', 'Retry');
            if (action === 'Open Settings') {
                void vscode.commands.executeCommand('pawz-code.openSettings');
            }
            else if (action === 'Retry') {
                await mgr.checkHealth();
            }
            return;
        }
        // Show status in a native QuickPick panel instead of opening a text document
        const qp = vscode.window.createQuickPick();
        qp.title = 'Pawz CODE — Connected';
        qp.placeholder = 'Status (read-only)';
        qp.canSelectMany = false;
        qp.items = [
            { label: '$(check) Status', description: 'Connected', alwaysShow: true },
            { label: '$(symbol-misc) Model', description: info?.model ?? 'unknown', alwaysShow: true },
            { label: '$(cloud) Provider', description: info?.provider ?? 'unknown', alwaysShow: true },
            { label: '$(tag) Version', description: info?.version ?? 'unknown', alwaysShow: true },
            { label: '$(sync) Active runs', description: String(info?.active_runs ?? 0), alwaysShow: true },
            { label: '$(database) Memory entries', description: String(info?.memory_entries ?? 0), alwaysShow: true },
            { label: '$(book) Engram entries', description: String(info?.engram_entries ?? 0), alwaysShow: true },
            {
                label: '$(list-unordered) Protocols',
                description: (info?.protocols ?? []).join(', ') || 'none',
                alwaysShow: true,
            },
        ];
        qp.onDidAccept(() => qp.dispose());
        qp.onDidHide(() => qp.dispose());
        qp.show();
    }));
    // Reconnect command
    context.subscriptions.push(vscode.commands.registerCommand('pawz-code.reconnect', async () => {
        const connected = await connectionManager?.checkHealth();
        if (connected) {
            void vscode.window.showInformationMessage('Pawz CODE reconnected.');
        }
        else {
            void vscode.window.showWarningMessage('Pawz CODE could not reconnect. Is the daemon running?');
        }
    }));
    // React to config changes — restart heartbeat
    context.subscriptions.push(vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration('pawzCode')) {
            connectionManager?.startHeartbeat();
        }
    }));
}
async function handleChatRequest(request, context, stream, token) {
    const cfg = vscode.workspace.getConfiguration('pawzCode');
    const serverUrl = cfg.get('serverUrl') ?? 'http://127.0.0.1:3941';
    const authToken = cfg.get('authToken') ?? '';
    // Session persistence: use conversation history length as stable session key
    // If history is empty, create new session; otherwise reuse existing
    const vsCodeSessionKey = context.history.length > 0
        ? `hist-${context.history.length}-${context.history[0].participant}`
        : `new-${Date.now()}`;
    let sessionId = sessionMap.get(vsCodeSessionKey);
    if (!sessionId && context.history.length > 0) {
        // Try to find any existing session for this conversation
        for (const [key, sid] of sessionMap.entries()) {
            if (key.startsWith('hist-') && key.endsWith(`-${context.history[0].participant}`)) {
                sessionId = sid;
                sessionMap.set(vsCodeSessionKey, sid);
                break;
            }
        }
    }
    if (!sessionId) {
        // Generate new session ID and store mapping
        sessionId = `vscode-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
        sessionMap.set(vsCodeSessionKey, sessionId);
    }
    if (!authToken) {
        stream.markdown('**Pawz CODE is not connected.**\n\n' +
            '1. Run `pawz-code` binary (from `pawz-code/server/`)\n' +
            '2. Copy the auth token printed on first run (or from `~/.pawz-code/config.toml`)\n' +
            '3. Set it in VS Code settings: `pawzCode.authToken`\n\n' +
            'Then try `@code hello` again.');
        stream.button({ command: 'pawz-code.openSettings', title: '$(gear) Open Settings' });
        return;
    }
    // Check connection state before trying — show helpful message if offline
    if (connectionManager?.getStatus() === 'disconnected') {
        stream.markdown(`**Pawz CODE daemon is not reachable** at \`${serverUrl}\`.\n\n` +
            'Make sure the `pawz-code` server is running:\n' +
            '```bash\ncd pawz-code/server && cargo run\n```\n');
        stream.button({ command: 'pawz-code.reconnect', title: '$(sync) Retry Connection' });
        stream.button({ command: 'pawz-code.openSettings', title: '$(gear) Settings' });
        return;
    }
    const client = new pawz_client_1.PawzCodeClient(serverUrl, authToken);
    const renderer = new tool_renderer_1.ToolRenderer(stream);
    const workspaceContext = buildWorkspaceContext();
    const abortController = new AbortController();
    // Track the run_id so we can cancel the server-side agent loop on Stop.
    let activeRunId = null;
    let cancelPending = false;
    const cancelRun = (runId) => {
        // Fire-and-forget — we don't want to block the cancellation path
        fetch(new URL('/runs/cancel', serverUrl).toString(), {
            method: 'POST',
            headers: {
                Authorization: `Bearer ${authToken}`,
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ run_id: runId }),
            signal: AbortSignal.timeout(3000),
        }).catch(() => {
            // Daemon may already be gone — ignore
        });
    };
    token.onCancellationRequested(() => {
        abortController.abort();
        if (activeRunId) {
            cancelRun(activeRunId);
        }
        else {
            // run_id hasn't arrived yet — mark as pending so we cancel when it does
            cancelPending = true;
        }
    });
    try {
        await client.streamChat({ message: request.prompt, context: workspaceContext, user_id: 'vscode', session_id: sessionId }, (event) => renderer.handleEvent(event), abortController.signal, (runId) => {
            activeRunId = runId;
            // If cancellation was requested before the run_id arrived, cancel now
            if (cancelPending) {
                cancelRun(runId);
            }
        });
        // After successful response, ensure connection state is updated
        if (connectionManager?.getStatus() !== 'connected') {
            void connectionManager?.checkHealth();
        }
    }
    catch (err) {
        if (err.name === 'AbortError')
            return;
        const msg = err instanceof Error ? err.message : String(err);
        if (msg.includes('ECONNREFUSED') || msg.includes('fetch failed') || msg.includes('dropped before run completed')) {
            // Update connection state
            void connectionManager?.checkHealth();
            stream.markdown(`**Could not reach pawz-code** at \`${serverUrl}\`.\n\n` +
                'Make sure the `pawz-code` server is running.\n' +
                'Check `pawzCode.serverUrl` matches the configured port.');
            stream.button({ command: 'pawz-code.reconnect', title: '$(sync) Retry Connection' });
            stream.button({ command: 'pawz-code.openSettings', title: '$(gear) Check Settings' });
        }
        else {
            stream.markdown(`**Pawz CODE error:** ${msg}`);
        }
    }
}
function buildWorkspaceContext() {
    const parts = [];
    const wsFolder = vscode.workspace.workspaceFolders?.[0];
    if (wsFolder) {
        parts.push(`Workspace root: ${wsFolder.uri.fsPath}`);
    }
    const editor = vscode.window.activeTextEditor;
    if (editor) {
        const doc = editor.document;
        const rel = vscode.workspace.asRelativePath(doc.uri);
        parts.push(`Active file: ${rel} (${doc.languageId})`);
        const sel = editor.selection;
        if (!sel.isEmpty) {
            const text = doc.getText(sel);
            const range = `${sel.start.line + 1}–${sel.end.line + 1}`;
            parts.push(`Selected code (lines ${range}):\n\`\`\`${doc.languageId}\n${text}\n\`\`\``);
        }
    }
    // Inject active VS Code diagnostics (errors + warnings, up to 20)
    const allDiagnostics = vscode.languages
        .getDiagnostics()
        .flatMap(([uri, diags]) => diags
        .filter((d) => d.severity === vscode.DiagnosticSeverity.Error ||
        d.severity === vscode.DiagnosticSeverity.Warning)
        .map((d) => ({ uri, d })))
        // Sort errors before warnings
        .sort((a, b) => a.d.severity - b.d.severity)
        .slice(0, 20);
    if (allDiagnostics.length > 0) {
        const diagLines = allDiagnostics.map(({ uri, d }) => {
            const rel = vscode.workspace.asRelativePath(uri);
            const line = d.range.start.line + 1;
            const sev = d.severity === vscode.DiagnosticSeverity.Error ? 'error' : 'warning';
            return `  ${sev} ${rel}:${line} — ${d.message}`;
        });
        parts.push(`Active diagnostics (${allDiagnostics.length}):\n${diagLines.join('\n')}`);
    }
    parts.push('You have full access to the workspace via read_file, write_file, exec, ' +
        'list_directory, grep, and fetch tools. ' +
        'You also have remember and recall tools for persistent memory. ' +
        'Use absolute paths or resolve relative paths against the workspace root above.');
    return parts.join('\n\n');
}
function deactivate() {
    connectionManager?.dispose();
}
//# sourceMappingURL=extension.js.map
