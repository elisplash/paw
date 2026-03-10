/**
 * pawz-client.ts — SSE streaming client for the pawz-code /chat/stream endpoint.
 * Protocol is identical to the main Pawz webhook — same EngineEvent wire format.
 */

export interface PawzEvent {
  kind:
    | 'delta'
    | 'tool_request'
    | 'tool_result'
    | 'complete'
    | 'error'
    | 'thinking_delta'
    | 'tool_auto_approved'
    | 'canvas_push'
    | 'canvas_update';
  session_id: string;
  run_id: string;
  text?: string;
  tool_call?: {
    id: string;
    type: string;
    function: { name: string; arguments: string };
  };
  tool_tier?: 'safe' | 'reversible' | 'external' | 'dangerous' | 'unknown';
  round_number?: number;
  tool_call_id?: string;
  output?: string;
  success?: boolean;
  duration_ms?: number;
  tool_calls_count?: number;
  usage?: { input_tokens: number; output_tokens: number; total_tokens: number };
  model?: string;
  total_rounds?: number;
  max_rounds?: number;
  message?: string;
  tool_name?: string;
}

export interface CodeChatRequest {
  message: string;
  context?: string;
  user_id?: string;
  session_id?: string;
}

export class PawzCodeClient {
  constructor(
    private readonly baseUrl: string,
    private readonly authToken: string,
  ) {}

  async streamChat(
    req: CodeChatRequest,
    onEvent: (event: PawzEvent) => void,
    signal?: AbortSignal,
    onRunId?: (runId: string) => void,
  ): Promise<void> {
    const url = new URL('/chat/stream', this.baseUrl).toString();

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${this.authToken}`,
        'Content-Type': 'application/json',
        Accept: 'text/event-stream',
      },
      body: JSON.stringify({
        message: req.message,
        user_id: req.user_id ?? 'vscode',
        context: req.context,
        session_id: req.session_id,
      }),
      signal,
    });

    if (!response.ok) {
      const text = await response.text().catch(() => `HTTP ${response.status}`);
      throw new Error(`pawz-code ${response.status}: ${text}`);
    }

    const body = response.body;
    if (!body) throw new Error('Empty response from pawz-code server');

    const reader = body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let runIdEmitted = false;
    let streamCompleted = false;

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const blocks = buffer.split('\n\n');
        buffer = blocks.pop() ?? '';

        for (const block of blocks) {
          for (const line of block.split('\n')) {
            const trimmed = line.trim();
            if (!trimmed || trimmed.startsWith(':')) continue;
            if (trimmed.startsWith('data: ')) {
              try {
                const event = JSON.parse(trimmed.slice(6)) as PawzEvent;
                // Expose run_id to caller on first occurrence
                if (!runIdEmitted && onRunId && event.run_id) {
                  runIdEmitted = true;
                  onRunId(event.run_id);
                }
                if (event.kind === 'complete' || event.kind === 'error') {
                  streamCompleted = true;
                }
                onEvent(event);
              } catch {
                // malformed SSE frame — skip
              }
            }
          }
        }
      }

      // If the stream ended without a complete or error event, the daemon
      // dropped the connection mid-run (crash, restart, network loss).
      if (!streamCompleted) {
        throw new Error('Connection to pawz-code dropped before run completed');
      }
    } finally {
      reader.releaseLock();
    }
  }

  async isReachable(): Promise<boolean> {
    try {
      const resp = await fetch(new URL('/health', this.baseUrl).toString(), {
        signal: AbortSignal.timeout(2000),
      });
      return resp.ok;
    } catch {
      return false;
    }
  }
}
