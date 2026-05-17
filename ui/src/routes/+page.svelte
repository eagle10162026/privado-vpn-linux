<script lang="ts">
  import { onMount } from 'svelte';
  import ConnectScreen from '$lib/components/ConnectScreen.svelte';
  import ServerList from '$lib/components/ServerList.svelte';
  import ControlTower from '$lib/components/ControlTower.svelte';
  import PhantomMode from '$lib/components/PhantomMode.svelte';
  import SettingsScreen from '$lib/components/SettingsScreen.svelte';
  import AccountScreen from '$lib/components/AccountScreen.svelte';
  import SpeedTest from '$lib/components/SpeedTest.svelte';
  import ConnectionHistory from '$lib/components/ConnectionHistory.svelte';
  import NotificationsScreen from '$lib/components/NotificationsScreen.svelte';
  import {
    vpnLogin, vpnLogout, vpnGetServers, vpnConnect, vpnDisconnect,
    vpnStatus, vpnGetConfig, vpnSaveConfig, vpnGetHistory, vpnClearHistory,
    vpnRunSpeedTest, vpnGetSpeedResults, vpnToggleFavorite,
    vpnGetControlTower, vpnSaveControlTower, vpnAddSplitDomain,
    vpnRemoveSplitDomain, vpnCheckConnection, vpnReconnect,
    type ServerEntry, type VpnConfig, type SpeedTestResult, type ControlTowerConfig,
  } from '$lib/tauri';

  type Tab = 'connect' | 'servers' | 'tower' | 'phantom' | 'settings' | 'account';
  type SubScreen = 'speed' | 'history' | 'notifications' | null;
  type VpnState = 'disconnected' | 'connecting' | 'connected' | 'disconnecting' | 'reconnecting' | 'error';

  let tab = $state<Tab>('connect');
  let sub = $state<SubScreen>(null);

  // Core VPN state
  let vpnState = $state<VpnState>('disconnected');
  let connectedServer = $state('');
  let connectedIP = $state('');
  let selectedCountry = $state('NL');
  let connectError = $state('');
  let durationSecs = $state(0);
  let bytesSent = $state(0);
  let bytesRecv = $state(0);
  let killSwitchActive = $state(false);

  // Auth state
  let loggedIn = $state(false);
  let username = $state('');
  let loginBusy = $state(false);
  let loginError = $state('');

  // Server state
  let servers = $state<ServerEntry[]>([]);
  let serversLoading = $state(false);

  // Config state
  let killSwitch = $state(true);
  let splitTunnel = $state(false);
  let autoConnect = $state(false);
  let autoReconnect = $state(true);
  let splitDomains = $state<string[]>([]);
  let dnsServers = $state<string[]>(['198.18.0.1', '198.18.0.2']);
  let favorites = $state<string[]>([]);

  // Speed test state
  let speedTesting = $state(false);
  let speedResult = $state<SpeedTestResult | null>(null);
  let speedHistory = $state<SpeedTestResult[]>([]);

  // History state
  let connectionHistory = $state<{id:string;server:string;country:string;countryCode:string;connectedAt:string;duration:number;dataSent:number;dataReceived:number}[]>([]);
  let historyLoading = $state(false);
  let historySortBy = $state<'date'|'duration'|'data'>('date');

  // Control tower state
  let ctConfig = $state<Record<string, unknown>>({});
  let ctActive = $state(false);

  // Phantom mode state
  let phantomConfig = $state<Record<string, unknown>>({});

  // Notification config state
  let notifConfig = $state<Record<string, unknown>>({});

  // Connection health polling
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  function goTo(t: Tab) { sub = null; tab = t; }

  // === AUTH ===
  async function handleLogin(user: string, pass: string) {
    loginBusy = true; loginError = '';
    const r = await vpnLogin(user, pass);
    if (r.ok) {
      loggedIn = true;
      username = r.username ?? user;
      loginError = '';
      await loadServers();
      goTo('connect');
    } else {
      loginError = r.error ?? 'Login failed';
    }
    loginBusy = false;
  }

  async function handleLogout() {
    await vpnLogout();
    loggedIn = false;
    username = '';
    servers = [];
    vpnState = 'disconnected';
    connectedServer = '';
    connectedIP = '';
    durationSecs = 0;
    bytesSent = 0;
    bytesRecv = 0;
    killSwitchActive = false;
    stopPolling();
  }

  // === SERVERS ===
  async function loadServers() {
    serversLoading = true;
    try {
      servers = await vpnGetServers();
    } catch {
      servers = [];
    }
    serversLoading = false;
  }

  function handleSelectServer(cc: string) {
    selectedCountry = cc;
    goTo('connect');
  }

  // === CONNECT/DISCONNECT ===
  async function handleConnect() {
    vpnState = 'connecting'; connectError = '';
    const r = await vpnConnect(selectedCountry);
    if (r.ok) {
      vpnState = 'connected';
      connectedServer = r.server ?? '';
      connectedIP = r.ip ?? '';
      durationSecs = 0;
      bytesSent = 0;
      bytesRecv = 0;
      killSwitchActive = killSwitch;
      startPolling();
    } else {
      vpnState = 'error';
      connectError = r.error ?? 'Connection failed';
    }
  }

  async function handleDisconnect() {
    vpnState = 'disconnecting';
    stopPolling();
    await vpnDisconnect();
    vpnState = 'disconnected';
    connectedServer = '';
    connectedIP = '';
    durationSecs = 0;
    bytesSent = 0;
    bytesRecv = 0;
    killSwitchActive = false;
    connectError = '';
  }

  async function handleReconnect() {
    vpnState = 'reconnecting'; connectError = '';
    const r = await vpnReconnect();
    if (r.ok) {
      vpnState = 'connected';
      connectedServer = r.server ?? connectedServer;
      connectedIP = r.ip ?? connectedIP;
      durationSecs = 0;
      startPolling();
    } else {
      vpnState = 'error';
      connectError = r.error ?? 'Reconnection failed';
    }
  }

  // === CONNECTION POLLING (every 5s, matches APK dpd_delay=30s but UI updates faster) ===
  function startPolling() {
    stopPolling();
    pollInterval = setInterval(async () => {
      if (vpnState !== 'connected' && vpnState !== 'reconnecting') return;
      try {
        const check = await vpnCheckConnection();
        if (check.alive) {
          durationSecs = check.duration_secs ?? durationSecs;
          bytesSent = check.bytes_sent ?? bytesSent;
          bytesRecv = check.bytes_recv ?? bytesRecv;
        } else if (check.will_reconnect) {
          vpnState = 'reconnecting';
          handleReconnect();
        } else {
          vpnState = 'disconnected';
          connectedServer = '';
          connectedIP = '';
          killSwitchActive = false;
          stopPolling();
        }
      } catch { /* polling failure, ignore */ }
    }, 5000);
  }

  function stopPolling() {
    if (pollInterval) { clearInterval(pollInterval); pollInterval = null; }
  }

  // === SETTINGS ===
  async function saveSettings() {
    const cfg = await vpnGetConfig();
    cfg.kill_switch = killSwitch;
    cfg.split_tunnel = splitTunnel;
    cfg.auto_connect = autoConnect;
    cfg.auto_reconnect = autoReconnect;
    cfg.split_domains = splitDomains;
    cfg.dns_servers = dnsServers;
    cfg.favorites = favorites;
    cfg.preferred_country = selectedCountry;
    await vpnSaveConfig(cfg);
  }

  async function handleAddDomain(domain: string) {
    splitDomains = await vpnAddSplitDomain(domain);
    splitTunnel = splitDomains.length > 0;
  }

  async function handleRemoveDomain(domain: string) {
    splitDomains = await vpnRemoveSplitDomain(domain);
    splitTunnel = splitDomains.length > 0;
  }

  // === SPEED TEST ===
  async function handleSpeedTest() {
    speedTesting = true;
    const r = await vpnRunSpeedTest();
    if (r) { speedResult = r; speedHistory = [...speedHistory, r]; }
    speedTesting = false;
  }

  // === HISTORY ===
  async function loadHistory() {
    historyLoading = true;
    const h = await vpnGetHistory();
    connectionHistory = h.map((c, i) => ({
      id: String(i),
      server: c.server,
      country: c.country,
      countryCode: c.country_code,
      connectedAt: c.connected_at,
      duration: c.duration_secs,
      dataSent: c.bytes_sent,
      dataReceived: c.bytes_recv,
    }));
    historyLoading = false;
  }

  async function handleClearHistory() {
    await vpnClearHistory();
    connectionHistory = [];
  }

  // === CONTROL TOWER ===
  async function handleCtSave(config: unknown) {
    const c = config as Record<string, unknown>;
    ctConfig = c;
    ctActive = Boolean(c.enabled ?? c.adBlocking ?? c.trackerBlocking);
    await vpnSaveControlTower(c as ControlTowerConfig);
  }

  // === PHANTOM MODE ===
  async function handlePhantomSave(config: unknown) {
    const c = config as Record<string, unknown>;
    phantomConfig = c;
    if (Array.isArray(c.domains)) {
      splitDomains = c.domains as string[];
      splitTunnel = (c.mode === 'split' && splitDomains.length > 0);
      await saveSettings();
    }
  }

  // === NOTIFICATIONS ===
  async function handleNotifSave(config: unknown) {
    notifConfig = config as Record<string, unknown>;
  }

  // === INIT ===
  onMount(async () => {
    try {
      const status = await vpnStatus();
      loggedIn = status.logged_in;
      username = status.username;
      if (status.state === 'Connected') {
        vpnState = 'connected';
        connectedServer = status.server ?? '';
        connectedIP = status.ip ?? '';
        durationSecs = status.duration_secs;
        bytesSent = status.bytes_sent;
        bytesRecv = status.bytes_recv;
        killSwitchActive = status.kill_switch_active;
        startPolling();
      }
    } catch { /* not in Tauri */ }

    try {
      const cfg = await vpnGetConfig();
      if (cfg) {
        killSwitch = cfg.kill_switch;
        splitTunnel = cfg.split_tunnel;
        autoConnect = cfg.auto_connect;
        autoReconnect = cfg.auto_reconnect;
        splitDomains = cfg.split_domains ?? [];
        dnsServers = cfg.dns_servers ?? ['198.18.0.1', '198.18.0.2'];
        favorites = cfg.favorites ?? [];
        if (cfg.preferred_country) selectedCountry = cfg.preferred_country;
        if (cfg.username) { loggedIn = true; username = cfg.username; }
      }
    } catch { /* config load failed */ }

    if (loggedIn) {
      await loadServers();
      try {
        const ct = await vpnGetControlTower();
        if (ct) { ctConfig = ct as unknown as Record<string, unknown>; ctActive = ct.enabled; }
      } catch {}
      try { speedHistory = await vpnGetSpeedResults(); } catch {}
    }

    if (!loggedIn) tab = 'account';

    return () => { stopPolling(); };
  });
