<script lang="ts">
  interface Props {
    killSwitch: boolean;
    splitTunnel: boolean;
    autoConnect: boolean;
    splitDomains: string[];
    dnsServers: string[];
    onKillSwitchChange: (value: boolean) => void;
    onSplitTunnelChange: (value: boolean) => void;
    onAutoConnectChange: (value: boolean) => void;
    onAddDomain: (domain: string) => void;
    onRemoveDomain: (domain: string) => void;
    onDnsChange: (servers: string[]) => void;
  }

  let {
    killSwitch = true,
    splitTunnel = false,
    autoConnect = false,
    splitDomains = [],
    dnsServers = ['198.18.0.1', '198.18.0.2'],
    onKillSwitchChange,
    onSplitTunnelChange,
    onAutoConnectChange,
    onAddDomain,
    onRemoveDomain,
    onDnsChange
  }: Props = $props();

  let newDomain = $state('');
  let dnsInput = $state(dnsServers.join(', '));

  function handleAddDomain() {
    const domain = newDomain.trim().toLowerCase();
    if (domain && !splitDomains.includes(domain)) {
      onAddDomain(domain);
      newDomain = '';
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      handleAddDomain();
    }
  }

  function handleDnsChange() {
    const servers = dnsInput
      .split(',')
      .map(s => s.trim())
      .filter(s => s.length > 0);
    onDnsChange(servers);
  }

  function handleKillSwitchChange(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    onKillSwitchChange(checked);
  }

  function handleSplitTunnelChange(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    onSplitTunnelChange(checked);
  }

  function handleAutoConnectChange(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    onAutoConnectChange(checked);
  }
</script>

<div class="settings-page">
  <div class="settings-section card">
    <div class="section-title">Protocol</div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">VPN Protocol</div>
        <div class="setting-desc">Internet Key Exchange v2 with IPsec encapsulation</div>
      </div>
      <div class="setting-value">IKEv2</div>
    </div>
  </div>

  <div class="settings-section card">
    <div class="section-title">Connection</div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Kill Switch</div>
        <div class="setting-desc">Block all traffic if VPN disconnects unexpectedly</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={killSwitch}
          onchange={handleKillSwitchChange}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Split Tunnel</div>
        <div class="setting-desc">Route only specified domains through VPN</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={splitTunnel}
          onchange={handleSplitTunnelChange}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Auto-Connect</div>
        <div class="setting-desc">Connect automatically on startup</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={autoConnect}
          onchange={handleAutoConnectChange}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>
  </div>

  {#if splitTunnel}
    <div class="settings-section card">
      <div class="section-title">Split-Tunnel Domains</div>

      {#if splitDomains.length === 0}
        <div class="empty-domains">
          <div class="empty-icon">📋</div>
          <div class="empty-text">No domains configured. Only DNS queries will be tunneled.</div>
        </div>
      {:else}
        <div class="domains-list">
          {#each splitDomains as domain}
            <div class="domain-item">
              <span class="domain-text">{domain}</span>
              <button
                class="btn-remove"
                onclick={() => onRemoveDomain(domain)}
                title="Remove domain"
              >
                ✕
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="domain-input-group">
        <input
          type="text"
          bind:value={newDomain}
          onkeydown={handleKeyDown}
          placeholder="example.com"
          class="domain-input"
        />
        <button class="btn-add-domain" onclick={handleAddDomain}>
          Add
        </button>
      </div>
    </div>
  {/if}

  <div class="settings-section card">
    <div class="section-title">DNS</div>
    <div class="setting-desc" style="margin-bottom: 12px;">
      Custom DNS servers for enhanced privacy (comma-separated)
    </div>
    <input
      type="text"
      bind:value={dnsInput}
      onchange={handleDnsChange}
      placeholder="198.18.0.1, 198.18.0.2"
      class="dns-input"
    />
  </div>

  <div class="settings-section card">
    <div class="section-title">Appearance</div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Theme</div>
        <div class="setting-desc">Dark theme optimized for low light</div>
      </div>
      <div class="setting-value">Dark</div>
    </div>
  </div>

  <div class="settings-section card">
    <div class="section-title">About</div>

    <div class="setting-row">
      <div class="setting-label">Version</div>
      <div class="setting-value">1.0.0</div>
    </div>

    <div class="setting-row">
      <div class="setting-label">License</div>
      <div class="setting-value">MIT Open Source</div>
    </div>

    <div class="setting-row">
      <div class="setting-label">strongSwan Engine</div>
      <div class="setting-value">IKEv2/IPsec</div>
    </div>
  </div>
</div>

<style>
  .settings-page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 0 20px 0;
  }

  .settings-section {
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

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-3) 0;
    border-bottom: 1px solid var(--border);
  }

  .setting-row:last-child {
    border-bottom: none;
  }

  .setting-info {
    flex: 1;
  }

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

  .setting-value {
    font-size: 14px;
    color: var(--text-dim);
    font-weight: 500;
  }

  .toggle {
    position: relative;
    width: 44px;
    height: 24px;
    flex-shrink: 0;
  }

  .toggle input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .toggle-track {
    position: absolute;
    inset: 0;
    background: var(--bg-hover);
    border-radius: var(--radius-pill);
    transition: background 0.2s;
    cursor: pointer;
  }

  .toggle input:checked + .toggle-track {
    background: var(--orange-500);
  }

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

  .toggle input:checked ~ .toggle-thumb {
    transform: translateX(20px);
  }

  .empty-domains {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 20px;
    text-align: center;
    background: rgba(223, 92, 5, 0.04);
    border-radius: var(--radius-md);
    margin-bottom: 12px;
  }

  .empty-icon {
    font-size: 32px;
    margin-bottom: 8px;
  }

  .empty-text {
    font-size: 13px;
    color: var(--text-muted);
  }

  .domains-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 12px;
  }

  .domain-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .domain-text {
    font-size: 13px;
    font-family: var(--font-mono);
    color: var(--text-primary);
    word-break: break-all;
  }

  .btn-remove {
    background: transparent;
    border: none;
    color: var(--red-500);
    font-size: 16px;
    cursor: pointer;
    padding: 4px 8px;
    margin-left: 8px;
    flex-shrink: 0;
    transition: opacity 0.15s;
  }

  .btn-remove:hover {
    opacity: 0.7;
  }

  .domain-input-group {
    display: flex;
    gap: 8px;
  }

  .domain-input {
    flex: 1;
    padding: 10px 14px;
    font-size: 13px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }

  .domain-input:focus {
    border-color: var(--orange-500);
  }

  .btn-add-domain {
    background: transparent;
    color: var(--orange-500);
    font-weight: 600;
    padding: 10px 16px;
    border: none;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-add-domain:hover {
    background: rgba(223, 92, 5, 0.1);
  }

  .dns-input {
    width: 100%;
    padding: 10px 14px;
    font-size: 13px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-mono);
  }

  .dns-input:focus {
    border-color: var(--orange-500);
  }
</style>
