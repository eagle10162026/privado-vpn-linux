<script lang="ts">
  /**
   * ConnectScreen — main VPN connection screen. Mirrors the PrivadoVPN
   * Android HomeFragment: oval ring, IP display, server selector, traffic
   * stats, kill-switch / split-tunnel indicators, and a state-driven
   * connect/disconnect button.
   */

  interface Props {
    vpnState: 'disconnected' | 'connecting' | 'connected' | 'disconnecting' | 'reconnecting' | 'error';
    connectedServer: string;
    connectedIP: string;
    selectedCountry: string;
    loggedIn: boolean;
    connectError: string;
    durationSecs: number;
    bytesSent: number;
    bytesRecv: number;
    killSwitchActive: boolean;
    onConnect: () => void;
    onDisconnect: () => void;
    onBrowseServers: () => void;
    onCountryChange: (country: string) => void;
    onReconnect: () => void;
  }

  let {
    vpnState = 'disconnected',
    connectedServer = '',
    connectedIP = '',
    selectedCountry = 'NL',
    loggedIn = false,
    connectError = '',
    durationSecs = 0,
    bytesSent = 0,
    bytesRecv = 0,
    killSwitchActive = false,
    onConnect,
    onDisconnect,
    onBrowseServers,
    onCountryChange,
    onReconnect,
  }: Props = $props();

  const FLAGS: Record<string, string> = {
    US: '🇺🇸', GB: '🇬🇧', DE: '🇩🇪', NL: '🇳🇱', CH: '🇨🇭', FR: '🇫🇷',
    JP: '🇯🇵', SG: '🇸🇬', CA: '🇨🇦', AU: '🇦🇺', SE: '🇸🇪', NO: '🇳🇴',
    DK: '🇩🇰', FI: '🇫🇮', AT: '🇦🇹', BE: '🇧🇪', IT: '🇮🇹', ES: '🇪🇸',
    PT: '🇵🇹', IE: '🇮🇪', PL: '🇵🇱', CZ: '🇨🇿', RO: '🇷🇴', HU: '🇭🇺',
    BR: '🇧🇷', MX: '🇲🇽', AR: '🇦🇷', IN: '🇮🇳', KR: '🇰🇷', HK: '🇭🇰',
    TW: '🇹🇼', IL: '🇮🇱', TR: '🇹🇷', ZA: '🇿🇦', UA: '🇺🇦', RU: '🇷🇺',
    BG: '🇧🇬', HR: '🇭🇷', LV: '🇱🇻', IS: '🇮🇸', NZ: '🇳🇿', PH: '🇵🇭',
    MY: '🇲🇾', TH: '🇹🇭', CO: '🇨🇴', CL: '🇨🇱', PE: '🇵🇪', EG: '🇪🇬',
  };

  const NAMES: Record<string, string> = {
    US: 'United States', GB: 'United Kingdom', DE: 'Germany', NL: 'Netherlands',
    CH: 'Switzerland', FR: 'France', JP: 'Japan', SG: 'Singapore', CA: 'Canada',
    AU: 'Australia', SE: 'Sweden', NO: 'Norway', DK: 'Denmark', FI: 'Finland',
    AT: 'Austria', BE: 'Belgium', IT: 'Italy', ES: 'Spain', PT: 'Portugal',
    IE: 'Ireland', PL: 'Poland', CZ: 'Czech Republic', RO: 'Romania', HU: 'Hungary',
    BR: 'Brazil', MX: 'Mexico', AR: 'Argentina', IN: 'India', KR: 'South Korea',
    HK: 'Hong Kong', TW: 'Taiwan', IL: 'Israel', TR: 'Turkey', ZA: 'South Africa',
    UA: 'Ukraine', RU: 'Russia', BG: 'Bulgaria', HR: 'Croatia', LV: 'Latvia',
    IS: 'Iceland', NZ: 'New Zealand', PH: 'Philippines', MY: 'Malaysia', TH: 'Thailand',
    CO: 'Colombia', CL: 'Chile', PE: 'Peru', EG: 'Egypt',
  };

  function flag(cc: string): string { return FLAGS[cc?.toUpperCase()] ?? '🌍'; }
  function countryName(cc: string): string { return NAMES[cc?.toUpperCase()] ?? cc?.toUpperCase() ?? '—'; }

  let isConnected = $derived(vpnState === 'connected');
  let isConnecting = $derived(vpnState === 'connecting' || vpnState === 'reconnecting');
  let isDisconnecting = $derived(vpnState === 'disconnecting');
  let isBusy = $derived(isConnecting || isDisconnecting);

  function fmtDuration(s: number): string {
    if (s < 60) return `00:${String(s).padStart(2, '0')}`;
    if (s < 3600) return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
  }

  function fmtBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function connectStep(): string {
    if (vpnState === 'reconnecting') return 'Reconnecting...';
    if (vpnState === 'connecting') return 'Establishing secure tunnel...';
    if (vpnState === 'disconnecting') return 'Disconnecting...';
    return '';
  }
