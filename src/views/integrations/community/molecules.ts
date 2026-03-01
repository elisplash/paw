// src/views/integrations/community/molecules.ts — DOM rendering + IPC
//
// Molecule-level: builds HTML, binds events, calls Tauri commands.

import { invoke } from '@tauri-apps/api/core';
import { showToast } from '../../../components/toast';
import { kineticStagger } from '../../../components/kinetic-row';
import {
  escHtml,
  formatDownloads,
  relativeDate,
  sortPackages,
  isInstalled,
  displayName,
  SORT_OPTIONS,
  DEBOUNCE_MS,
  type CommunityPackage,
  type InstalledPackage,
  type CommunityTab,
  type CommunitySortOption,
  type PackageCredentialInfo,
  type N8nCredentialSchema,
} from './atoms';

// ── Module state ───────────────────────────────────────────────────────

let _tab: CommunityTab = 'browse';
let _query = '';
let _sortOption: CommunitySortOption = 'downloads';
let _results: CommunityPackage[] = [];
let _installed: InstalledPackage[] = [];
let _loading = false;
const _installing: Set<string> = new Set();
/** Package that was just installed — shows post-install guidance overlay. */
let _justInstalled: string | null = null;
/** Credential schemas discovered for the just-installed package. */
let _credentialInfo: PackageCredentialInfo | null = null;
/** Post-install credential form state: 'form' | 'saving' | 'done' | 'error' | 'loading' */
let _credFormState: 'loading' | 'form' | 'saving' | 'done' | 'error' = 'loading';
let _credFormError = '';
let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
let _container: HTMLElement | null = null;

// ── Public API ─────────────────────────────────────────────────────────

/** Mount the community browser into a container element. */
export function mountCommunityBrowser(container: HTMLElement): void {
  _container = container;
  _render();
  _fetchInstalled();
  // Pre-populate with popular packages
  _search('n8n');
}

// ── Rendering ──────────────────────────────────────────────────────────

function _render(): void {
  if (!_container) return;

  _container.innerHTML = `
    <div class="community-browser">
      <div class="community-header">
        <div class="community-tabs">
          <button class="community-tab ${_tab === 'browse' ? 'active' : ''}" data-tab="browse">
            <span class="ms ms-sm">explore</span> Browse
          </button>
          <button class="community-tab ${_tab === 'installed' ? 'active' : ''}" data-tab="installed">
            <span class="ms ms-sm">inventory_2</span> Installed
            ${_installed.length > 0 ? `<span class="community-tab-badge">${_installed.length}</span>` : ''}
          </button>
        </div>
      </div>

      ${_justInstalled ? _renderPostInstallGuide(_justInstalled) : _tab === 'browse' ? _renderBrowseTab() : _renderInstalledTab()}
    </div>
  `;

  _wireEvents();
}

function _renderBrowseTab(): string {
  return `
    <div class="community-toolbar">
      <div class="community-search-wrap">
        <span class="ms ms-sm">search</span>
        <input type="text" class="community-search"
               placeholder="Search 25,000+ community packages…"
               value="${escHtml(_query)}" />
      </div>
      <select class="community-sort">
        ${SORT_OPTIONS.map(
          (o) =>
            `<option value="${o.value}" ${_sortOption === o.value ? 'selected' : ''}>${o.label}</option>`,
        ).join('')}
      </select>
    </div>

    <div class="community-results">
      ${_loading ? _renderLoading() : _renderPackageList()}
    </div>
  `;
}

function _renderInstalledTab(): string {
  if (_installed.length === 0) {
    return `
      <div class="community-empty">
        <span class="ms ms-lg">inventory_2</span>
        <p>No community packages installed yet.</p>
        <p class="community-empty-hint">Browse and install packages to extend your n8n automations.</p>
      </div>
    `;
  }

  return `
    <div class="community-installed-list">
      ${_installed
        .map(
          (pkg) => `
        <div class="community-installed-row k-row k-spring k-breathe k-status-healthy" data-pkg="${escHtml(pkg.packageName)}">
          <span class="ms community-installed-icon">extension</span>
          <div class="community-installed-info">
            <span class="community-installed-name">${escHtml(pkg.packageName)}</span>
            <span class="community-installed-meta">
              v${escHtml(pkg.installedVersion)} · ${pkg.installedNodes.length} node${pkg.installedNodes.length !== 1 ? 's' : ''}
            </span>
          </div>
          <div class="community-installed-nodes">
            ${pkg.installedNodes
              .slice(0, 3)
              .map((n) => `<span class="community-node-chip">${escHtml(n.name)}</span>`)
              .join('')}
            ${pkg.installedNodes.length > 3 ? `<span class="community-node-chip community-node-more">+${pkg.installedNodes.length - 3}</span>` : ''}
          </div>
          <button class="btn btn-ghost btn-sm community-uninstall-btn" data-pkg="${escHtml(pkg.packageName)}"
                  title="Uninstall">
            <span class="ms ms-sm">delete</span>
          </button>
        </div>
      `,
        )
        .join('')}
    </div>
  `;
}

