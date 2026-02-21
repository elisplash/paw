// Projects View — Git integration (banner, actions, shell wrappers)

import { escHtml, escAttr } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { shortenRemote, type GitInfo } from './atoms';

// ── Shell state (set by index.ts) ──────────────────────────────────────────

let _shellAvailable = false;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _shellCommand: any = null;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function initShellRefs(shellCommand: any, available: boolean): void {
  _shellCommand = shellCommand;
  _shellAvailable = available;
}

// ── Git exec helper ────────────────────────────────────────────────────────

/** Run a git command in a directory and return stdout (or null on error). */
export async function gitExec(cwd: string, ...args: string[]): Promise<string | null> {
  if (!_shellAvailable || !_shellCommand) return null;
  try {
    const cmd = _shellCommand.create('git', args, { cwd });
    const result = await cmd.execute();
    if (result.code !== 0) return null;
    return (result.stdout as string).trim();
  } catch {
    return null;
  }
}

// ── Git info gathering ────────────────────────────────────────────────────

const _gitInfoCache = new Map<string, GitInfo>();

export function getCachedGitInfo(projectPath: string): GitInfo | undefined {
  return _gitInfoCache.get(projectPath);
}

export function clearGitInfoCache(path?: string): void {
  if (path) _gitInfoCache.delete(path);
  else _gitInfoCache.clear();
}

/** Gather git info for a project path. Cached until invalidated. */
export async function getGitInfo(projectPath: string, forceRefresh = false): Promise<GitInfo> {
  if (!forceRefresh && _gitInfoCache.has(projectPath)) {
    return _gitInfoCache.get(projectPath)!;
  }

  const noGit: GitInfo = { isRepo: false };

  const topLevel = await gitExec(projectPath, 'rev-parse', '--show-toplevel');
  if (!topLevel) {
    _gitInfoCache.set(projectPath, noGit);
    return noGit;
  }

  const info: GitInfo = { isRepo: true };

  // Branch
  info.branch = (await gitExec(projectPath, 'rev-parse', '--abbrev-ref', 'HEAD')) ?? undefined;

  // Remote URL
  info.remote = (await gitExec(projectPath, 'config', '--get', 'remote.origin.url')) ?? undefined;

  // Dirty file count
  const statusOut = await gitExec(projectPath, 'status', '--porcelain');
  if (statusOut !== null) {
    info.dirty = statusOut === '' ? 0 : statusOut.split('\n').filter((l) => l.trim()).length;
  }

  // Ahead/behind (only if upstream is set)
  const upstream = await gitExec(projectPath, 'rev-parse', '--abbrev-ref', '@{upstream}');
  if (upstream) {
    const abOut = await gitExec(
      projectPath,
      'rev-list',
      '--left-right',
      '--count',
      `HEAD...@{upstream}`,
    );
    if (abOut) {
      const [ahead, behind] = abOut.split(/\s+/).map(Number);
      info.ahead = ahead || 0;
      info.behind = behind || 0;
    }
  }

  // Last commit
  const logOut = await gitExec(projectPath, 'log', '-1', '--format=%s|||%ar');
  if (logOut) {
    const [msg, date] = logOut.split('|||');
    info.lastCommit = msg;
    info.lastCommitDate = date;
  }

  _gitInfoCache.set(projectPath, info);
  return info;
}

// ── Git banner rendering ──────────────────────────────────────────────────

