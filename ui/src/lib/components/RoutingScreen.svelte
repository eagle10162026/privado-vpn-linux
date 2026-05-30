<script lang="ts">
  import type { RoutingRule, ServerEntry } from '$lib/tauri';

  interface Props {
    rules: RoutingRule[];
    servers: ServerEntry[];
    activeExit: string;
    /** Connection state from vpnStatus(); drives the "applied / pending" line. */
    connectionState: string;
    onAddRule: (rule: RoutingRule) => void;
    onUpdateRule: (rule: RoutingRule) => void;
    onDeleteRule: (id: string) => void;
    onReorderRule: (order: string[]) => void;
    onSetActiveExit: (server: string) => void;
  }

  let {
    rules = [],
    servers = [],
    activeExit = '',
    connectionState = 'Disconnected',
    onAddRule,
    onUpdateRule,
    onDeleteRule,
    onReorderRule,
    onSetActiveExit
  }: Props = $props();

  // Flag emoji table reused from ServerList.svelte so the exit picker looks
  // identical to the server browser.
  const COUNTRY_FLAGS: Record<string, string> = {
    US: '🇺🇸', GB: '🇬🇧', DE: '🇩🇪', NL: '🇳🇱', CH: '🇨🇭', FR: '🇫🇷',
    JP: '🇯🇵', SG: '🇸🇬', CA: '🇨🇦', AU: '🇦🇺', SE: '🇸🇪', NO: '🇳🇴',
    DK: '🇩🇰', FI: '🇫🇮', AT: '🇦🇹', BE: '🇧🇪', IT: '🇮🇹', ES: '🇪🇸',
    PT: '🇵🇹', IE: '🇮🇪', PL: '🇵🇱', CZ: '🇨🇿', RO: '🇷🇴', HU: '🇭🇺',
    BR: '🇧🇷', MX: '🇲🇽', AR: '🇦🇷', IN: '🇮🇳', KR: '🇰🇷', HK: '🇭🇰',
    TW: '🇹🇼', IL: '🇮🇱', TR: '🇹🇷', ZA: '🇿🇦', UA: '🇺🇦', RU: '🇷🇺',
    BG: '🇧🇬', HR: '🇭🇷', LV: '🇱🇻', IS: '🇮🇸', NZ: '🇳🇿', PH: '🇵🇭',
    MY: '🇲🇾', TH: '🇹🇭', CO: '🇨🇴', CL: '🇨🇱', PE: '🇵🇪', EG: '🇪🇬'
  };

  function getFlag(cc: string): string {
    return COUNTRY_FLAGS[cc?.toUpperCase()] ?? '🌍';
  }

  type MatchType = RoutingRule['match_type'];
  type Action = RoutingRule['action'];

  const MATCH_LABELS: Record<MatchType, string> = {
    process: 'Process',
    domain: 'Domain',
    ip_cidr: 'IP / CIDR',
    port: 'Port',
    port_range: 'Port range'
  };

  const MATCH_HINTS: Record<MatchType, string> = {
    process: 'App name (e.g. firefox) or uid:1000',
    domain: 'Fully-qualified domain, e.g. netflix.com',
    ip_cidr: 'CIDR block, e.g. 10.0.0.0/24 or 1.2.3.4/32',
    port: 'Single port, e.g. 443',
    port_range: 'Range lo-hi, e.g. 8000-8100'
  };

  const MATCH_PLACEHOLDERS: Record<MatchType, string> = {
    process: 'firefox',
    domain: 'example.com',
    ip_cidr: '10.0.0.0/24',
    port: '443',
    port_range: '8000-8100'
  };

  // --- live-apply status line ---
  let appliedStatus = $derived(
    connectionState === 'Connected'
      ? { text: 'Connected — rule changes apply live.', live: true }
      : { text: 'Not connected — rules apply on next connect.', live: false }
  );

  // --- active-exit resolution ---
  function exitLabel(rule: RoutingRule): string {
    if (!rule.exit_server) return 'Active tunnel';
    const s = servers.find(sv => sv.hostname === rule.exit_server);
    if (s) return s.city ? `${s.city}, ${s.country}` : s.country;
    if (rule.exit_server.startsWith('cc:')) return rule.exit_server.slice(3).toUpperCase();
    if (rule.exit_server.startsWith('city:')) return rule.exit_server.slice(5);
    return rule.exit_server;
  }

  let activeExitLabel = $derived.by(() => {
    if (!activeExit) return 'Auto';
    const s = servers.find(sv => sv.hostname === activeExit);
    if (s) return s.city ? `${s.city}, ${s.country}` : s.country;
    return activeExit;
  });

  // --- modal state ---
  let modalOpen = $state(false);
  let editingId = $state<string | null>(null);
  let formName = $state('');
  let formMatchType = $state<MatchType>('domain');
  let formMatchValue = $state('');
  let formProtocol = $state<'both' | 'tcp' | 'udp'>('both');
  let formAction = $state<Action>('vpn');
  let formExitServer = $state(''); // '' = use active tunnel
  let formError = $state('');

  let isPortType = $derived(formMatchType === 'port' || formMatchType === 'port_range');

  // Exit picker (searchable, grouped by country).
  let exitSearch = $state('');
  let exitPickerOpen = $state(false);

  let groupedExits = $derived.by(() => {
    const q = exitSearch.trim().toLowerCase();
    const filtered = q
      ? servers.filter(s =>
          s.country.toLowerCase().includes(q) ||
          s.city.toLowerCase().includes(q) ||
          s.country_code.toLowerCase().includes(q) ||
          s.hostname.toLowerCase().includes(q))
      : servers;
    const groups: Record<string, ServerEntry[]> = {};
    for (const s of filtered) {
      const key = `${s.country_code}_${s.country}`;
      (groups[key] ??= []).push(s);
    }
    return Object.values(groups)
      .map(entries => ({
        cc: entries[0].country_code,
        country: entries[0].country,
        entries: [...entries].sort((a, b) => a.load - b.load)
      }))
      .sort((a, b) => a.country.localeCompare(b.country));
  });

  let formExitLabel = $derived.by(() => {
    if (!formExitServer) return '(use active tunnel)';
    const s = servers.find(sv => sv.hostname === formExitServer);
    if (s) return `${getFlag(s.country_code)} ${s.city ? `${s.city}, ${s.country}` : s.country}`;
    return formExitServer;
  });

  function openAdd() {
    editingId = null;
    formName = '';
    formMatchType = 'domain';
    formMatchValue = '';
    formProtocol = 'both';
    formAction = 'vpn';
    formExitServer = '';
    formError = '';
    exitSearch = '';
    exitPickerOpen = false;
    modalOpen = true;
  }

  function openEdit(rule: RoutingRule) {
    editingId = rule.id;
    formName = rule.name;
    formMatchType = rule.match_type;
    formMatchValue = rule.match_value;
    formProtocol = (rule.protocol as 'tcp' | 'udp' | null) ?? 'both';
    formAction = rule.action;
    formExitServer = rule.exit_server ?? '';
    formError = '';
    exitSearch = '';
    exitPickerOpen = false;
    modalOpen = true;
  }

  function closeModal() {
    modalOpen = false;
    exitPickerOpen = false;
  }

  function selectExit(hostname: string) {
    formExitServer = hostname;
    exitPickerOpen = false;
  }

  function saveRule() {
    const name = formName.trim();
    const value = formMatchValue.trim();
    if (!value) {
      formError = 'A match value is required.';
      return;
    }
    const rule: RoutingRule = {
      id: editingId ?? '',
      enabled: editingId ? (rules.find(r => r.id === editingId)?.enabled ?? true) : true,
      name: name || `${MATCH_LABELS[formMatchType]}: ${value}`,
      match_type: formMatchType,
      match_value: value,
      protocol: isPortType && formProtocol !== 'both' ? formProtocol : null,
      action: formAction,
      exit_server: formExitServer || null,
      priority: editingId ? (rules.find(r => r.id === editingId)?.priority ?? 0) : rules.length
    };
    if (editingId) {
      onUpdateRule(rule);
    } else {
      onAddRule(rule);
    }
    closeModal();
  }

  function toggleEnabled(rule: RoutingRule) {
    onUpdateRule({ ...rule, enabled: !rule.enabled });
  }

  function moveRule(index: number, dir: -1 | 1) {
    const target = index + dir;
    if (target < 0 || target >= rules.length) return;
    const order = rules.map(r => r.id);
    [order[index], order[target]] = [order[target], order[index]];
    onReorderRule(order);
  }

  // The active-exit picker mirrors the per-rule one but applies immediately.
  let activeExitPickerOpen = $state(false);
  let activeExitSearch = $state('');

  let groupedActiveExits = $derived.by(() => {
    const q = activeExitSearch.trim().toLowerCase();
    const filtered = q
      ? servers.filter(s =>
          s.country.toLowerCase().includes(q) ||
          s.city.toLowerCase().includes(q) ||
          s.country_code.toLowerCase().includes(q) ||
          s.hostname.toLowerCase().includes(q))
      : servers;
    const groups: Record<string, ServerEntry[]> = {};
    for (const s of filtered) {
      const key = `${s.country_code}_${s.country}`;
      (groups[key] ??= []).push(s);
    }
    return Object.values(groups)
      .map(entries => ({
        cc: entries[0].country_code,
        country: entries[0].country,
        entries: [...entries].sort((a, b) => a.load - b.load)
      }))
      .sort((a, b) => a.country.localeCompare(b.country));
  });

  function chooseActiveExit(hostname: string) {
    activeExitPickerOpen = false;
    onSetActiveExit(hostname);
  }