</script>

<div class="home">
  <!-- IP / Address header -->
  <div class="ip-bar">
    {#if isConnected && connectedIP}
      <div class="ip-label">Your IP: <span class="ip-value">{connectedIP}</span></div>
    {:else if !isConnected}
      <div class="ip-label ip-exposed">Your IP is <span class="ip-exposed-text">exposed</span></div>
    {:else}
      <div class="ip-label">&nbsp;</div>
    {/if}
  </div>

  <!-- Oval connection ring (matches APK DialogOvalView) -->
  <div class="oval-container">
    <div class="oval-ring"
      class:connected={isConnected}
      class:connecting={isConnecting}
      class:disconnecting={isDisconnecting}
    >
      <div class="oval-glow"
        class:connected={isConnected}
        class:connecting={isConnecting}
      ></div>
      <div class="oval-inner"
        class:connected={isConnected}
        class:connecting={isConnecting}
      >
        <div class="oval-icon">
          {#if isConnected}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shield-icon connected">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M9 12l2 2 4-4"/>
            </svg>
          {:else if isConnecting}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shield-icon connecting">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shield-icon disconnected">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
            </svg>
          {/if}
        </div>
        <div class="oval-status"
          class:connected={isConnected}
          class:connecting={isConnecting}
          class:disconnected={!isConnected && !isConnecting && !isDisconnecting}
        >
          {#if isConnected}Protected{:else if isConnecting}Connecting{:else if isDisconnecting}Disconnecting{:else}Not Protected{/if}
        </div>
      </div>
    </div>
  </div>

  <!-- Connection steps / status message -->
  {#if isBusy}
    <div class="connect-steps animate-in">{connectStep()}</div>
  {/if}

  <!-- Error banner -->
  {#if connectError && vpnState === 'error'}
    <div class="error-banner animate-in">
      <span class="error-icon">⚠</span>
      <span class="error-text">{connectError}</span>
      <button class="error-retry" onclick={onReconnect}>Retry</button>
    </div>
  {/if}

  <!-- Connected info card -->
  {#if isConnected}
    <div class="connected-card animate-in">
      <div class="connected-header">
        <span class="connected-dot"></span>
        <span class="connected-label">Connected to</span>
      </div>
      <div class="connected-server">{connectedServer || countryName(selectedCountry)}</div>

      <!-- Traffic stats row (matches APK traffic display) -->
      <div class="stats-row">
        <div class="stat">
          <div class="stat-icon">↓</div>
          <div class="stat-info">
            <div class="stat-value">{fmtBytes(bytesRecv)}</div>
            <div class="stat-label">Download</div>
          </div>
        </div>
        <div class="stat-divider"></div>
        <div class="stat">
          <div class="stat-icon">↑</div>
          <div class="stat-info">
            <div class="stat-value">{fmtBytes(bytesSent)}</div>
            <div class="stat-label">Upload</div>
          </div>
        </div>
        <div class="stat-divider"></div>
        <div class="stat">
          <div class="stat-icon">⏱</div>
          <div class="stat-info">
            <div class="stat-value">{fmtDuration(durationSecs)}</div>
            <div class="stat-label">Duration</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Feature indicators (kill switch, split tunnel) -->
    <div class="features-bar animate-in">
      {#if killSwitchActive}
        <div class="feature-chip active">
          <span class="feature-dot ks"></span>
          <span>Kill Switch</span>
        </div>
      {/if}
      <div class="feature-chip">
        <span class="feature-dot protocol"></span>
        <span>IKEv2</span>
      </div>
    </div>
  {/if}

  <!-- Server selector card -->
  <button class="server-select-card" onclick={onBrowseServers}>
    <span class="server-flag">{flag(selectedCountry)}</span>
    <div class="server-info">
      <div class="server-country">{countryName(selectedCountry)}</div>
      <div class="server-sub">
        {#if isConnected}
          {connectedServer || 'Best available'}
        {:else}
          Tap to change location
        {/if}
      </div>
    </div>
    <svg class="server-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <polyline points="9 18 15 12 9 6"/>
    </svg>
  </button>

  <!-- Main action button -->
  <div class="action-container">
    {#if !loggedIn}
      <button class="btn-action login" disabled>Log in to connect</button>
    {:else if vpnState === 'disconnected' || vpnState === 'error'}
      <button class="btn-action connect" onclick={onConnect}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="btn-icon">
          <polygon points="5 3 19 12 5 21 5 3"/>
        </svg>
        Connect
      </button>
    {:else if isConnected}
      <button class="btn-action disconnect" onclick={onDisconnect}>
        <svg viewBox="0 0 24 24" fill="currentColor" class="btn-icon">
          <rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/>
        </svg>
        Disconnect
      </button>
    {:else}
      <button class="btn-action busy" disabled>
        <div class="spinner"></div>
        {connectStep()}
      </button>
    {/if}
  </div>
</div>

<style>
  .home {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 0 4px;
  }

  /* ── IP bar ── */
  .ip-bar { text-align: center; width: 100%; }
  .ip-label { font-size: 13px; color: var(--text-muted); }
  .ip-value { color: var(--green-500); font-family: var(--font-mono); font-weight: 600; }
  .ip-exposed { color: var(--text-muted); }
  .ip-exposed-text { color: var(--red-500); font-weight: 600; }

  /* ── Oval ring (APK DialogOvalView) ── */
  .oval-container {
    width: 200px;
    height: 200px;
    margin: 8px auto 0;
  }

  .oval-ring {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    border: 3px solid var(--border-card);
    transition: border-color 0.6s ease, box-shadow 0.6s ease;
  }

  .oval-ring.connected {
    border-color: var(--green-500);
    box-shadow: 0 0 0 6px rgba(34, 197, 94, 0.12), 0 0 30px rgba(34, 197, 94, 0.15);
    animation: ring-pulse 2.5s ease-in-out infinite;
  }

  .oval-ring.connecting {
    border-color: var(--orange-500);
    border-style: dashed;
    box-shadow: 0 0 0 6px rgba(223, 92, 5, 0.1);
    animation: ring-spin 2s linear infinite;
  }

  .oval-ring.disconnecting {
    border-color: var(--orange-500);
    opacity: 0.6;
  }

  .oval-glow {
    position: absolute;
    inset: -12px;
    border-radius: 50%;
    pointer-events: none;
    transition: opacity 0.6s;
    opacity: 0;
  }

  .oval-glow.connected {
    opacity: 1;
    background: radial-gradient(circle, rgba(34, 197, 94, 0.06) 0%, transparent 70%);
  }

  .oval-glow.connecting {
    opacity: 1;
    background: radial-gradient(circle, rgba(223, 92, 5, 0.05) 0%, transparent 70%);
  }

  .oval-inner {
    position: absolute;
    inset: 10px;
    border-radius: 50%;
    border: 2px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    transition: border-color 0.5s, background 0.5s;
    background: var(--bg-card);
  }

  .oval-inner.connected {
    border-color: rgba(34, 197, 94, 0.3);
    background: rgba(34, 197, 94, 0.06);
  }

  .oval-inner.connecting {
    border-color: rgba(223, 92, 5, 0.3);
    background: rgba(223, 92, 5, 0.04);
  }

  .oval-icon { width: 44px; height: 44px; }

  .shield-icon {
    width: 100%;
    height: 100%;
    stroke-width: 1.8;
  }

  .shield-icon.connected { color: var(--green-500); }
  .shield-icon.connecting { color: var(--orange-500); animation: shield-pulse 1s ease-in-out infinite; }
  .shield-icon.disconnected { color: var(--text-muted); }

  .oval-status {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
  }

  .oval-status.connected { color: var(--green-500); }
  .oval-status.connecting { color: var(--orange-500); }
  .oval-status.disconnected { color: var(--text-muted); }

  /* ── Connection steps ── */
  .connect-steps {
    font-size: 13px;
    color: var(--orange-400);
    text-align: center;
    letter-spacing: 0.3px;
  }

  /* ── Error banner ── */
  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 14px;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-md);
  }

  .error-icon { font-size: 16px; }
  .error-text { flex: 1; font-size: 13px; color: var(--red-500); }
  .error-retry {
    background: transparent;
    color: var(--orange-500);
    font-size: 13px;
    font-weight: 600;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
  }
  .error-retry:hover { background: rgba(223, 92, 5, 0.1); }

  /* ── Connected card ── */
  .connected-card {
    width: 100%;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 16px;
  }

  .connected-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .connected-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--green-500);
    box-shadow: 0 0 6px var(--green-500);
    animation: dot-pulse 2s ease-in-out infinite;
  }

  .connected-label {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.8px;
  }

  .connected-server {
    font-size: 17px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 14px;
  }

  .stats-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
  }

  .stat-icon {
    font-size: 16px;
    color: var(--text-dim);
    width: 20px;
    text-align: center;
  }

  .stat-value {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-mono);
  }

  .stat-label {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }

  .stat-divider {
    width: 1px;
    height: 28px;
    background: var(--border);
    margin: 0 4px;
    flex-shrink: 0;
  }

  /* ── Feature indicators ── */
  .features-bar {
    display: flex;
    gap: 8px;
    width: 100%;
  }

  .feature-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: 12px;
    color: var(--text-dim);
    font-weight: 500;
  }

  .feature-chip.active { border-color: rgba(34, 197, 94, 0.3); color: var(--green-500); }

  .feature-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .feature-dot.ks { background: var(--green-500); box-shadow: 0 0 4px var(--green-500); }
  .feature-dot.protocol { background: var(--teal-500); }

  /* ── Server selector card ── */
  .server-select-card {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 14px 16px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .server-select-card:hover {
    background: var(--bg-hover);
    border-color: var(--border-active);
  }

  .server-flag { font-size: 32px; flex-shrink: 0; }

  .server-info { flex: 1; text-align: left; }

  .server-country {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .server-sub {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .server-chevron {
    width: 20px;
    height: 20px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  /* ── Action button ── */
  .action-container { width: 100%; margin-top: 4px; }

  .btn-action {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    font-weight: 600;
    font-size: 16px;
    border-radius: var(--radius-pill);
    padding: 15px 32px;
    border: none;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s, box-shadow 0.2s;
    letter-spacing: 0.3px;
  }

  .btn-action.connect {
    background: var(--orange-500);
    color: white;
    box-shadow: 0 4px 16px rgba(223, 92, 5, 0.3);
  }
  .btn-action.connect:hover { background: var(--orange-400); box-shadow: 0 6px 20px rgba(223, 92, 5, 0.4); }
  .btn-action.connect:active { transform: scale(0.98); }

  .btn-action.disconnect {
    background: var(--red-500);
    color: white;
    box-shadow: 0 4px 16px rgba(239, 68, 68, 0.3);
  }
  .btn-action.disconnect:hover { background: #dc2626; }
  .btn-action.disconnect:active { transform: scale(0.98); }

  .btn-action.busy {
    background: var(--bg-card-secondary);
    color: var(--text-muted);
    cursor: not-allowed;
    gap: 12px;
  }

  .btn-action.login {
    background: var(--bg-card-secondary);
    color: var(--text-muted);
    cursor: not-allowed;
  }

  .btn-icon { width: 18px; height: 18px; flex-shrink: 0; }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--text-disabled);
    border-top-color: var(--orange-500);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  /* ── Animations ── */
  @keyframes ring-pulse {
    0%, 100% { box-shadow: 0 0 0 6px rgba(34, 197, 94, 0.12), 0 0 30px rgba(34, 197, 94, 0.15); }
    50% { box-shadow: 0 0 0 10px rgba(34, 197, 94, 0.06), 0 0 40px rgba(34, 197, 94, 0.1); }
  }

  @keyframes ring-spin { to { transform: rotate(360deg); } }

  @keyframes shield-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  @keyframes dot-pulse {
    0%, 100% { opacity: 1; box-shadow: 0 0 6px var(--green-500); }
    50% { opacity: 0.6; box-shadow: 0 0 10px var(--green-500); }
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  .animate-in { animation: fade-in 0.25s ease; }
  @keyframes fade-in {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