function _renderPackageList(): string {
  if (_results.length === 0 && _query) {
    return `
      <div class="community-empty">
        <span class="ms ms-lg">search_off</span>
        <p>No packages match "${escHtml(_query)}"</p>
        <p class="community-empty-hint">Try a broader search term or check spelling.</p>
      </div>
    `;
  }

  if (_results.length === 0) {
    return `
      <div class="community-empty">
        <span class="ms ms-lg">explore</span>
        <p>Search for community packages</p>
        <p class="community-empty-hint">Try "puppeteer", "redis", "telegram", or "aws"</p>
      </div>
    `;
  }

  const sorted = sortPackages(_results, _sortOption);

  return `
    <div class="community-package-grid">
      ${sorted.map((pkg) => _renderPackageCard(pkg)).join('')}
    </div>
    <div class="community-footer">
      Showing ${sorted.length} results · Data from <a href="https://www.ncnodes.com" target="_blank" rel="noopener" style="color:var(--accent)">ncnodes.com</a> + npm registry
    </div>
  `;
}

function _renderPackageCard(pkg: CommunityPackage): string {
  const installed = isInstalled(pkg, _installed);
  const isInstalling = _installing.has(pkg.package_name);
  const name = displayName(pkg.package_name);

  return `
    <div class="community-card k-row k-spring ${installed ? 'community-card-installed k-breathe k-status-healthy' : 'k-status-idle'}"
         data-pkg="${escHtml(pkg.package_name)}">
      <div class="community-card-header">
        <span class="ms community-card-icon">${installed ? 'check_circle' : 'extension'}</span>
        <div class="community-card-title">
          <span class="community-card-name">${escHtml(name)}</span>
          <span class="community-card-pkg">${escHtml(pkg.package_name)}</span>
        </div>
      </div>
      <div class="community-card-desc">${escHtml(pkg.description || 'No description available.')}</div>
      <div class="community-card-meta">
        <span class="community-card-stat" title="Weekly downloads">
          <span class="ms ms-sm">download</span> ${formatDownloads(pkg.weekly_downloads)}
        </span>
        <span class="community-card-stat" title="Last updated">
          <span class="ms ms-sm">schedule</span> ${relativeDate(pkg.last_updated)}
        </span>
        <span class="community-card-stat" title="Version">
          v${escHtml(pkg.version)}
        </span>
      </div>
      <div class="community-card-author">by ${escHtml(pkg.author || 'Unknown')}</div>
      <div class="community-card-actions">
        ${
          installed
            ? '<span class="community-installed-badge"><span class="ms ms-sm">check_circle</span> Installed</span>'
            : isInstalling
              ? '<button class="btn btn-sm community-install-btn" disabled><span class="ms ms-sm k-spin">progress_activity</span> Installing…</button>'
              : `<button class="btn btn-sm btn-ghost community-install-btn" data-pkg="${escHtml(pkg.package_name)}">
                  <span class="ms ms-sm">add_circle</span> Install
                </button>`
        }
        ${
          pkg.repository_url
            ? `<a href="${escHtml(pkg.repository_url)}" target="_blank" rel="noopener"
                class="btn btn-sm btn-ghost" title="View source">
                <span class="ms ms-sm">open_in_new</span>
              </a>`
            : ''
        }
      </div>
    </div>
  `;
}

function _renderLoading(): string {
  return `
    <div class="community-loading">
      <span class="ms ms-lg k-spin">progress_activity</span>
      <p>Searching packages…</p>
    </div>
  `;
}

// ── Post-install guidance ──────────────────────────────────────────────