export function renderGitBanner(git: GitInfo, projectPath: string): string {
  if (!git.isRepo) {
    return `
      <div class="git-banner git-banner--none" style="margin-top:12px;padding:10px 12px;border-radius:8px;background:var(--surface-2, rgba(255,255,255,0.04));font-size:12px;color:var(--text-muted)">
        <span style="opacity:0.6">Not a git repository</span>
        <button class="btn btn-sm git-action" data-action="init" data-path="${escAttr(projectPath)}" style="margin-left:auto;font-size:11px">
          git init
        </button>
      </div>`;
  }

  const branchBadge = git.branch
    ? `<span style="font-weight:600;font-family:var(--font-mono);font-size:12px;background:var(--accent-alpha, rgba(99,102,241,0.15));color:var(--accent);padding:2px 8px;border-radius:4px">${escHtml(git.branch)}</span>`
    : '';

  const dirtyBadge =
    git.dirty !== undefined && git.dirty > 0
      ? `<span style="font-size:11px;color:var(--warning)">● ${git.dirty} changed</span>`
      : `<span style="font-size:11px;color:var(--success)">● Clean</span>`;

  let syncBadge = '';
  if (git.ahead || git.behind) {
    const parts: string[] = [];
    if (git.ahead) parts.push(`↑${git.ahead}`);
    if (git.behind) parts.push(`↓${git.behind}`);
    syncBadge = `<span style="font-size:11px;color:var(--text-muted);font-family:var(--font-mono)">${parts.join(' ')}</span>`;
  }

  const remoteBadge = git.remote
    ? `<span style="font-size:11px;color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:250px" title="${escAttr(git.remote)}">${escHtml(shortenRemote(git.remote))}</span>`
    : '';

  const lastCommitLine = git.lastCommit
    ? `<div style="font-size:11px;color:var(--text-muted);margin-top:6px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
        Latest: ${escHtml(git.lastCommit)}${git.lastCommitDate ? ` <span style="opacity:0.6">(${escHtml(git.lastCommitDate)})</span>` : ''}
      </div>`
    : '';

  return `
    <div class="git-banner" style="margin-top:12px;padding:10px 12px;border-radius:8px;background:var(--surface-2, rgba(255,255,255,0.04))">
      <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
        <span class="ms ms-sm" style="flex-shrink:0;opacity:0.7">commit</span>
        ${branchBadge}
        ${dirtyBadge}
        ${syncBadge}
        ${remoteBadge}
      </div>
      ${lastCommitLine}
      <div class="git-actions" style="display:flex;gap:6px;margin-top:8px;flex-wrap:wrap">
        ${git.remote ? `<button class="btn btn-sm git-action" data-action="pull" data-path="${escAttr(projectPath)}">⬇ Pull</button>` : ''}
        ${git.remote ? `<button class="btn btn-sm git-action" data-action="push" data-path="${escAttr(projectPath)}">⬆ Push</button>` : ''}
        <button class="btn btn-sm git-action" data-action="commit" data-path="${escAttr(projectPath)}"><span class="ms ms-sm">save</span> Commit</button>
        <button class="btn btn-sm git-action" data-action="status" data-path="${escAttr(projectPath)}" style="margin-left:auto;opacity:0.7;font-size:11px">↻ Refresh</button>
      </div>
    </div>`;
}

// ── Git action handlers ───────────────────────────────────────────────────

export function bindGitActions(
  container: HTMLElement,
  projectPath: string,
  onRefresh: (path: string) => Promise<void>,
): void {
  container.querySelectorAll('.git-action').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const action = (btn as HTMLElement).dataset.action;
      const path = (btn as HTMLElement).dataset.path || projectPath;
      if (!action) return;

      const origText = btn.textContent;
      btn.textContent = '…';
      (btn as HTMLButtonElement).disabled = true;

      try {
        switch (action) {
          case 'pull': {
            const out = await gitExec(path, 'pull');
            if (out !== null) {
              showToast(
                out.includes('Already up to date') ? 'Already up to date' : 'Pull complete',
                'success',
              );
            } else {
              showToast('Pull failed — check remote & credentials', 'error');
            }
            break;
          }
          case 'push': {
            const out = await gitExec(path, 'push');
            if (out !== null) {
              showToast('Push complete', 'success');
            } else {
              showToast('Push failed — check remote & credentials', 'error');
            }
            break;
          }
          case 'commit': {
            const msg = prompt('Commit message:');
            if (!msg) break;
            const addOut = await gitExec(path, 'add', '-A');
            if (addOut === null) {
              showToast('git add failed', 'error');
              break;
            }
            const commitOut = await gitExec(path, 'commit', '-m', msg);
            if (commitOut !== null) {
              showToast('Committed!', 'success');
            } else {
              showToast('Commit failed — nothing to commit?', 'error');
            }
            break;
          }
          case 'init': {
            const initOut = await gitExec(path, 'init');
            if (initOut !== null) {
              showToast('Initialized git repo', 'success');
            } else {
              showToast('git init failed', 'error');
            }
            break;
          }
          case 'status': {
            // Just refresh git info
            break;
          }
        }

        // Refresh git info
        clearGitInfoCache(path);
        await onRefresh(path);
      } catch (err) {
        showToast(`Git error: ${err instanceof Error ? err.message : err}`, 'error');
      } finally {
        btn.textContent = origText;
        (btn as HTMLButtonElement).disabled = false;
      }
    });
  });
}
