// Tauri v2 IPC bridge
// withGlobalTauri must be true in tauri.conf.json for window.__TAURI__ to exist
// __TAURI_INTERNALS__ is always injected by Tauri v2 runtime
const invoke = (() => {
  if (typeof window === 'undefined') {
    return async (_cmd: string, _args?: unknown) => null;
  }
  const ti = (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ as
    { invoke?: (cmd: string, args?: unknown) => Promise<unknown> } | undefined;
  if (ti?.invoke) return ti.invoke;
  const tg = (window as unknown as Record<string, unknown>).__TAURI__ as
    { core?: { invoke?: (cmd: string, args?: unknown) => Promise<unknown> } } | undefined;
  if (tg?.core?.invoke) return tg.core.invoke;
  return async (cmd: string, _args?: unknown) => {
    console.warn(`[tauri-stub] ${cmd} — no IPC available`);
    return null;
  };
})();

export async function vpnLogin(username: string, password: string): Promise<{ ok: boolean; username?: string; error?: string; account_type?: number }> {
  try {
    const r = await invoke('vpn_login', { username, password }) as Record<string, unknown>;
    return { ok: true, username: r?.username as string, account_type: r?.account_type as number };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function vpnLogout(): Promise<void> {
  await invoke('vpn_logout');
}

export async function vpnGetServers(): Promise<ServerEntry[]> {
  const r = await invoke('vpn_get_servers');
  return (r as ServerEntry[]) ?? [];
}

export async function vpnConnect(country: string, city?: string): Promise<{ ok: boolean; server?: string; ip?: string; country?: string; country_code?: string; error?: string }> {
  try {
    const r = await invoke('vpn_connect', { country, city }) as Record<string, unknown>;
    return { ok: true, server: r?.server as string, ip: r?.ip as string, country: r?.country as string, country_code: r?.country_code as string };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function vpnDisconnect(): Promise<void> {
  await invoke('vpn_disconnect');
}

export async function vpnStatus(): Promise<VpnStatus> {
  const r = await invoke('vpn_status') as VpnStatus;
  return r ?? { state: 'Disconnected', server: null, ip: null, country_code: null, username: '', logged_in: false, duration_secs: 0, bytes_sent: 0, bytes_recv: 0, kill_switch_active: false };
}

export async function vpnGetConfig(): Promise<VpnConfig> {
  const r = await invoke('vpn_get_config');
  return (r as VpnConfig) ?? { username: '', password: '', preferred_country: null, preferred_city: null, split_tunnel: false, split_domains: [], kill_switch: true, auto_connect: false, dns_servers: ['198.18.0.1', '198.18.0.2'], favorites: [], trusted_networks: [], protocol: 'ikev2', auto_reconnect: true, route_llm_browser: false, route_llm_tools: false, routing_rules: [] };
}

export async function vpnSaveConfig(config: VpnConfig): Promise<void> {
  await invoke('vpn_save_config', { config });
}

export async function vpnGetHistory(): Promise<ConnectionRecord[]> {
  const r = await invoke('vpn_get_history');
  return (r as ConnectionRecord[]) ?? [];
}

export async function vpnClearHistory(): Promise<void> {
  await invoke('vpn_clear_history');
}

export async function vpnRunSpeedTest(): Promise<SpeedTestResult | null> {
  try {
    return await invoke('vpn_run_speed_test') as SpeedTestResult;
  } catch { return null; }
}

export async function vpnGetSpeedResults(): Promise<SpeedTestResult[]> {
  const r = await invoke('vpn_get_speed_results');
  return (r as SpeedTestResult[]) ?? [];
}

export async function vpnToggleFavorite(serverName: string): Promise<string[]> {
  const r = await invoke('vpn_toggle_favorite', { serverName });
  return (r as string[]) ?? [];
}

export async function vpnGetControlTower(): Promise<ControlTowerConfig> {
  const r = await invoke('vpn_get_control_tower');
  return (r as ControlTowerConfig) ?? { enabled: false, ad_blocking: true, tracker_blocking: true, malware_protection: true, phishing_protection: true, adult_content: false, custom_blocklist: [], dns_provider: 'privado', custom_dns: null, ads_blocked: 0, trackers_blocked: 0, threats_blocked: 0 };
}

export async function vpnSaveControlTower(config: ControlTowerConfig): Promise<void> {
  await invoke('vpn_save_control_tower', { config });
}

export async function vpnGetNotifications(): Promise<NotificationConfig> {
  const r = await invoke('vpn_get_notifications');
  return (r as NotificationConfig) ?? { enabled: true, on_connect: true, on_disconnect: true, on_killswitch: true, on_connection_failed: true, on_subscription_expiring: true };
}

export async function vpnSaveNotifications(config: NotificationConfig): Promise<void> {
  await invoke('vpn_save_notifications', { config });
}

export async function vpnAddSplitDomain(domain: string): Promise<string[]> {
  const r = await invoke('vpn_add_split_domain', { domain });
  return (r as string[]) ?? [];
}

export async function vpnRemoveSplitDomain(domain: string): Promise<string[]> {
  const r = await invoke('vpn_remove_split_domain', { domain });
  return (r as string[]) ?? [];
}

export async function vpnImportDomains(domains: string[]): Promise<string[]> {
  const r = await invoke('vpn_import_domains', { domains });
  return (r as string[]) ?? [];
}

export async function vpnCheckConnection(): Promise<ConnectionCheck> {
  const r = await invoke('vpn_check_connection');
  return (r as ConnectionCheck) ?? { alive: false, state: 'Disconnected', duration_secs: 0, bytes_sent: 0, bytes_recv: 0 };
}

export async function vpnReconnect(): Promise<{ ok: boolean; server?: string; ip?: string; error?: string }> {
  try {
    const r = await invoke('vpn_reconnect') as Record<string, unknown>;
    return { ok: true, server: r?.server as string, ip: r?.ip as string };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function vpnGetIpInfo(): Promise<{ ip?: string }> {
  try {
    const r = await invoke('vpn_get_ip_info') as Record<string, unknown>;
    return { ip: r?.ip as string };
  } catch { return {}; }
}

export interface ServerEntry {
  name: string; hostname: string; city: string; country: string;
  country_code: string; ip: string; status: string; load: number;
}

export interface VpnStatus {
  state: string; server: string | null; ip: string | null;
  country_code: string | null; username: string; logged_in: boolean;
  duration_secs: number; bytes_sent: number; bytes_recv: number;
  kill_switch_active: boolean;
}

export interface RoutingRule {
  id: string;
  enabled: boolean;
  name: string;
  match_type: 'process' | 'domain' | 'ip_cidr' | 'port' | 'port_range';
  match_value: string;
  protocol?: 'tcp' | 'udp' | null;
  action: 'vpn' | 'direct';
  exit_server?: string | null;
  priority: number;
}

export interface VpnConfig {
  username: string; password: string; preferred_country: string | null;
  preferred_city: string | null; split_tunnel: boolean; split_domains: string[];
  kill_switch: boolean; auto_connect: boolean; dns_servers: string[];
  favorites: string[]; trusted_networks: string[]; protocol: string;
  auto_reconnect: boolean;
  route_llm_browser: boolean; route_llm_tools: boolean;
  routing_rules: RoutingRule[];
}

export interface ConnectionRecord {
  server: string; country: string; country_code: string;
  connected_at: string; duration_secs: number; bytes_sent: number; bytes_recv: number;
}

export interface SpeedTestResult {
  download_mbps: number; upload_mbps: number; ping_ms: number;
  server: string; timestamp: string;
}

export interface ControlTowerConfig {
  enabled: boolean; ad_blocking: boolean; tracker_blocking: boolean;
  malware_protection: boolean; phishing_protection: boolean; adult_content: boolean;
  custom_blocklist: string[]; dns_provider: string; custom_dns: string | null;
  ads_blocked: number; trackers_blocked: number; threats_blocked: number;
}

export interface NotificationConfig {
  enabled: boolean; on_connect: boolean; on_disconnect: boolean;
  on_killswitch: boolean; on_connection_failed: boolean; on_subscription_expiring: boolean;
}

export interface ConnectionCheck {
  alive: boolean; state: string; duration_secs?: number;
  bytes_sent?: number; bytes_recv?: number; will_reconnect?: boolean;
}

export interface PingResult {
  hostname: string; name: string; city: string;
  load: number; ping_ms: number; score: number;
}

export interface DiagnosticResult {
  daemon_reachable: boolean; dns_working: boolean;
  vpn_server_reachable: boolean; vpn_server_ping_ms: number;
  killswitch_state: string; dns_override_active: boolean;
  resolv_conf: string; journal_last_50: string; strongswan_sas: string;
}

// Shape returned by the `vpn_run_diagnostics` helper that DiagnosticsScreen renders.
export interface DiagnosticsInfo {
  helper_installed: boolean;
  helper_setuid: boolean;
  helper_path: string;
  strongswan_running: boolean;
  protocol: string;
  tunnel_status: Record<string, string>;
  config_dir: string;
  logged_in: boolean;
  kill_switch: boolean;
  auto_reconnect: boolean;
  auto_connect: boolean;
  split_tunnel_enabled: boolean;
  split_domains_count: number;
  dns_servers: string[];
}

export interface BreachResult {
  email: string; breached: boolean; breach_count: number;
  breaches: string[]; password_in_dump: boolean;
}

export interface SecurityScanResult {
  dns_leak: { passed: boolean; using_vpn_dns: boolean; detail: string };
  ipv6_leak: { passed: boolean; ipv6_reachable: boolean; detail: string };
  webrtc_leak: { passed: boolean; public_ip: string; detail: string };
  connection_integrity: { passed: boolean; vpn_active: boolean; detail: string };
}

export interface SubscriptionInfo {
  payment_url: string; username: string; plan: string;
  account_type: number; sub_end_epoch: number; opened_browser: boolean;
}

export async function vpnPingServers(country: string): Promise<PingResult[]> {
  try {
    const r = await invoke('vpn_ping_servers', { country });
    return (r as PingResult[]) ?? [];
  } catch { return []; }
}

export async function vpnCreateAccount(email: string, password: string): Promise<{ ok: boolean; error?: string }> {
  try {
    await invoke('vpn_create_account', { email, password });
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function vpnRunSpeedTestPrivado(): Promise<SpeedTestResult | null> {
  try {
    return await invoke('vpn_run_speed_test_privado') as SpeedTestResult;
  } catch { return null; }
}

export async function vpnGetControlTowerFull(): Promise<Record<string, unknown>> {
  try {
    return await invoke('vpn_get_control_tower_full') as Record<string, unknown>;
  } catch { return {}; }
}

export async function vpnSaveControlTowerProfile(config: ControlTowerConfig): Promise<void> {
  await invoke('vpn_save_control_tower_profile', { config });
}

export async function vpnRunDiagnostics(): Promise<DiagnosticResult | null> {
  try {
    return await invoke('vpn_run_diagnostics') as DiagnosticResult;
  } catch { return null; }
}

export async function vpnCheckBreach(email: string): Promise<BreachResult | null> {
  try {
    return await invoke('vpn_check_breach', { email }) as BreachResult;
  } catch { return null; }
}

export async function vpnSecurityScan(): Promise<SecurityScanResult | null> {
  try {
    return await invoke('vpn_security_scan') as SecurityScanResult;
  } catch { return null; }
}

export async function vpnPauseConnection(durationSecs: number): Promise<{ ok: boolean; error?: string }> {
  try {
    await invoke('vpn_pause_connection', { durationSecs });
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function vpnManageSubscription(): Promise<SubscriptionInfo | null> {
  try {
    return await invoke('vpn_manage_subscription') as SubscriptionInfo;
  } catch { return null; }
}

// ====== ROUTING RULES (daemon-backed) ======
// These call the daemon's /routing/rules endpoints via Tauri so the GUI, CLI,
// and LLM share one source of truth and the daemon re-applies rules live.

export async function vpnListRoutingRules(): Promise<RoutingRule[]> {
  try {
    return await invoke('vpn_list_routing_rules') as RoutingRule[];
  } catch { return []; }
}

export async function vpnAddRoutingRule(rule: RoutingRule): Promise<{ ok: boolean; rules: RoutingRule[]; error?: string }> {
  try {
    const rules = await invoke('vpn_add_routing_rule', { rule }) as RoutingRule[];
    return { ok: true, rules: rules ?? [] };
  } catch (e) {
    return { ok: false, rules: [], error: String(e) };
  }
}

export async function vpnUpdateRoutingRule(rule: RoutingRule): Promise<{ ok: boolean; rules: RoutingRule[]; error?: string }> {
  try {
    const rules = await invoke('vpn_update_routing_rule', { rule }) as RoutingRule[];
    return { ok: true, rules: rules ?? [] };
  } catch (e) {
    return { ok: false, rules: [], error: String(e) };
  }
}

export async function vpnDeleteRoutingRule(id: string): Promise<{ ok: boolean; rules: RoutingRule[]; error?: string }> {
  try {
    const rules = await invoke('vpn_delete_routing_rule', { id }) as RoutingRule[];
    return { ok: true, rules: rules ?? [] };
  } catch (e) {
    return { ok: false, rules: [], error: String(e) };
  }
}

export async function vpnReorderRoutingRules(order: string[]): Promise<{ ok: boolean; rules: RoutingRule[]; error?: string }> {
  try {
    const rules = await invoke('vpn_reorder_routing_rules', { order }) as RoutingRule[];
    return { ok: true, rules: rules ?? [] };
  } catch (e) {
    return { ok: false, rules: [], error: String(e) };
  }
}

export async function vpnSetActiveExit(server: string): Promise<{ ok: boolean; server?: string; ip?: string; error?: string }> {
  try {
    const r = await invoke('vpn_set_active_exit', { server }) as Record<string, unknown>;
    return { ok: true, server: r?.server as string, ip: r?.ip as string };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function vpnSendNotification(title: string, body: string): Promise<void> {
  await invoke('vpn_send_notification', { title, body });
}

export async function vpnReportError(errorType: string, message: string, context?: Record<string, unknown>): Promise<void> {
  await invoke('vpn_report_error', { errorType, message, context });
}

export async function vpnTrackEvent(eventName: string, properties?: Record<string, unknown>): Promise<void> {
  await invoke('vpn_track_event', { eventName, properties });
}

// Sentry/Analytics configuration is managed via JSON files on disk:
// ~/.config/privado-vpn/sentry.json and analytics.json
// The vpn_report_error and vpn_track_event commands read these at runtime.