</script>

<div class="routing-page">
  <!-- Active exit -->
  <div class="routing-section card">
    <div class="section-title">Active Exit</div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Tunnel exit server</div>
        <div class="setting-desc">Single active tunnel. Per-rule exits act as preferred exits.</div>
      </div>
      <button class="exit-current" onclick={() => (activeExitPickerOpen = !activeExitPickerOpen)}>
        <span class="exit-current-label">{activeExitLabel}</span>
        <span class="exit-current-chev">{activeExitPickerOpen ? '▲' : '▼'}</span>
      </button>
    </div>
    {#if activeExitPickerOpen}
      <div class="exit-picker">
        <input
          type="text"
          class="exit-search"
          bind:value={activeExitSearch}
          placeholder="Search country or city..."
        />
        <div class="exit-list">
          {#each groupedActiveExits as group}
            <div class="exit-group-head">
              <span class="exit-flag">{getFlag(group.cc)}</span>
              <span class="exit-group-name">{group.country}</span>
            </div>
            {#each group.entries as s}
              <button
                class="exit-option"
                class:selected={s.hostname === activeExit}
                onclick={() => chooseActiveExit(s.hostname)}
              >
                <span class="exit-option-name">{s.city || s.name}</span>
                <span class="exit-option-host">{s.hostname}</span>
                <span class="exit-option-load">{s.load.toFixed(0)}%</span>
              </button>
            {/each}
          {/each}
          {#if groupedActiveExits.length === 0}
            <div class="exit-empty">No servers loaded.</div>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <!-- Rules -->
  <div class="routing-section card">
    <div class="section-head">
      <div class="section-title" style="margin-bottom: 0;">Routing Rules</div>
      <button class="btn-add-rule" onclick={openAdd}>+ Add Rule</button>
    </div>

    <div class="apply-status" class:live={appliedStatus.live}>
      <span class="apply-dot"></span>
      {appliedStatus.text}
    </div>

    {#if rules.length === 0}
      <div class="empty-rules">
        <div class="empty-icon">🧭</div>
        <div class="empty-text">No routing rules yet. Add a rule to send specific apps, domains, IPs, or ports through the VPN or directly.</div>
      </div>
    {:else}
      <div class="rules-list">
        {#each rules as rule, i (rule.id)}
          <div class="rule-row" class:disabled={!rule.enabled}>
            <label class="toggle">
              <input
                type="checkbox"
                checked={rule.enabled}
                onchange={() => toggleEnabled(rule)}
              />
              <span class="toggle-track"></span>
              <span class="toggle-thumb"></span>
            </label>

            <div class="rule-body">
              <div class="rule-top">
                <span class="rule-name">{rule.name}</span>
                <span class="chip" class:vpn={rule.action === 'vpn'} class:direct={rule.action === 'direct'}>
                  {rule.action === 'vpn' ? 'VPN' : 'Direct'}
                </span>
              </div>
              <div class="rule-meta">
                <span class="match-type">{MATCH_LABELS[rule.match_type]}</span>
                <span class="match-value">{rule.match_value}</span>
                {#if rule.protocol}<span class="proto-chip">{rule.protocol.toUpperCase()}</span>{/if}
                <span class="exit-chip">↪ {exitLabel(rule)}</span>
              </div>
            </div>

            <div class="rule-actions">
              <button class="icon-btn" title="Move up" disabled={i === 0} onclick={() => moveRule(i, -1)}>▲</button>
              <button class="icon-btn" title="Move down" disabled={i === rules.length - 1} onclick={() => moveRule(i, 1)}>▼</button>
              <button class="icon-btn" title="Edit" onclick={() => openEdit(rule)}>✎</button>
              <button class="icon-btn danger" title="Delete" onclick={() => onDeleteRule(rule.id)}>✕</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if modalOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="0"
    onclick={closeModal}
    onkeydown={(e) => { if (e.key === 'Escape') closeModal(); }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === 'Escape') closeModal(); }}
    >
      <div class="modal-title">{editingId ? 'Edit Rule' : 'Add Rule'}</div>

      <div class="form-group">
        <label class="form-label" for="rule-name">Name</label>
        <input id="rule-name" type="text" class="form-input" bind:value={formName} placeholder="Optional — auto-generated if blank" />
      </div>

      <div class="form-group">
        <label class="form-label" for="rule-match-type">Match type</label>
        <select id="rule-match-type" class="form-input" bind:value={formMatchType}>
          <option value="process">Process</option>
          <option value="domain">Domain</option>
          <option value="ip_cidr">IP / CIDR</option>
          <option value="port">Port</option>
          <option value="port_range">Port range</option>
        </select>
      </div>

      <div class="form-group">
        <label class="form-label" for="rule-match-value">Match value</label>
        <input
          id="rule-match-value"
          type="text"
          class="form-input mono"
          bind:value={formMatchValue}
          placeholder={MATCH_PLACEHOLDERS[formMatchType]}
        />
        <div class="form-hint">{MATCH_HINTS[formMatchType]}</div>
      </div>

      {#if isPortType}
        <div class="form-group">
          <label class="form-label" for="rule-protocol">Protocol</label>
          <select id="rule-protocol" class="form-input" bind:value={formProtocol}>
            <option value="both">Both (TCP + UDP)</option>
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
          </select>
        </div>
      {/if}

      <div class="form-group">
        <span class="form-label">Action</span>
        <div class="segmented">
          <button
            class="seg-btn"
            class:active={formAction === 'vpn'}
            onclick={() => (formAction = 'vpn')}
          >Through VPN</button>
          <button
            class="seg-btn"
            class:active={formAction === 'direct'}
            onclick={() => (formAction = 'direct')}
          >Direct bypass</button>
        </div>
        {#if isPortType && formAction === 'vpn'}
          <div class="form-hint">Port rules need a full-tunnel exit to match.</div>
        {/if}
      </div>

      <div class="form-group">
        <span class="form-label">Exit server</span>
        <button class="exit-current full" onclick={() => (exitPickerOpen = !exitPickerOpen)}>
          <span class="exit-current-label">{formExitLabel}</span>
          <span class="exit-current-chev">{exitPickerOpen ? '▲' : '▼'}</span>
        </button>
        {#if exitPickerOpen}
          <div class="exit-picker">
            <input
              type="text"
              class="exit-search"
              bind:value={exitSearch}
              placeholder="Search country or city..."
            />
            <div class="exit-list">
              <button
                class="exit-option default"
                class:selected={formExitServer === ''}
                onclick={() => selectExit('')}
              >
                <span class="exit-option-name">(use active tunnel)</span>
              </button>
              {#each groupedExits as group}
                <div class="exit-group-head">
                  <span class="exit-flag">{getFlag(group.cc)}</span>
                  <span class="exit-group-name">{group.country}</span>
                </div>
                {#each group.entries as s}
                  <button
                    class="exit-option"
                    class:selected={s.hostname === formExitServer}
                    onclick={() => selectExit(s.hostname)}
                  >
                    <span class="exit-option-name">{s.city || s.name}</span>
                    <span class="exit-option-host">{s.hostname}</span>
                    <span class="exit-option-load">{s.load.toFixed(0)}%</span>
                  </button>
                {/each}
              {/each}
              {#if groupedExits.length === 0}
                <div class="exit-empty">No servers loaded.</div>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      {#if formError}
        <div class="form-error">{formError}</div>
      {/if}

      <div class="modal-actions">
        <button class="btn-cancel" onclick={closeModal}>Cancel</button>
        <button class="btn-save" onclick={saveRule}>{editingId ? 'Save' : 'Add Rule'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .routing-page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 0 20px 0;
  }

  .routing-section {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 12px;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .setting-info { flex: 1; min-width: 0; }

  .setting-label {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .setting-desc {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  /* toggle (reused from SettingsScreen.svelte) */
  .toggle {
    position: relative;
    width: 44px;
    height: 24px;
    flex-shrink: 0;
  }
  .toggle input { opacity: 0; width: 0; height: 0; }
  .toggle-track {
    position: absolute;
    inset: 0;
    background: var(--bg-hover);
    border-radius: var(--radius-pill);
    transition: background 0.2s;
    cursor: pointer;
  }
  .toggle input:checked + .toggle-track { background: var(--orange-500); }
  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    background: white;
    border-radius: 50%;
    transition: transform 0.2s;
    pointer-events: none;
  }
  .toggle input:checked ~ .toggle-thumb { transform: translateX(20px); }

  .btn-add-rule {
    background: var(--orange-500);
    color: white;
    font-weight: 600;
    font-size: 13px;
    padding: 8px 14px;
    border: none;
    border-radius: var(--radius-pill);
    cursor: pointer;
    transition: background 0.15s;
  }
  .btn-add-rule:hover { background: var(--orange-400); }

  .apply-status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 12px;
  }
  .apply-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-dim);
    flex-shrink: 0;
  }
  .apply-status.live .apply-dot { background: var(--green-500); }

  .empty-rules {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 20px;
    text-align: center;
    background: rgba(223, 92, 5, 0.04);
    border-radius: var(--radius-md);
  }
  .empty-icon { font-size: 32px; margin-bottom: 8px; }
  .empty-text { font-size: 13px; color: var(--text-muted); }

  .rules-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .rule-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .rule-row.disabled { opacity: 0.55; }

  .rule-body { flex: 1; min-width: 0; }

  .rule-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .rule-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    flex-shrink: 0;
  }
  .chip.vpn {
    background: rgba(223, 92, 5, 0.18);
    color: var(--orange-500);
  }
  .chip.direct {
    background: var(--bg-hover);
    color: var(--text-muted);
  }

  .rule-meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .match-type {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .match-value {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-primary);
    word-break: break-all;
  }
  .proto-chip {
    font-size: 9px;
    font-weight: 700;
    color: var(--text-dim);
    background: var(--bg-hover);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }
  .exit-chip {
    font-size: 10px;
    color: var(--text-dim);
  }

  .rule-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
    transition: background 0.1s, color 0.1s;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .icon-btn:disabled { opacity: 0.3; cursor: default; }
  .icon-btn.danger:hover:not(:disabled) { color: var(--red-500); }

  /* exit picker (shared by active-exit + per-rule) */
  .exit-current {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    flex-shrink: 0;
    max-width: 60%;
  }
  .exit-current.full { width: 100%; max-width: none; justify-content: space-between; }
  .exit-current:hover { border-color: var(--orange-500); }
  .exit-current-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .exit-current-chev { font-size: 9px; color: var(--text-muted); }

  .exit-picker {
    margin-top: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-card);
  }
  .exit-search {
    width: 100%;
    padding: 10px 12px;
    font-size: 13px;
    background: var(--bg-input);
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text-primary);
  }
  .exit-search:focus { outline: none; }
  .exit-list {
    max-height: 240px;
    overflow-y: auto;
  }
  .exit-group-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .exit-flag { font-size: 16px; }
  .exit-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 16px;
    background: transparent;
    border: none;
    border-top: 1px solid var(--border);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }
  .exit-option:hover { background: var(--bg-hover); }
  .exit-option.selected { background: rgba(223, 92, 5, 0.1); }
  .exit-option.default { border-top: none; }
  .exit-option-name { font-size: 13px; flex-shrink: 0; }
  .exit-option-host {
    flex: 1;
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .exit-option-load {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .exit-empty {
    padding: 16px;
    text-align: center;
    font-size: 12px;
    color: var(--text-muted);
  }

  /* modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    z-index: 100;
  }
  .modal {
    width: 100%;
    max-width: 420px;
    max-height: 88vh;
    overflow-y: auto;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }
  .modal-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 16px;
  }
  .form-group { margin-bottom: 14px; }
  .form-label {
    display: block;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    margin-bottom: 6px;
  }
  .form-input {
    width: 100%;
    padding: 10px 12px;
    font-size: 13px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }
  .form-input:focus { border-color: var(--orange-500); }
  .form-input.mono { font-family: var(--font-mono); }
  .form-hint {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 6px;
  }
  .form-error {
    font-size: 12px;
    color: var(--red-500);
    margin-bottom: 12px;
  }

  .segmented {
    display: flex;
    gap: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .seg-btn {
    flex: 1;
    padding: 9px 8px;
    font-size: 13px;
    font-weight: 500;
    background: var(--bg-input);
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }
  .seg-btn + .seg-btn { border-left: 1px solid var(--border); }
  .seg-btn.active {
    background: var(--orange-500);
    color: white;
    font-weight: 600;
  }

  .modal-actions {
    display: flex;
    gap: 10px;
    margin-top: 8px;
  }
  .btn-cancel {
    flex: 1;
    padding: 10px;
    font-size: 14px;
    font-weight: 600;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .btn-cancel:hover { background: var(--bg-hover); }
  .btn-save {
    flex: 1;
    padding: 10px;
    font-size: 14px;
    font-weight: 600;
    background: var(--orange-500);
    color: white;
    border: none;
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .btn-save:hover { background: var(--orange-400); }
</style>