function _renderPostInstallGuide(packageName: string): string {
  const name = displayName(packageName);
  const pkg = _installed.find((p) => p.packageName === packageName);
  const nodeCount = pkg?.installedNodes.length ?? 0;
  const nodeList = pkg?.installedNodes.slice(0, 5) ?? [];

  return `
    <div class="community-post-install">
      <div class="community-post-install-icon">
        <span class="ms" style="font-size:48px;color:var(--success,#4caf50)">check_circle</span>
      </div>
      <h2 class="community-post-install-title">${escHtml(name)} Installed</h2>
      <p class="community-post-install-subtitle">
        ${
          nodeCount > 0
            ? `${nodeCount} new node${nodeCount !== 1 ? 's' : ''} registered — ready to use.`
            : `Package installed — n8n will register the nodes on next restart.`
        }
      </p>

      ${
        nodeList.length > 0
          ? `
        <div class="community-post-install-nodes">
          ${nodeList.map((n) => `<span class="community-node-chip">${escHtml(n.name)}</span>`).join('')}
          ${
            (pkg?.installedNodes.length ?? 0) > 5
              ? `<span class="community-node-chip community-node-more">+${(pkg?.installedNodes.length ?? 0) - 5}</span>`
              : ''
          }
        </div>
      `
          : ''
      }

      ${_renderCredentialSection(packageName, name)}

      <div class="community-post-install-actions">
        ${
          _credFormState === 'done'
            ? `<button class="btn btn-primary" id="post-install-dismiss">
              <span class="ms ms-sm">check</span> Done
            </button>`
            : `<button class="btn btn-ghost" id="post-install-dismiss">
              <span class="ms ms-sm">${_credFormState === 'loading' ? 'arrow_back' : 'skip_next'}</span>
              ${_credFormState === 'loading' ? 'Back to Browser' : 'Skip for now'}
            </button>`
        }
      </div>
    </div>
  `;
}

/** Render the inline credential form section within post-install. */
function _renderCredentialSection(_packageName: string, name: string): string {
  if (_credFormState === 'loading') {
    return `
      <div class="community-post-install-steps">
        <div class="community-cred-loading">
          <span class="ms ms-sm k-spin">progress_activity</span>
          Detecting credential requirements…
        </div>
      </div>
    `;
  }

  if (_credFormState === 'done') {
    return `
      <div class="community-post-install-steps">
        <div class="community-cred-success">
          <span class="ms ms-sm" style="color:var(--success,#4caf50)">check_circle</span>
          <strong>Credentials saved!</strong> — ${escHtml(name)} is ready to use.
          Ask your agent to use ${escHtml(name)} actions from any conversation.
        </div>
      </div>
    `;
  }

  if (!_credentialInfo || _credentialInfo.credential_types.length === 0) {
    return `
      <div class="community-post-install-steps">
        <div class="community-cred-success">
          <span class="ms ms-sm" style="color:var(--success,#4caf50)">check_circle</span>
          <strong>No credentials needed</strong> — ${escHtml(name)} is ready to use.
          Ask your agent to use ${escHtml(name)} actions from any conversation.
        </div>
      </div>
    `;
  }

  // Render one form per credential type (most packages need just one)
  return _credentialInfo.credential_types.map((schema) => _renderCredentialForm(schema)).join('');
}

/** Render an inline credential form for one credential type. */
function _renderCredentialForm(schema: N8nCredentialSchema): string {
  const formFields = schema.fields.filter(
    (f) => f.field_type !== 'notice' && f.field_type !== 'hidden',
  );

  return `
    <div class="community-post-install-steps" data-cred-type="${escHtml(schema.credential_type)}">
      <h3 class="community-post-install-steps-title">
        <span class="ms ms-sm">key</span> Connect ${escHtml(schema.display_name)}
      </h3>

      ${
        _credFormState === 'error'
          ? `
        <div class="community-cred-error">
          <span class="ms ms-sm">error</span> ${escHtml(_credFormError || 'Failed to save credentials')}
        </div>
      `
          : ''
      }

      <div class="community-cred-fields">
        ${formFields
          .map(
            (f) => `
          <div class="community-cred-field">
            <label for="cred-${escHtml(f.name)}">
              ${escHtml(f.display_name)}${f.required ? ' <span class="community-cred-required">*</span>' : ''}
            </label>
            ${f.description ? `<span class="community-cred-hint">${escHtml(f.description)}</span>` : ''}
            ${
              f.field_type === 'options' && f.options.length > 0
                ? `<select id="cred-${escHtml(f.name)}" class="input community-cred-input"
                         data-cred-key="${escHtml(f.name)}">
                  ${f.options.map((o) => `<option value="${escHtml(o)}"${f.default_value === o ? ' selected' : ''}>${escHtml(o)}</option>`).join('')}
                </select>`
                : `<div class="community-cred-input-wrap">
                  <input type="${f.is_secret ? 'password' : 'text'}"
                         id="cred-${escHtml(f.name)}"
                         class="input community-cred-input"
                         data-cred-key="${escHtml(f.name)}"
                         placeholder="${escHtml(f.placeholder ?? '')}"
                         value="${escHtml(f.default_value ?? '')}"
                         ${f.required ? 'required' : ''}
                         autocomplete="off" />
                  ${
                    f.is_secret
                      ? `
                    <button class="btn btn-ghost btn-xs community-cred-toggle-vis" data-field="${escHtml(f.name)}" title="Toggle visibility">
                      <span class="ms ms-sm">visibility</span>
                    </button>
                  `
                      : ''
                  }
                </div>`
            }
          </div>
        `,
          )
          .join('')}
      </div>

      <div class="community-cred-form-actions">
        <button class="btn btn-primary community-cred-save-btn"
                data-cred-type="${escHtml(schema.credential_type)}"
                data-cred-display="${escHtml(schema.display_name)}"
                ${_credFormState === 'saving' ? 'disabled' : ''}>
          ${
            _credFormState === 'saving'
              ? '<span class="ms ms-sm k-spin">progress_activity</span> Saving…'
              : '<span class="ms ms-sm">check</span> Save & Connect'
          }
        </button>
      </div>
    </div>
  `;
}

