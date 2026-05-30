<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import ConnectScreen from '$lib/components/ConnectScreen.svelte';
  import ServerList from '$lib/components/ServerList.svelte';
  import ControlTower from '$lib/components/ControlTower.svelte';
  import PhantomMode from '$lib/components/PhantomMode.svelte';
  import SettingsScreen from '$lib/components/SettingsScreen.svelte';
  import RoutingScreen from '$lib/components/RoutingScreen.svelte';
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
    vpnListRoutingRules, vpnAddRoutingRule, vpnUpdateRoutingRule,
    vpnDeleteRoutingRule, vpnReorderRoutingRules, vpnSetActiveExit,
    type ServerEntry, type VpnConfig, type SpeedTestResult, type ControlTowerConfig,
    type RoutingRule,
  } from '$lib/tauri';

  type Tab = 'connect' | 'servers' | 'tower' | 'phantom' | 'routing' | 'settings' | 'account';
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
  // SpeedTest component row shape (numeric ts; download/upload Mbps; ping ms).
  type SpeedRow = { timestamp: number; download: number; upload: number; ping: number; server: string };
  let speedResult = $state<SpeedRow | null>(null);
  let speedHistory = $state<SpeedRow[]>([]);

  // History state
  let connectionHistory = $state<{id:string;server:string;country:string;country_code:string;duration:number;dataTransferred:number;timestamp:number;flag:string}[]>([]);
  let historyLoading = $state(false);
  let historySortBy = $state<'date'|'duration'|'data'>('date');

  // Control tower state
  let ctConfig = $state<Record<string, unknown>>({});
  let ctActive = $state(false);

  // Phantom mode state
  let phantomConfig = $state<Record<string, unknown>>({});

  // Routing rules state (daemon-backed; the one source of truth across GUI/CLI/LLM).
  let routingRules = $state<RoutingRule[]>([]);
  let activeExit = $state('');

  // Notification config state
  let notifConfig = $state<Record<string, unknown>>({});

  // Connection health polling
  let pollInterval: ReturnType<typeof setInterval> | null = null;
  // Steady CLI<->GUI sync. The daemon (127.10.0.18:1600 / config.json) is the
  // single source of truth that the GUI, the CLI, and the LLM integration all
  // read/write. This interval re-pulls daemon state so a setting/login changed
  // by the LLM or CLI is reflected in the GUI live (and GUI saves already push
  // back through the same daemon, so the CLI/LLM see those too).
  let syncInterval: ReturnType<typeof setInterval> | null = null;

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
  // Country-code → flag emoji (regional-indicator pair). '' for unknown codes.
  function flagEmoji(cc: string): string {
    if (!cc || cc.length !== 2) return '';
    const cp = [...cc.toUpperCase()].map((ch) => 0x1f1e6 - 65 + ch.charCodeAt(0));
    return String.fromCodePoint(...cp);
  }
  // Map daemon SpeedTestResult (…_mbps + string ts) → the SpeedTest row shape.
  function toSpeedRow(r: SpeedTestResult): SpeedRow {
    return {
      timestamp: Date.parse(r.timestamp) || Date.now(),
      download: r.download_mbps,
      upload: r.upload_mbps,
      ping: r.ping_ms,
      server: r.server,
    };
  }

  async function handleSpeedTest() {
    speedTesting = true;
    const r = await vpnRunSpeedTest();
    if (r) { const row = toSpeedRow(r); speedResult = row; speedHistory = [...speedHistory, row]; }
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
      country_code: c.country_code,
      duration: c.duration_secs,
      dataTransferred: c.bytes_sent + c.bytes_recv,
      timestamp: Date.parse(c.connected_at) || 0,
      flag: flagEmoji(c.country_code),
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
    await vpnSaveControlTower(c as unknown as ControlTowerConfig);
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

  // === ROUTING RULES ===
  async function loadRoutingRules() {
    try {
      routingRules = await vpnListRoutingRules();
    } catch {
      routingRules = [];
    }
  }

  async function handleAddRule(rule: RoutingRule) {
    const r = await vpnAddRoutingRule(rule);
    if (r.ok) routingRules = r.rules;
  }

  async function handleUpdateRule(rule: RoutingRule) {
    const r = await vpnUpdateRoutingRule(rule);
    if (r.ok) routingRules = r.rules;
  }

  async function handleDeleteRule(id: string) {
    const r = await vpnDeleteRoutingRule(id);
    if (r.ok) routingRules = r.rules;
  }

  async function handleReorderRule(order: string[]) {
    const r = await vpnReorderRoutingRules(order);
    if (r.ok) routingRules = r.rules;
  }

  async function handleSetActiveExit(server: string) {
    const r = await vpnSetActiveExit(server);
    if (r.ok) {
      activeExit = server;
      vpnState = 'connected';
      connectedServer = r.server ?? connectedServer;
      connectedIP = r.ip ?? connectedIP;
      if (!pollInterval) startPolling();
    }
  }

  // === NOTIFICATIONS ===
  async function handleNotifSave(config: unknown) {
    notifConfig = config as Record<string, unknown>;
  }

  // Pull authoritative state from the daemon (login + connection + config) so
  // changes made by the LLM/CLI — which write through the SAME daemon — show up
  // in the GUI without a reload. Config toggles are NOT overwritten while the
  // user is on the Settings tab, so an in-progress edit is never clobbered.
  async function syncFromDaemon() {
    try {
      const status = await vpnStatus();
      // Login/account state — reflects a login/logout done via CLI/LLM (#1).
      loggedIn = status.logged_in;
      username = status.username;
      // Reflect connection state, but don't fight an in-flight transition.
      if (status.state === 'Connected') {
        if (vpnState === 'disconnected' || vpnState === 'connected') {
          vpnState = 'connected';
          connectedServer = status.server ?? connectedServer;
          connectedIP = status.ip ?? connectedIP;
          killSwitchActive = status.kill_switch_active;
          if (status.server) activeExit = status.server;
          if (!pollInterval) startPolling();
        }
      } else if (status.state === 'Disconnected' && vpnState === 'connected') {
        vpnState = 'disconnected';
        connectedServer = '';
        connectedIP = '';
        killSwitchActive = false;
        stopPolling();
      }
    } catch { /* not in Tauri / daemon unreachable */ }

    // Reflect rule changes made by the CLI/LLM, but don't clobber an open editor.
    if (tab !== 'routing') {
      await loadRoutingRules();
    }

    // Don't overwrite settings the user is actively editing.
    if (tab === 'settings') return;
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
        activeExit = status.server ?? '';
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
      try { speedHistory = (await vpnGetSpeedResults()).map(toSpeedRow); } catch {}
    }

    // Load routing rules (daemon-backed) so the Routing tab is ready.
    await loadRoutingRules();

    if (!loggedIn) tab = 'account';

    // Steady CLI<->GUI sync (4s): keeps the GUI reflecting LLM/CLI-driven login
    // + setting changes. GUI saves push back through the same daemon, so the
    // CLI/LLM see GUI changes too — fully bidirectional.
    syncInterval = setInterval(syncFromDaemon, 4000);
  });

  // Teardown via onDestroy — an async onMount() cannot return a cleanup fn
  // (Svelte's types reject Promise<cleanup>), so register it here instead.
  onDestroy(() => {
    stopPolling();
    if (syncInterval) { clearInterval(syncInterval); syncInterval = null; }
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
    {:else if tab === 'routing'}
      <RoutingScreen
        rules={routingRules}
        {servers}
        {activeExit}
        connectionState={vpnState === 'connected' ? 'Connected' : 'Disconnected'}
        onAddRule={handleAddRule}
        onUpdateRule={handleUpdateRule}
        onDeleteRule={handleDeleteRule}
        onReorderRule={handleReorderRule}
        onSetActiveExit={handleSetActiveExit}
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
    <button class="tab-item" class:active={tab === 'routing'} onclick={() => goTo('routing')}>
      <svg class="tab-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="6" cy="19" r="3"/><circle cx="6" cy="5" r="3"/><circle cx="18" cy="12" r="3"/><path d="M6 8v3a3 3 0 0 0 3 3h6M6 16v-1"/>
      </svg>
      <span class="tab-label">Routing</span>
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
