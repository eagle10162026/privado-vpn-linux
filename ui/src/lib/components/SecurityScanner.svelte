<script lang="ts">
  interface ThreatEntry { file: string; threat: string; }
  interface ScanResult { scanned: boolean; threats_found: number; threats: ThreatEntry[]; scan_path: string; }

  interface Props {
    onScan: (path?: string) => Promise<ScanResult>;
    onDeleteThreat: (filePath: string) => Promise<void>;
    onBack: () => void;
  }

  let { onScan, onDeleteThreat, onBack }: Props = $props();

  let scanning = $state(false);
  let result = $state<ScanResult | null>(null);
  let scanPath = $state('');
  let deletedFiles = $state<Set<string>>(new Set());

  async function startScan() {
    scanning = true;
    result = null;
    deletedFiles = new Set();
    result = await onScan(scanPath || undefined);
    scanning = false;
  }

  async function handleDelete(filePath: string) {
    try {
      await onDeleteThreat(filePath);
      const s = new Set(deletedFiles);
      s.add(filePath);
      deletedFiles = s;
    } catch {}
  }

  let activeThreats = $derived(
    result?.threats.filter(t => !deletedFiles.has(t.file)) ?? []
  );
</script>

<div class="scanner-page">
  <div class="scanner-header">
    <button class="back-btn" onclick={onBack}>&#8592; Back</button>
    <span class="scanner-title">Security Scanner</span>
  </div>

  <div class="scan-card card">
    <div class="scan-icon">{scanning ? '&#128270;' : result ? (result.threats_found > 0 ? '&#9888;&#65039;' : '&#9989;') : '&#128737;&#65039;'}</div>
    <div class="scan-status">
      {#if scanning}
        Scanning files...
      {:else if result}
        {result.threats_found > 0 ? `${activeThreats.length} threat${activeThreats.length !== 1 ? 's' : ''} found` : 'No threats detected'}
      {:else}
        Ready to scan
      {/if}
    </div>
    <div class="scan-desc">
      {#if scanning}
        This may take several minutes depending on the number of files
      {:else if !result}
        Scan your home directory for malware, viruses, and suspicious files using ClamAV
      {:else}
        Scanned: {result.scan_path}
      {/if}
    </div>
  </div>

  <div class="scan-input-group">
    <input
      type="text"
      bind:value={scanPath}
      placeholder="Scan path (default: home directory)"
      class="scan-input"
      disabled={scanning}
    />
    <button class="btn-scan" onclick={startScan} disabled={scanning}>
      {scanning ? 'Scanning...' : 'Scan'}
    </button>
  </div>

  {#if scanning}
    <div class="scanning-indicator">
      <div class="scan-spinner"></div>
      <div class="scan-progress">Analyzing files with ClamAV engine...</div>
    </div>
  {/if}

  {#if result && activeThreats.length > 0}
    <div class="threats-section card">
      <div class="threats-header">
        <span class="threats-title">Threats Found</span>
        <span class="threats-count">{activeThreats.length}</span>
      </div>
      <div class="threats-list">
        {#each activeThreats as threat (threat.file)}
          <div class="threat-item">
            <div class="threat-info">
              <div class="threat-name">{threat.threat}</div>
              <div class="threat-file">{threat.file}</div>
            </div>
            <button class="btn-delete-threat" onclick={() => handleDelete(threat.file)}>
              Delete
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if result && result.threats_found === 0}
    <div class="clean-card card">
      <div class="clean-icon">&#128994;</div>
      <div class="clean-title">All Clear</div>
      <div class="clean-desc">No malware or suspicious files were found in the scanned directory.</div>
    </div>
  {/if}
</div>

<style>
  .scanner-page { display: flex; flex-direction: column; gap: 16px; }

  .scanner-header {
    display: flex; align-items: center; gap: 12px;
    padding-bottom: 12px; border-bottom: 1px solid var(--border);
  }
  .back-btn {
    background: none; color: var(--orange-500); font-size: 16px;
    font-weight: 600; padding: 4px 8px; border-radius: var(--radius-md);
  }
  .back-btn:hover { background: var(--bg-hover); }
  .scanner-title { font-size: 16px; font-weight: 600; }

  .scan-card {
    display: flex; flex-direction: column; align-items: center;
    text-align: center; padding: 32px 20px; gap: 8px;
  }
  .scan-icon { font-size: 48px; }
  .scan-status { font-size: 16px; font-weight: 600; color: var(--text-primary); }
  .scan-desc { font-size: 12px; color: var(--text-muted); max-width: 280px; }

  .scan-input-group { display: flex; gap: 8px; }
  .scan-input {
    flex: 1; padding: 10px 14px; font-size: 13px;
    background: var(--bg-input); border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-primary);
    font-family: var(--font-mono);
  }
  .scan-input:focus { border-color: var(--orange-500); }
  .scan-input:disabled { opacity: 0.5; }

  .btn-scan {
    padding: 10px 24px; background: var(--orange-500); color: white;
    font-weight: 600; font-size: 14px; border-radius: var(--radius-pill);
    cursor: pointer; white-space: nowrap; transition: background 0.15s;
  }
  .btn-scan:hover:not(:disabled) { background: var(--orange-400); }
  .btn-scan:disabled { background: var(--text-disabled); cursor: not-allowed; }

  .scanning-indicator {
    display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 20px;
  }
  .scan-spinner {
    width: 32px; height: 32px; border: 3px solid var(--border);
    border-top-color: var(--orange-500); border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  .scan-progress { font-size: 13px; color: var(--text-muted); }

  .threats-section { padding: 0; overflow: hidden; }
  .threats-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 14px 16px; border-bottom: 1px solid var(--border);
  }
  .threats-title { font-size: 14px; font-weight: 600; color: var(--red-500); }
  .threats-count {
    background: var(--red-500); color: white; font-size: 12px; font-weight: 700;
    padding: 2px 8px; border-radius: var(--radius-pill);
  }
  .threats-list { display: flex; flex-direction: column; }
  .threat-item {
    display: flex; align-items: center; justify-content: space-between;
    padding: 12px 16px; border-bottom: 1px solid var(--border);
  }
  .threat-item:last-child { border-bottom: none; }
  .threat-info { flex: 1; min-width: 0; }
  .threat-name { font-size: 13px; font-weight: 600; color: var(--red-500); }
  .threat-file {
    font-size: 11px; color: var(--text-muted); font-family: var(--font-mono);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px;
  }
  .btn-delete-threat {
    padding: 6px 14px; background: var(--red-500); color: white;
    font-size: 12px; font-weight: 600; border-radius: var(--radius-md);
    cursor: pointer; flex-shrink: 0; margin-left: 12px; transition: background 0.15s;
  }
  .btn-delete-threat:hover { background: #dc2626; }

  .clean-card {
    display: flex; flex-direction: column; align-items: center;
    text-align: center; padding: 32px 20px; gap: 8px;
  }
  .clean-icon { font-size: 48px; }
  .clean-title { font-size: 16px; font-weight: 600; color: var(--green-500); }
  .clean-desc { font-size: 12px; color: var(--text-muted); }

  @keyframes spin { to { transform: rotate(360deg); } }
</style>