// ── Data fetching ──────────────────────────────────────────────────────

async function _search(query: string): Promise<void> {
  _loading = true;
  _render();

  try {
    const results = await invoke<CommunityPackage[]>('engine_n8n_search_ncnodes', {
      query,
      limit: 30,
    });
    _results = results;
  } catch (e) {
    const err = e instanceof Error ? e.message : String(e);
    showToast(`Search failed: ${err}`, 'error');
    _results = [];
  } finally {
    _loading = false;
    _render();
    // Stagger animate results
    const grid = _container?.querySelector('.community-package-grid');
    if (grid) kineticStagger(grid as HTMLElement, '.community-card');
  }
}

async function _fetchInstalled(): Promise<void> {
  try {
    const pkgs = await invoke<InstalledPackage[]>('engine_n8n_community_packages_list');
    _installed = pkgs;

    // Re-render if on installed tab or to update badges
    if (_tab === 'installed' || _installed.length > 0) _render();
  } catch (e) {
    console.warn('[community] Failed to fetch installed packages:', e);
  }
}

async function _installPackage(packageName: string): Promise<void> {
  _installing.add(packageName);
  _render();

  // Show persistent toast since npm install in Docker can take minutes
  showToast(`Installing ${packageName}… this may take a minute or two.`, 'info');

  try {
    await invoke<InstalledPackage>('engine_n8n_community_packages_install', {
      packageName,
    });

    // Backend handles MCP bridge reconnection + tool index invalidation.
    // Just refresh the installed list from the backend.
    await _fetchInstalled();

    // Show post-install guidance with inline credential form
    _justInstalled = packageName;
    _credentialInfo = null;
    _credFormState = 'loading';
    _credFormError = '';
    _render();

    // Fetch credential schema for this package's nodes in the background
    _fetchCredentialSchema(packageName);
  } catch (e) {
    const err = e instanceof Error ? e.message : String(e);
    console.error(`[community] Install failed for ${packageName}:`, err);
    showToast(`Install failed: ${err}`, 'error');
  } finally {
    _installing.delete(packageName);
    _render();
  }
}

/** Fetch the credential schema for a just-installed package's nodes. */
async function _fetchCredentialSchema(packageName: string): Promise<void> {
  try {
    const info = await invoke<PackageCredentialInfo>('engine_n8n_package_credential_schema', {
      packageName,
    });
    _credentialInfo = info;
    _credFormState = info.credential_types.length > 0 ? 'form' : 'form';
    _render();
  } catch (e) {
    console.warn('[community] Could not fetch credential schema:', e);
    // If schema fetch fails, show the form-less success state
    _credentialInfo = null;
    _credFormState = 'form';
    _render();
  }
}

