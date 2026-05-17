<script lang="ts">
  interface Connection {
    id: string;
    server: string;
    country: string;
    country_code: string;
    duration: number;
    dataTransferred: number;
    timestamp: number;
    flag: string;
  }

  interface Props {
    connections: Connection[];
    isLoading: boolean;
    sortBy: 'date' | 'duration' | 'data';
    onSortChange: (sortBy: 'date' | 'duration' | 'data') => void;
    onClearHistory?: () => void;
  }

  let {
    connections = [],
    isLoading = false,
    sortBy = 'date',
    onSortChange,
    onClearHistory
  }: Props = $props();

  let expandedConnections = $state<Set<string>>(new Set());

  function toggleConnection(id: string) {
    const newSet = new Set(expandedConnections);
    if (newSet.has(id)) {
      newSet.delete(id);
    } else {
      newSet.add(id);
    }
    expandedConnections = newSet;
  }

  function formatDuration(seconds: number): string {
    if (seconds < 60) {
      return seconds + 's';
    }
    if (seconds < 3600) {
      const mins = Math.floor(seconds / 60);
      return mins + 'm';
    }
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return hours + 'h ' + mins + 'm';
  }

  function formatData(bytes: number): string {
    if (bytes < 1024) {
      return bytes + ' B';
    }
    if (bytes < 1024 * 1024) {
      return (bytes / 1024).toFixed(1) + ' KB';
    }
    if (bytes < 1024 * 1024 * 1024) {
      return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
    }
    return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
  }

  function formatDateTime(timestamp: number): string {
    const date = new Date(timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date.toDateString() === today.toDateString()) {
      return 'Today, ' + date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' });
    } else if (date.toDateString() === yesterday.toDateString()) {
      return 'Yesterday, ' + date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' });
    } else {
      return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) + ', ' + date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' });
    }
  }

  let sortedConnections = $derived.by(() => {
    const sorted = [...connections];
    if (sortBy === 'date') {
      sorted.sort((a, b) => b.timestamp - a.timestamp);
    } else if (sortBy === 'duration') {
      sorted.sort((a, b) => b.duration - a.duration);
    } else if (sortBy === 'data') {
      sorted.sort((a, b) => b.dataTransferred - a.dataTransferred);
    }
    return sorted;
  });

  let totalStats = $derived.by(() => {
    let totalDuration = 0;
    let totalData = 0;
    for (const conn of connections) {
      totalDuration += conn.duration;
      totalData += conn.dataTransferred;
    }
    return { totalDuration, totalData };
  });

  function getSortButtonClass(btnSort: 'date' | 'duration' | 'data'): string {
    return sortBy === btnSort ? 'active' : '';
  }
</script>

