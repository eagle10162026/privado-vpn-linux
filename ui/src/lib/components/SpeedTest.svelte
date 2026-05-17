<script lang="ts">
  interface SpeedTestResult {
    timestamp: number;
    download: number;
    upload: number;
    ping: number;
    server: string;
  }

  interface Props {
    isConnected: boolean;
    isTesting: boolean;
    currentResult: SpeedTestResult | null;
    history: SpeedTestResult[];
    onStartTest: () => void;
  }

  let {
    isConnected = false,
    isTesting = false,
    currentResult = null,
    history = [],
    onStartTest
  }: Props = $props();

  function formatSpeed(mbps: number): string {
    if (mbps >= 1000) {
      return (mbps / 1000).toFixed(1) + ' Gbps';
    }
    return mbps.toFixed(1) + ' Mbps';
  }

  function formatTime(ms: number): string {
    return ms.toFixed(0) + ' ms';
  }

  function formatDate(timestamp: number): string {
    const date = new Date(timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date.toDateString() === today.toDateString()) {
      return date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' });
    } else if (date.toDateString() === yesterday.toDateString()) {
      return 'Yesterday';
    } else {
      return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    }
  }

  function getSpeedCategory(mbps: number): 'slow' | 'normal' | 'fast' {
    if (mbps < 25) return 'slow';
    if (mbps < 100) return 'normal';
    return 'fast';
  }

  let expandedHistoryItems = $state<Set<number>>(new Set());

  function toggleHistoryItem(timestamp: number) {
    const newSet = new Set(expandedHistoryItems);
    if (newSet.has(timestamp)) {
      newSet.delete(timestamp);
    } else {
      newSet.add(timestamp);
    }
    expandedHistoryItems = newSet;
  }
</script>

