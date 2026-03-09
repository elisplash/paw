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

const PARTICIPANT_ID = 'pawz-code';

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

      const lines = [
        `**Pawz CODE** — Connected`,
        ``,
        `- Model: \`${info?.model ?? 'unknown'}\``,
        `- Provider: \`${info?.provider ?? 'unknown'}\``,
        `- Version: \`${info?.version ?? 'unknown'}\``,
        `- Active runs: ${info?.active_runs ?? 0}`,
        `- Memory entries: ${info?.memory_entries ?? 0}`,
        `- Engram entries: ${info?.engram_entries ?? 0}`,
        `- Protocols: ${(info?.protocols ?? []).join(', ') || 'none'}`,
      ];

      const doc = await vscode.workspace.openTextDocument({
        content: lines.join('\n'),
        language: 'markdown',
      });
      await vscode.window.showTextDocument(doc, { preview: true });
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
  _context: vscode.ChatContext,
  stream: vscode.ChatResponseStream,
  token: vscode.CancellationToken,
): Promise<void> {
  const cfg = vscode.workspace.getConfiguration('pawzCode');
  const serverUrl = cfg.get<string>('serverUrl') ?? 'http://127.0.0.1:3941';
  const authToken = cfg.get<string>('authToken') ?? '';

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
  const context = buildWorkspaceContext();

  const abortController = new AbortController();
  token.onCancellationRequested(() => abortController.abort());

  try {
    await client.streamChat(
      { message: request.prompt, context, user_id: 'vscode' },
      (event) => renderer.handleEvent(event),
      abortController.signal,
    );

    // After successful response, ensure connection state is updated
    if (connectionManager?.getStatus() !== 'connected') {
      void connectionManager?.checkHealth();
    }
  } catch (err: unknown) {
    if ((err as { name?: string }).name === 'AbortError') return;
    const msg = err instanceof Error ? err.message : String(err);
    if (msg.includes('ECONNREFUSED') || msg.includes('fetch failed')) {
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

  parts.push(
    'You have full access to the workspace via read_file, write_file, exec, ' +
      'list_directory, grep, fetch, remember, recall, workspace_map, file_summary, ' +
      'search_symbols, git_status, git_diff, apply_patch, engram_store, and engram_recall tools. ' +
      'Use absolute paths or resolve relative paths against the workspace root above.',
  );

  return parts.join('\n\n');
}

export function deactivate(): void {
  connectionManager?.dispose();
}
