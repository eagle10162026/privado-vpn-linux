<script lang="ts">
  import type { DiagnosticsInfo } from '$lib/tauri';

  interface Props {
    diagnostics: DiagnosticsInfo | null;
    loading: boolean;
    onRefresh: () => void;
    onBack: () => void;
  }

  let { diagnostics = null, loading = false, onRefresh, onBack }: Props = $props();

  function statusColor(ok: boolean): string {
    return ok ? 'var(--success)' : 'var(--error)';
  }

  function fmtBool(v: boolean | undefined): string {
    return v ? 'Yes' : 'No';
  }
</script>

<div class="diag-screen">
  <div class="diag-header">
    <button class="back-btn" onclick={onBack}>← Back</button>
    <h2>Diagnostics</h2>
    <button class="refresh-btn" onclick={onRefresh} disabled={loading}>
      {loading ? 'Checking...' : 'Refresh'}
    </button>
  </div>

  {#if !diagnostics}
    <div class="loading-state">
      <div class="spin"></div>
      <span>Running diagnostics...</span>
    </div>
  {:else}
    <div class="diag-grid">
      <div class="diag-card">
        <h3>Helper Binary</h3>
        <div class="diag-row">
          <span>Installed</span>
          <span class="val" style="color: {statusColor(diagnostics.helper_installed)}">{fmtBool(diagnostics.helper_installed)}</span>
        </div>
        <div class="diag-row">
          <span>Setuid Root</span>
          <span class="val" style="color: {statusColor(diagnostics.helper_setuid)}">{fmtBool(diagnostics.helper_setuid)}</span>
        </div>
        <div class="diag-row">
          <span>Path</span>
          <span class="val mono">{diagnostics.helper_path}</span>
        </div>
        {#if !diagnostics.helper_setuid && diagnostics.helper_installed}
          <div class="diag-fix">
            Run: <code>sudo chmod u+s {diagnostics.helper_path}</code>
          </div>
        {/if}
      </div>

      <div class="diag-card">
        <h3>strongSwan / IKEv2</h3>
        <div class="diag-row">
          <span>Service Running</span>
          <span class="val" style="color: {statusColor(diagnostics.strongswan_running)}">{fmtBool(diagnostics.strongswan_running)}</span>
        </div>
        <div class="diag-row">
          <span>Protocol</span>
          <span class="val">{diagnostics.protocol}</span>
        </div>
      </div>

      <div class="diag-card">
        <h3>Tunnel Status</h3>
        {#if diagnostics.tunnel_status}
          {#each Object.entries(diagnostics.tunnel_status) as [key, val]}
            <div class="diag-row">
              <span>{key}</span>
              <span class="val">{typeof val === 'boolean' ? fmtBool(val) : String(val)}</span>
            </div>
          {/each}
        {:else}
          <p class="empty">No tunnel active</p>
        {/if}
      </div>

      <div class="diag-card">
        <h3>Configuration</h3>
        <div class="diag-row">
          <span>Config Dir</span>
          <span class="val mono">{diagnostics.config_dir}</span>
        </div>
        <div class="diag-row">
          <span>Logged In</span>
          <span class="val">{fmtBool(diagnostics.logged_in)}</span>
        </div>
        <div class="diag-row">
          <span>Kill Switch</span>
          <span class="val">{fmtBool(diagnostics.kill_switch)}</span>
        </div>
        <div class="diag-row">
          <span>Auto Reconnect</span>
          <span class="val">{fmtBool(diagnostics.auto_reconnect)}</span>
        </div>
        <div class="diag-row">
          <span>Auto Connect</span>
          <span class="val">{fmtBool(diagnostics.auto_connect)}</span>
        </div>
        <div class="diag-row">
          <span>Split Tunnel</span>
          <span class="val">{fmtBool(diagnostics.split_tunnel_enabled)} ({diagnostics.split_domains_count} domains)</span>
        </div>
        <div class="diag-row">
          <span>DNS Servers</span>
          <span class="val mono">{diagnostics.dns_servers?.join(', ') || 'default'}</span>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .diag-screen { display: flex; flex-direction: column; gap: 1rem; height: 100%; }
  .diag-header { display: flex; align-items: center; gap: 1rem; }
  .diag-header h2 { flex: 1; margin: 0; font-size: 1.1rem; }
  .back-btn, .refresh-btn {
    background: var(--surface-2, #2a2a3e);
    border: 1px solid var(--border, #3a3a5e);
    color: var(--text, #e0e0e0);
    padding: 0.4rem 0.8rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.8rem;
  }
  .back-btn:hover, .refresh-btn:hover { background: var(--surface-3, #3a3a5e); }
  .refresh-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .diag-grid { display: flex; flex-direction: column; gap: 0.75rem; overflow-y: auto; }
  .diag-card {
    background: var(--surface-1, #1e1e30);
    border: 1px solid var(--border, #3a3a5e);
    border-radius: 8px;
    padding: 0.75rem;
  }
  .diag-card h3 { margin: 0 0 0.5rem; font-size: 0.9rem; color: var(--primary, #f39c12); }
  .diag-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.25rem 0; font-size: 0.8rem;
    border-bottom: 1px solid var(--border, #2a2a4e);
  }
  .diag-row:last-child { border-bottom: none; }
  .val { color: var(--text-dim, #aaa); text-align: right; max-width: 60%; overflow: hidden; text-overflow: ellipsis; }
  .mono { font-family: monospace; font-size: 0.75rem; }
  .diag-fix {
    margin-top: 0.5rem; padding: 0.5rem; background: #331100;
    border: 1px solid #f39c12; border-radius: 4px; font-size: 0.75rem;
  }
  .diag-fix code { color: #f39c12; font-family: monospace; }
  .empty { color: var(--text-dim); font-style: italic; font-size: 0.8rem; }

  .loading-state { display: flex; align-items: center; justify-content: center; gap: 0.5rem; padding: 2rem; }
  .spin {
    width: 20px; height: 20px; border: 2px solid var(--border);
    border-top-color: var(--primary); border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
