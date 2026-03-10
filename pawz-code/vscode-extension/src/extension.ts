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

import * as vscode from 'vscode';
import { PawzCodeClient } from './pawz-client';
import { ToolRenderer } from './tool-renderer';
import { ConnectionStateManager } from './connection-state';

const PARTICIPANT_ID = 'pawzcode';

// Session management - map conversation to pawz-code session ID
// We use WeakMap keyed by the history array to track conversations
const conversationSessions = new WeakMap<readonly (vscode.ChatRequestTurn | vscode.ChatResponseTurn)[], string>();

class MemoryContentProvider implements vscode.TextDocumentContentProvider {
  private store = new Map<string, string>();
  setContent(uri: vscode.Uri, content: string): void {
    this.store.set(uri.toString(), content);
  }
  provideTextDocumentContent(uri: vscode.Uri): string {
    return this.store.get(uri.toString()) ?? '';
  }
}

let connectionManager: ConnectionStateManager | undefined;

export function activate(context: vscode.ExtensionContext): void {
  // Connection health monitor + status bar
  connectionManager = new ConnectionStateManager(context);
  context.subscriptions.push(connectionManager);
  connectionManager.startHeartbeat();

  const participant = vscode.chat.createChatParticipant(PARTICIPANT_ID, handleChatRequest);
  participant.iconPath = vscode.Uri.joinPath(context.extensionUri, 'images', 'icon.png');
  context.subscriptions.push(participant);

  const diffProvider = new MemoryContentProvider();
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider('pawz-code-diff', diffProvider),
  );

  // Show diff command
  context.subscriptions.push(
    vscode.commands.registerCommand(
      'pawz-code.showDiff',
      async (filePath: string, oldContent: string, newContent: string) => {
        const label = filePath.split(/[\\/]/).pop() ?? filePath;
        const beforeUri = vscode.Uri.parse(
          `pawz-code-diff:before/${encodeURIComponent(filePath)}`,
        );
        const afterUri = vscode.Uri.parse(
          `pawz-code-diff:after/${encodeURIComponent(filePath)}`,
        );
        diffProvider.setContent(beforeUri, oldContent);
        diffProvider.setContent(afterUri, newContent);
        await vscode.commands.executeCommand(
          'vscode.diff',
          beforeUri,
          afterUri,
          `Pawz CODE → ${label}`,
          { preview: true } as vscode.TextDocumentShowOptions,
        );
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('pawz-code.openSettings', () => {
      void vscode.commands.executeCommand('workbench.action.openSettings', 'pawzCode');
    }),
  );

  // Show status command (triggered by status bar click)
  context.subscriptions.push(
    vscode.commands.registerCommand('pawz-code.showStatus', async () => {
      const mgr = connectionManager;
      if (!mgr) return;

      const isConnected = mgr.getStatus() === 'connected';
      const info = mgr.getDaemonInfo();

      if (!isConnected) {
        const action = await vscode.window.showWarningMessage(
          'Pawz CODE is not connected to the daemon.',
          'Open Settings',
          'Retry',
        );
        if (action === 'Open Settings') {
          void vscode.commands.executeCommand('pawz-code.openSettings');
        } else if (action === 'Retry') {
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
    }),
  );

  // Reconnect command
  context.subscriptions.push(
    vscode.commands.registerCommand('pawz-code.reconnect', async () => {
      const connected = await connectionManager?.checkHealth();
      if (connected) {
        void vscode.window.showInformationMessage('Pawz CODE reconnected.');
      } else {
        void vscode.window.showWarningMessage(
          'Pawz CODE could not reconnect. Is the daemon running?',
        );
      }
    }),
  );

  // React to config changes — restart heartbeat
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e: vscode.ConfigurationChangeEvent) => {
      if (e.affectsConfiguration('pawzCode')) {
        connectionManager?.startHeartbeat();
      }
    }),
  );
}