</script>

<div class="app">
  <div class="content scroll-area">
    {#if sub === 'speed'}
      <div class="sub-header">
        <button class="back-btn" onclick={() => (sub = null)}>&#8592; Back</button>
        <span class="sub-title">Speed Test</span>
      </div>
      <SpeedTest
        isConnected={vpnState === 'connected'}
        isTesting={speedTesting}
        currentResult={speedResult}
        history={speedHistory}
        onStartTest={handleSpeedTest}
      />
    {:else if sub === 'history'}
      <div class="sub-header">
        <button class="back-btn" onclick={() => (sub = null)}>&#8592; Back</button>
        <span class="sub-title">Connection History</span>
      </div>
      <ConnectionHistory
        connections={connectionHistory}
        isLoading={historyLoading}
        sortBy={historySortBy}
        onSortChange={(s) => { historySortBy = s; }}
        onClearHistory={handleClearHistory}
      />
    {:else if sub === 'notifications'}
      <div class="sub-header">
        <button class="back-btn" onclick={() => (sub = null)}>&#8592; Back</button>
        <span class="sub-title">Notifications</span>
      </div>
      <NotificationsScreen config={notifConfig} onSave={handleNotifSave} />
    {:else if tab === 'connect'}
      <ConnectScreen
        {vpnState}
        {connectedServer}
        {connectedIP}
        {selectedCountry}
        {loggedIn}
        {connectError}
        {durationSecs}
        {bytesSent}
        {bytesRecv}
        {killSwitchActive}
        onConnect={handleConnect}
        onDisconnect={handleDisconnect}
        onBrowseServers={() => { goTo('servers'); if (servers.length === 0 && loggedIn) loadServers(); }}
        onCountryChange={(cc) => { selectedCountry = cc; }}
        onReconnect={handleReconnect}
      />
    {:else if tab === 'servers'}
      <ServerList
        {servers}
        {selectedCountry}
        isLoading={serversLoading}
        onSelectServer={handleSelectServer}
        onLoadServers={loadServers}
      />
    {:else if tab === 'tower'}
      <ControlTower
        isActive={ctActive}
        config={ctConfig}
        onSave={handleCtSave}
      />
    {:else if tab === 'phantom'}
      <PhantomMode
        config={{ mode: splitTunnel ? 'split' : 'full', domains: splitDomains }}
        onSave={handlePhantomSave}
      />
    {:else if tab === 'settings'}
      <SettingsScreen
        {killSwitch}
        {splitTunnel}
        {autoConnect}
        {splitDomains}
        {dnsServers}
        onKillSwitchChange={(v) => { killSwitch = v; saveSettings(); }}
        onSplitTunnelChange={(v) => { splitTunnel = v; saveSettings(); }}
        onAutoConnectChange={(v) => { autoConnect = v; saveSettings(); }}
        onAddDomain={handleAddDomain}
        onRemoveDomain={handleRemoveDomain}
        onDnsChange={(s) => { dnsServers = s; saveSettings(); }}
      />
      <div class="settings-links">
        <button class="settings-link" onclick={() => { sub = 'speed'; }}>
          <span>&#9889; Speed Test</span><span class="chev">&#8250;</span>
        </button>
        <button class="settings-link" onclick={() => { sub = 'history'; loadHistory(); }}>
          <span>&#128203; Connection History</span><span class="chev">&#8250;</span>
        </button>
        <button class="settings-link" onclick={() => { sub = 'notifications'; }}>
          <span>&#128276; Notifications</span><span class="chev">&#8250;</span>
        </button>
      </div>
    {:else if tab === 'account'}
      <AccountScreen
        isLoggedIn={loggedIn}
        {username}
        accountType="Premium"
        {loginBusy}
        {loginError}
        onLogin={handleLogin}
        onLogout={handleLogout}
      />
    {/if}
  </div>

  <nav class="tab-bar">
    <button class="tab-item" class:active={tab === 'connect' && !sub} onclick={() => goTo('connect')}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
      </svg>
      <span class="tab-label">Connect</span>
    </button>
    <button class="tab-item" class:active={tab === 'servers'} onclick={() => { goTo('servers'); if (servers.length === 0 && loggedIn) loadServers(); }}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
      </svg>
      <span class="tab-label">Servers</span>
    </button>
    <button class="tab-item" class:active={tab === 'tower'} onclick={() => goTo('tower')}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/>
      </svg>
      <span class="tab-label">Tower</span>
    </button>
    <button class="tab-item" class:active={tab === 'phantom'} onclick={() => goTo('phantom')}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10c1.85 0 3.58-.5 5.07-1.37"/><path d="M17 8l4 4-4 4"/><path d="M21 12H9"/>
      </svg>
      <span class="tab-label">Phantom</span>
    </button>
    <button class="tab-item" class:active={tab === 'settings' && !sub} onclick={() => goTo('settings')}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68 1.65 1.65 0 0 0 9 3V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
      </svg>
      <span class="tab-label">More</span>
    </button>
    <button class="tab-item" class:active={tab === 'account'} onclick={() => goTo('account')}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>
      </svg>
      <span class="tab-label">Account</span>
    </button>
  </nav>
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100vh; max-width: 480px; margin: 0 auto; }
  .content { flex: 1; padding: 16px; overflow-y: auto; }
  .sub-header { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; padding-bottom: 12px; border-bottom: 1px solid var(--border); }
  .back-btn { background: none; color: var(--orange-500); font-size: 16px; font-weight: 600; padding: 4px 8px; border-radius: var(--radius-md); }
  .back-btn:hover { background: var(--bg-hover); }
  .sub-title { font-size: 16px; font-weight: 600; }
  .settings-links { display: flex; flex-direction: column; margin-top: 16px; background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; }
  .settings-link { display: flex; justify-content: space-between; align-items: center; padding: 14px 16px; background: none; color: var(--text-primary); font-size: 14px; border-bottom: 1px solid var(--border); width: 100%; text-align: left; }
  .settings-link:last-child { border-bottom: none; }
  .settings-link:hover { background: var(--bg-hover); }
  .chev { color: var(--text-muted); font-size: 18px; }

  .tab-bar {
    display: flex;
    border-top: 1px solid var(--border);
    background: var(--bg-card);
    padding: 4px 0 8px;
  }
  .tab-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 6px 4px 2px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    transition: color 0.15s;
  }
  .tab-item:hover { color: var(--text-dim); }
  .tab-item.active { color: var(--orange-500); }
  .tab-svg {
    width: 20px;
    height: 20px;
    stroke-width: 1.8;
  }
  .tab-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.3px;
  }
</style>