<div class="history-page">
  {#if isLoading}
    <div class="loading-state">
      <div class="spinner"></div>
      <div class="loading-text">Loading connection history...</div>
    </div>
  {:else if connections.length === 0}
    <div class="empty-state">
      <div class="empty-icon">📋</div>
      <div class="empty-title">No connections</div>
      <div class="empty-desc">Your VPN connection history will appear here</div>
    </div>
  {:else}
    <div class="history-content">
      <div class="stats-header card">
        <div class="stats-row">
          <div class="stat-item">
            <div class="stat-label">Total Sessions</div>
            <div class="stat-value">{connections.length}</div>
          </div>
          <div class="divider"></div>
          <div class="stat-item">
            <div class="stat-label">Total Time</div>
            <div class="stat-value">{formatDuration(totalStats.totalDuration)}</div>
          </div>
          <div class="divider"></div>
          <div class="stat-item">
            <div class="stat-label">Data Used</div>
            <div class="stat-value">{formatData(totalStats.totalData)}</div>
          </div>
        </div>
      </div>

      <div class="sort-bar">
        <div class="sort-label">Sort by:</div>
        <div class="sort-buttons">
          <button
            class="sort-btn {getSortButtonClass('date')}"
            onclick={() => onSortChange('date')}
          >
            Date
          </button>
          <button
            class="sort-btn {getSortButtonClass('duration')}"
            onclick={() => onSortChange('duration')}
          >
            Duration
          </button>
          <button
            class="sort-btn {getSortButtonClass('data')}"
            onclick={() => onSortChange('data')}
          >
            Data
          </button>
        </div>
      </div>

      <div class="connections-list">
        {#each sortedConnections as conn (conn.id)}
          {@const isExpanded = expandedConnections.has(conn.id)}
          <button
            class="connection-item"
            class:expanded={isExpanded}
            onclick={() => toggleConnection(conn.id)}
          >
            <div class="connection-header">
              <div class="connection-left">
                <span class="connection-toggle">{isExpanded ? '▼' : '▶'}</span>
                <span class="connection-flag">{conn.flag}</span>
                <div class="connection-info">
                  <div class="connection-server">{conn.server}</div>
                  <div class="connection-detail">{conn.country}</div>
                </div>
              </div>
              <div class="connection-right">
                <div class="connection-duration">{formatDuration(conn.duration)}</div>
              </div>
            </div>

            {#if isExpanded}
              <div class="connection-details">
                <div class="detail-row">
                  <span class="detail-label">Time</span>
                  <span class="detail-value">{formatDateTime(conn.timestamp)}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">Duration</span>
                  <span class="detail-value">{formatDuration(conn.duration)}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">Data Transferred</span>
                  <span class="detail-value">{formatData(conn.dataTransferred)}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">Country</span>
                  <span class="detail-value">{conn.country}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">Server</span>
                  <span class="detail-value">{conn.server}</span>
                </div>
              </div>
            {/if}
          </button>
        {/each}
      </div>

      {#if onClearHistory}
        <div class="action-container">
          <button
            class="btn-clear"
            onclick={onClearHistory}
            title="Clear entire connection history"
          >
            Clear History
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .history-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 0 0 20px 0;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 20px;
    gap: 16px;
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--border);
    border-top-color: var(--orange-500);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .loading-text {
    color: var(--text-muted);
    font-size: 14px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 20px;
    gap: 12px;
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
    text-align: center;
  }

  .history-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .stats-header {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .stats-row {
    display: flex;
    align-items: center;
    justify-content: space-around;
  }

  .stat-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    flex: 1;
  }

  .stat-label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .stat-value {
    font-size: 16px;
    font-weight: 700;
    color: var(--orange-500);
  }

  .divider {
    width: 1px;
    height: 32px;
    background: var(--border);
  }

  .sort-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 4px;
  }

  .sort-label {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    white-space: nowrap;
  }

  .sort-buttons {
    display: flex;
    gap: 6px;
    flex: 1;
  }

  .sort-btn {
    flex: 1;
    padding: 8px 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .sort-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .sort-btn.active {
    background: var(--orange-500);
    border-color: var(--orange-500);
    color: white;
  }

  .connections-list {
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .connection-item {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 12px 16px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.1s;
    text-align: left;
  }

  .connection-item:last-child {
    border-bottom: none;
  }

  .connection-item:hover {
    background: var(--bg-hover);
  }

  .connection-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .connection-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }

  .connection-toggle {
    display: inline-block;
    width: 16px;
    text-align: center;
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .connection-flag {
    font-size: 20px;
    flex-shrink: 0;
  }

  .connection-info {
    flex: 1;
    min-width: 0;
  }

  .connection-server {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .connection-detail {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .connection-right {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-left: 12px;
  }

  .connection-duration {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
    font-family: var(--font-mono);
  }

  .connection-details {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
  }

  .detail-label {
    color: var(--text-muted);
  }

  .detail-value {
    color: var(--text-dim);
    font-weight: 600;
    font-family: var(--font-mono);
    text-align: right;
  }

  .action-container {
    padding: 0 4px;
  }

  .btn-clear {
    width: 100%;
    padding: 12px 24px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--red-500);
    border-radius: var(--radius-pill);
    color: var(--red-500);
    font-weight: 600;
    font-size: 14px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-clear:hover {
    background: rgba(239, 68, 68, 0.2);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