async function handleChatRequest(
  request: vscode.ChatRequest,
  context: vscode.ChatContext,
  stream: vscode.ChatResponseStream,
  token: vscode.CancellationToken,
): Promise<void> {
  const cfg = vscode.workspace.getConfiguration('pawzCode');
  const serverUrl = cfg.get<string>('serverUrl') ?? 'http://127.0.0.1:3941';
  const authToken = cfg.get<string>('authToken') ?? '';

  // Session persistence: use the conversation history as a stable reference
  // The history array reference stays the same throughout a conversation thread
  let sessionId: string | undefined;

  if (context.history.length > 0) {
    // Try to get existing session from this conversation
    sessionId = conversationSessions.get(context.history);

    if (!sessionId) {
      // This is a continuation of a conversation, but we lost the session
      // (e.g., extension reloaded). Create new session and store it.
      sessionId = `vscode-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
      conversationSessions.set(context.history, sessionId);
    }
  } else {
    // New conversation - generate session ID
    sessionId = `vscode-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
    // We can't store it in WeakMap yet since history is empty, but it will be
    // stored on the next turn when history has content
  }

  if (!authToken) {
    stream.markdown(
      '**Pawz CODE is not connected.**\n\n' +
        '1. Run `pawz-code` binary (from `pawz-code/server/`)\n' +
        '2. Copy the auth token printed on first run (or from `~/.pawz-code/config.toml`)\n' +
        '3. Set it in VS Code settings: `pawzCode.authToken`\n\n' +
        'Then try `@code hello` again.',
    );
    stream.button({ command: 'pawz-code.openSettings', title: '$(gear) Open Settings' });
    return;
  }

  // Check connection state before trying — show helpful message if offline
  if (connectionManager?.getStatus() === 'disconnected') {
    stream.markdown(
      `**Pawz CODE daemon is not reachable** at \`${serverUrl}\`.\n\n` +
        'Make sure the `pawz-code` server is running:\n' +
        '```bash\ncd pawz-code/server && cargo run\n```\n',
    );
    stream.button({ command: 'pawz-code.reconnect', title: '$(sync) Retry Connection' });
    stream.button({ command: 'pawz-code.openSettings', title: '$(gear) Settings' });
    return;
  }

  const client = new PawzCodeClient(serverUrl, authToken);
  const renderer = new ToolRenderer(stream);
  const workspaceContext = buildWorkspaceContext();

  const abortController = new AbortController();

  // Track the run_id so we can cancel the server-side agent loop on Stop.
  let activeRunId: string | null = null;
  let cancelPending = false;

  const cancelRun = (runId: string): void => {
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
    } else {
      // run_id hasn't arrived yet — mark as pending so we cancel when it does
      cancelPending = true;
    }
  });

  try {
    await client.streamChat(
      { message: request.prompt, context: workspaceContext, user_id: 'vscode', session_id: sessionId },
      (event) => renderer.handleEvent(event),
      abortController.signal,
      (runId) => {
        activeRunId = runId;
        // If cancellation was requested before the run_id arrived, cancel now
        if (cancelPending) {
          cancelRun(runId);
        }
      },
    );

    // After successful response, ensure connection state is updated
    if (connectionManager?.getStatus() !== 'connected') {
      void connectionManager?.checkHealth();
    }
  } catch (err: unknown) {
    if ((err as { name?: string }).name === 'AbortError') return;
    const msg = err instanceof Error ? err.message : String(err);
    if (msg.includes('ECONNREFUSED') || msg.includes('fetch failed') || msg.includes('dropped before run completed')) {
      // Update connection state
      void connectionManager?.checkHealth();

      stream.markdown(
        `**Could not reach pawz-code** at \`${serverUrl}\`.\n\n` +
          'Make sure the `pawz-code` server is running.\n' +
          'Check `pawzCode.serverUrl` matches the configured port.',
      );
      stream.button({ command: 'pawz-code.reconnect', title: '$(sync) Retry Connection' });
      stream.button({ command: 'pawz-code.openSettings', title: '$(gear) Check Settings' });
    } else {
      stream.markdown(`**Pawz CODE error:** ${msg}`);
    }
  }
}

function buildWorkspaceContext(): string {
  const parts: string[] = [];

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
      parts.push(
        `Selected code (lines ${range}):\n\`\`\`${doc.languageId}\n${text}\n\`\`\``,
      );
    }
  }

  // Inject active VS Code diagnostics (errors + warnings, up to 20)
  const allDiagnostics = vscode.languages
    .getDiagnostics()
    .flatMap(([uri, diags]) =>
      diags
        .filter(
          (d) =>
            d.severity === vscode.DiagnosticSeverity.Error ||
            d.severity === vscode.DiagnosticSeverity.Warning,
        )
        .map((d) => ({ uri, d })),
    )
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

  parts.push(
    'You have full access to the workspace via read_file, write_file, exec, ' +
      'list_directory, grep, and fetch tools. ' +
      'You also have remember and recall tools for persistent memory. ' +
      'Use absolute paths or resolve relative paths against the workspace root above.',
  );

  return parts.join('\n\n');
}

export function deactivate(): void {
  connectionManager?.dispose();
}