<div class="speed-test-page">
  <div class="status-banner" class:connected={isConnected} class:disconnected={!isConnected}>
    <span class="status-icon">{isConnected ? '✓' : '✕'}</span>
    <span class="status-text">
      {isConnected ? 'VPN Connected' : 'VPN Not Connected'}
    </span>
  </div>

  {#if currentResult && !isTesting}
    <div class="result-container card">
      <div class="result-header">
        <div class="result-title">Latest Result</div>
        <div class="result-time">{formatDate(currentResult.timestamp)}</div>
      </div>

      <div class="gauge-container">
        <svg class="speed-gauge" viewBox="0 0 200 120" width="200" height="120">
          <defs>
            <linearGradient id="gaugeGradient" x1="0%" y1="0%" x2="100%" y2="0%">
              <stop offset="0%" style="stop-color:var(--red-500);stop-opacity:1" />
              <stop offset="50%" style="stop-color:var(--yellow-500);stop-opacity:1" />
              <stop offset="100%" style="stop-color:var(--green-500);stop-opacity:1" />
            </linearGradient>
          </defs>

          <path
            d="M 20 100 A 80 80 0 0 1 180 100"
            stroke="var(--border)"
            stroke-width="8"
            fill="none"
          />

          <path
            d="M 20 100 A 80 80 0 0 1 180 100"
            stroke="url(#gaugeGradient)"
            stroke-width="8"
            fill="none"
            stroke-dasharray="251.33"
            stroke-dashoffset="calc(251.33 - (251.33 * min({currentResult.download}, 300) / 300))"
            style="transition: stroke-dashoffset 0.6s ease;"
          />

          <circle cx="100" cy="100" r="4" fill="var(--orange-500)" />
          <line
            x1="100"
            y1="100"
            x2="100"
            y2="25"
            stroke="var(--orange-500)"
            stroke-width="2"
            stroke-linecap="round"
            style="transform: rotate(calc((min({currentResult.download}, 300) / 300) * 180deg)); transform-origin: 100px 100px; transition: transform 0.6s ease;"
          />

          <text x="100" y="115" text-anchor="middle" font-size="10" fill="var(--text-muted)">
            0 Mbps
          </text>
          <text x="100" y="25" text-anchor="middle" font-size="10" fill="var(--text-muted)">
            300+ Mbps
          </text>
        </svg>

        <div class="gauge-value">
          {formatSpeed(currentResult.download)}
        </div>
      </div>

      <div class="result-details">
        <div class="detail-row">
          <div class="detail-label">Download</div>
          <div class="detail-value" class:fast={getSpeedCategory(currentResult.download) === 'fast'}>
            {formatSpeed(currentResult.download)}
          </div>
        </div>
        <div class="detail-row">
          <div class="detail-label">Upload</div>
          <div class="detail-value" class:normal={getSpeedCategory(currentResult.upload) === 'normal'}>
            {formatSpeed(currentResult.upload)}
          </div>
        </div>
        <div class="detail-row">
          <div class="detail-label">Ping</div>
          <div class="detail-value">
            {formatTime(currentResult.ping)}
          </div>
        </div>
        <div class="detail-row">
          <div class="detail-label">Server</div>
          <div class="detail-value">{currentResult.server || 'N/A'}</div>
        </div>
      </div>
    </div>
  {:else if isTesting}
    <div class="testing-container card">
      <div class="testing-spinner"></div>
      <div class="testing-text">Running speed test...</div>
      <div class="testing-desc">This may take 30-60 seconds</div>
    </div>
  {:else}
    <div class="empty-state card">
      <div class="empty-icon">⚡</div>
      <div class="empty-title">No tests yet</div>
      <div class="empty-desc">Run your first speed test to see your connection quality</div>
    </div>
  {/if}

  <div class="action-container">
    <button
      class="btn-test"
      onclick={onStartTest}
      disabled={isTesting || !isConnected}
      title={isConnected ? 'Start speed test' : 'Connect VPN first'}
    >
      {isTesting ? 'Testing...' : 'Start Test'}
    </button>
  </div>

  {#if history.length > 0}
    <div class="history-container">
      <div class="history-header">
        <h3 class="history-title">History</h3>
        <div class="history-count">{history.length} test{history.length !== 1 ? 's' : ''}</div>
      </div>

      <div class="history-list">
        {#each history as result}
          {@const isExpanded = expandedHistoryItems.has(result.timestamp)}
          <button
            class="history-item"
            class:expanded={isExpanded}
            onclick={() => toggleHistoryItem(result.timestamp)}
          >
            <div class="history-item-header">
              <div class="history-item-left">
                <span class="history-toggle">{isExpanded ? '▼' : '▶'}</span>
                <span class="history-item-time">{formatDate(result.timestamp)}</span>
              </div>
              <div class="history-item-right">
                <div class="history-item-download">
                  {formatSpeed(result.download)}
                </div>
              </div>
            </div>

            {#if isExpanded}
              <div class="history-item-details">
                <div class="history-detail-row">
                  <span class="history-detail-label">Download</span>
                  <span class="history-detail-value">{formatSpeed(result.download)}</span>
                </div>
                <div class="history-detail-row">
                  <span class="history-detail-label">Upload</span>
                  <span class="history-detail-value">{formatSpeed(result.upload)}</span>
                </div>
                <div class="history-detail-row">
                  <span class="history-detail-label">Ping</span>
                  <span class="history-detail-value">{formatTime(result.ping)}</span>
                </div>
                <div class="history-detail-row">
                  <span class="history-detail-label">Server</span>
                  <span class="history-detail-value">{result.server || 'Unknown'}</span>
                </div>
              </div>
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .speed-test-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 0 0 20px 0;
  }

  .status-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 600;
  }

  .status-banner.connected {
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid var(--green-500);
    color: var(--green-500);
  }

  .status-banner.disconnected {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--red-500);
    color: var(--red-500);
  }

  .status-icon {
    font-size: 16px;
  }

  .status-text {
    flex: 1;
  }

  .result-container {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .result-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
  }

  .result-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .result-time {
    font-size: 12px;
    color: var(--text-muted);
  }

  .gauge-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    margin-bottom: 24px;
  }

  .speed-gauge {
    width: 100%;
    max-width: 200px;
    height: auto;
  }

  .gauge-value {
    font-size: 24px;
    font-weight: 700;
    color: var(--orange-500);
  }

  .result-details {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .detail-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px;
    background: var(--bg-input);
    border-radius: var(--radius-md);
  }

  .detail-label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .detail-value {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-dim);
  }

  .detail-value.fast {
    color: var(--green-500);
  }

  .detail-value.normal {
    color: var(--yellow-500);
  }

  .testing-container {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 40px var(--sp-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    text-align: center;
  }

  .testing-spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--border);
    border-top-color: var(--orange-500);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .testing-text {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .testing-desc {
    font-size: 12px;
    color: var(--text-muted);
  }

  .empty-state {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 40px var(--sp-5);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
  }

  .empty-icon {
    font-size: 48px;
  }

  .empty-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .empty-desc {
    font-size: 13px;
    color: var(--text-muted);
  }

  .action-container {
    padding: 0 4px;
  }

  .btn-test {
    width: 100%;
    padding: 14px 32px;
    background: var(--orange-500);
    border: none;
    border-radius: var(--radius-pill);
    color: white;
    font-weight: 600;
    font-size: 16px;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s;
  }

  .btn-test:hover:not(:disabled) {
    background: var(--orange-400);
  }

  .btn-test:active:not(:disabled) {
    background: var(--orange-600);
    transform: scale(0.98);
  }

  .btn-test:disabled {
    background: var(--text-disabled);
    cursor: not-allowed;
  }

  .history-container {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .history-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .history-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin: 0;
  }

  .history-count {
    font-size: 12px;
    color: var(--text-dim);
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .history-item {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 12px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.1s;
    text-align: left;
  }

  .history-item:last-child {
    border-bottom: none;
  }

  .history-item:hover {
    background: var(--bg-hover);
  }

  .history-item-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .history-item-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
  }

  .history-toggle {
    display: inline-block;
    width: 16px;
    text-align: center;
    font-size: 10px;
    color: var(--text-muted);
  }

  .history-item-time {
    font-size: 13px;
    font-weight: 500;
  }

  .history-item-right {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .history-item-download {
    font-size: 13px;
    font-weight: 600;
    color: var(--orange-500);
    text-align: right;
    min-width: 80px;
  }

  .history-item-details {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .history-detail-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
  }

  .history-detail-label {
    color: var(--text-muted);
  }

  .history-detail-value {
    color: var(--text-dim);
    font-weight: 600;
    font-family: var(--font-mono);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