/** Save credentials for a just-installed package by pushing them to n8n. */
async function _saveCredentials(credentialType: string, displayName: string): Promise<void> {
  if (!_container) return;

  // Gather field values from the form
  const section = _container.querySelector(`[data-cred-type="${credentialType}"]`);
  if (!section) return;

  const data: Record<string, string> = {};
  const inputs = section.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
    '.community-cred-input',
  );
  for (const input of inputs) {
    const key = input.dataset.credKey;
    if (key) data[key] = input.value;
  }

  // Validate required fields
  const schema = _credentialInfo?.credential_types.find(
    (s) => s.credential_type === credentialType,
  );
  if (schema) {
    const missing = schema.fields.filter((f) => f.required && !data[f.name]?.trim());
    if (missing.length > 0) {
      _credFormState = 'error';
      _credFormError = `Missing required: ${missing.map((f) => f.display_name).join(', ')}`;
      _render();
      return;
    }
  }

  _credFormState = 'saving';
  _render();

  try {
    // Push credentials directly to n8n via its REST API
    await invoke('engine_n8n_create_credential', {
      credentialType,
      credentialName: displayName,
      credentialData: data,
    });

    _credFormState = 'done';
    showToast(`${displayName} credentials saved!`, 'success');
  } catch (e) {
    const err = e instanceof Error ? e.message : String(e);
    console.error('[community] Save credentials failed:', err);
    _credFormState = 'error';
    _credFormError = err;
  }

  _render();
}

async function _uninstallPackage(packageName: string): Promise<void> {
  try {
    await invoke('engine_n8n_community_packages_uninstall', { packageName });
    showToast(`Uninstalled ${packageName}`, 'success');
    await _fetchInstalled();
    _render();
  } catch (e) {
    const err = e instanceof Error ? e.message : String(e);
    showToast(`Uninstall failed: ${err}`, 'error');
  }
}

// ── Event wiring ───────────────────────────────────────────────────────

function _wireEvents(): void {
  if (!_container) return;

  // Tab switching
  _container.querySelectorAll('.community-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      _tab = (btn as HTMLElement).dataset.tab as CommunityTab;
      _render();
      if (_tab === 'installed') _fetchInstalled();
    });
  });

  // Search input with debounce
  const searchInput = _container.querySelector('.community-search') as HTMLInputElement;
  if (searchInput) {
    searchInput.addEventListener('input', () => {
      _query = searchInput.value;
      if (_debounceTimer) clearTimeout(_debounceTimer);
      _debounceTimer = setTimeout(() => {
        if (_query.trim().length >= 2) {
          _search(_query.trim());
        }
      }, DEBOUNCE_MS);
    });
    // Focus search on mount
    searchInput.focus();
  }

  // Sort select
  const sortSelect = _container.querySelector('.community-sort') as HTMLSelectElement;
  if (sortSelect) {
    sortSelect.addEventListener('change', () => {
      _sortOption = sortSelect.value as CommunitySortOption;
      _render();
    });
  }

  // Install buttons
  _container.querySelectorAll('.community-install-btn[data-pkg]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const pkg = (btn as HTMLElement).dataset.pkg;
      if (pkg) _installPackage(pkg);
    });
  });

  // Uninstall buttons
  _container.querySelectorAll('.community-uninstall-btn[data-pkg]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const pkg = (btn as HTMLElement).dataset.pkg;
      if (pkg && confirm(`Uninstall ${pkg}?`)) _uninstallPackage(pkg);
    });
  });

  // Post-install guidance actions
  document.getElementById('post-install-dismiss')?.addEventListener('click', () => {
    _justInstalled = null;
    _credentialInfo = null;
    _credFormState = 'loading';
    _credFormError = '';
    _render();
    // Re-fetch installed list in background (n8n may have finished loading by now)
    _fetchInstalled();
  });

  // Credential form: Save & Connect buttons
  _container.querySelectorAll('.community-cred-save-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const credType = (btn as HTMLElement).dataset.credType;
      const credDisplay = (btn as HTMLElement).dataset.credDisplay;
      if (credType && credDisplay) _saveCredentials(credType, credDisplay);
    });
  });

  // Credential form: password visibility toggles
  _container.querySelectorAll('.community-cred-toggle-vis').forEach((btn) => {
    btn.addEventListener('click', () => {
      const key = (btn as HTMLElement).dataset.field;
      const input = _container?.querySelector(`#cred-${key}`) as HTMLInputElement;
      if (!input) return;
      const isPassword = input.type === 'password';
      input.type = isPassword ? 'text' : 'password';
      const icon = btn.querySelector('.ms');
      if (icon) icon.textContent = isPassword ? 'visibility_off' : 'visibility';
    });
  });

  // Stagger installed rows
  const installedList = _container.querySelector('.community-installed-list');
  if (installedList) kineticStagger(installedList as HTMLElement, '.community-installed-row');
}
