<script lang="ts">
  import { untrack } from 'svelte';
  interface Notification {
    id: string;
    title: string;
    message: string;
    timestamp: Date;
    type: 'info' | 'success' | 'warning' | 'error';
  }

  interface Props {
    config?: {
      desktopNotifications?: boolean;
      connectNotify?: boolean;
      disconnectNotify?: boolean;
      killSwitchNotify?: boolean;
      failureNotify?: boolean;
      subscriptionNotify?: boolean;
    };
    onSave?: (config: unknown) => void;
  }

  let {
    config = {},
    onSave
  }: Props = $props();

  // Seed once from the config prop (untrack = intentional initial read; the
  // form owns the values after mount and persists on change).
  let settings = $state(untrack(() => ({
    desktopNotifications: config.desktopNotifications ?? true,
    connectNotify: config.connectNotify ?? true,
    disconnectNotify: config.disconnectNotify ?? true,
    killSwitchNotify: config.killSwitchNotify ?? true,
    failureNotify: config.failureNotify ?? true,
    subscriptionNotify: config.subscriptionNotify ?? true
  })));

  let recentNotifications = $state<Notification[]>([
    {
      id: '1',
      title: 'VPN Connected',
      message: 'Successfully connected to Netherlands server',
      timestamp: new Date(Date.now() - 5 * 60000),
      type: 'success'
    },
    {
      id: '2',
      title: 'Tracker Blocked',
      message: 'Control Tower blocked 47 tracking requests',
      timestamp: new Date(Date.now() - 15 * 60000),
      type: 'info'
    },
    {
      id: '3',
      title: 'Kill Switch Triggered',
      message: 'VPN disconnected unexpectedly, internet blocked',
      timestamp: new Date(Date.now() - 1 * 3600000),
      type: 'warning'
    },
    {
      id: '4',
      title: 'Connection Failed',
      message: 'Unable to connect to server. Please try another location.',
      timestamp: new Date(Date.now() - 2 * 3600000),
      type: 'error'
    },
    {
      id: '5',
      title: 'Subscription Expiring',
      message: 'Your premium subscription expires in 7 days',
      timestamp: new Date(Date.now() - 1 * 86400000),
      type: 'warning'
    }
  ]);

  function toggleSetting(key: keyof typeof settings) {
    settings[key] = !settings[key];
    saveConfig();
  }

  function handleToggleChange(e: Event, key: keyof typeof settings) {
    const checked = (e.target as HTMLInputElement).checked;
    settings[key] = checked;
    saveConfig();
  }

  function saveConfig() {
    if (onSave) {
      onSave({
        desktopNotifications: settings.desktopNotifications,
        connectNotify: settings.connectNotify,
        disconnectNotify: settings.disconnectNotify,
        killSwitchNotify: settings.killSwitchNotify,
        failureNotify: settings.failureNotify,
        subscriptionNotify: settings.subscriptionNotify
      });
    }
  }

  function clearAllNotifications() {
    if (confirm('Are you sure you want to clear all notifications?')) {
      recentNotifications = [];
    }
  }

  function formatTime(date: Date): string {
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);

    if (minutes < 1) return 'Just now';
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    if (days < 7) return `${days}d ago`;

    return date.toLocaleDateString();
  }

  function getNotificationIcon(type: string): string {
    switch (type) {
      case 'success':
        return '✅';
      case 'error':
        return '❌';
      case 'warning':
        return '⚠️';
      case 'info':
      default:
        return 'ℹ️';
    }
  }

  function getNotificationColor(type: string): string {
    switch (type) {
      case 'success':
        return 'notification-success';
      case 'error':
        return 'notification-error';
      case 'warning':
        return 'notification-warning';
      case 'info':
      default:
        return 'notification-info';
    }
  }
</script>

<div class="notifications-page">
  <div class="settings-section card">
    <div class="section-title">Notification Settings</div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Desktop Notifications</div>
        <div class="setting-desc">Show notifications on your device</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={settings.desktopNotifications}
          onchange={(e) => handleToggleChange(e, 'desktopNotifications')}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">VPN Connected</div>
        <div class="setting-desc">Notify when VPN connection succeeds</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={settings.connectNotify}
          disabled={!settings.desktopNotifications}
          onchange={(e) => handleToggleChange(e, 'connectNotify')}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">VPN Disconnected</div>
        <div class="setting-desc">Notify when VPN connection closes</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={settings.disconnectNotify}
          disabled={!settings.desktopNotifications}
          onchange={(e) => handleToggleChange(e, 'disconnectNotify')}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Kill Switch Triggered</div>
        <div class="setting-desc">Alert when kill switch activates</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={settings.killSwitchNotify}
          disabled={!settings.desktopNotifications}
          onchange={(e) => handleToggleChange(e, 'killSwitchNotify')}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Connection Failed</div>
        <div class="setting-desc">Alert when connection attempt fails</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={settings.failureNotify}
          disabled={!settings.desktopNotifications}
          onchange={(e) => handleToggleChange(e, 'failureNotify')}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Subscription Expiring</div>
        <div class="setting-desc">Remind when subscription is about to expire</div>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          checked={settings.subscriptionNotify}
          disabled={!settings.desktopNotifications}
          onchange={(e) => handleToggleChange(e, 'subscriptionNotify')}
        />
        <span class="toggle-track"></span>
        <span class="toggle-thumb"></span>
      </label>
    </div>
  </div>

  <div class="notifications-history card">
    <div class="history-header">
      <div class="section-title">Recent Notifications</div>
      {#if recentNotifications.length > 0}
        <button class="btn-clear" onclick={clearAllNotifications}>Clear All</button>
      {/if}
    </div>

    {#if recentNotifications.length > 0}
      <div class="notifications-list">
        {#each recentNotifications as notification (notification.id)}
          <div class="notification-item {getNotificationColor(notification.type)}">
            <span class="notification-icon">{getNotificationIcon(notification.type)}</span>
            <div class="notification-content">
              <div class="notification-title">{notification.title}</div>
              <div class="notification-message">{notification.message}</div>
              <div class="notification-time">{formatTime(notification.timestamp)}</div>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <div class="notifications-empty">
        <div class="empty-icon">🔔</div>
        <div class="empty-title">No Notifications</div>
        <div class="empty-desc">You're all caught up! No recent notifications.</div>
      </div>
    {/if}
  </div>
</div>

<style>
  .notifications-page {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    padding: var(--sp-4);
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    padding: 0 0 var(--sp-3) 0;
    border-bottom: 1px solid var(--border);
    margin-bottom: var(--sp-3);
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-3) 0;
    border-bottom: 1px solid var(--border);
  }

  .setting-row:last-of-type {
    border-bottom: none;
  }

  .setting-info {
    flex: 1;
  }

  .setting-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .setting-desc {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: var(--sp-1);
  }

  .toggle {
    position: relative;
    width: 44px;
    height: 24px;
    flex-shrink: 0;
    margin-left: var(--sp-3);
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

  .toggle input:disabled + .toggle-track {
    background: var(--bg-card-tertiary);
    cursor: not-allowed;
    opacity: 0.5;
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

  .notifications-history {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .history-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--border);
    padding-bottom: var(--sp-3);
    margin-bottom: var(--sp-2);
  }

  .history-header .section-title {
    padding: 0;
    border: none;
    margin: 0;
  }

  .btn-clear {
    padding: var(--sp-2) var(--sp-3);
    background: transparent;
    color: var(--red-500);
    font-size: 12px;
    font-weight: 600;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-clear:hover {
    background: rgba(239, 68, 68, 0.1);
  }

  .notifications-list {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }

  .notification-item {
    display: flex;
    gap: var(--sp-3);
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border-radius: var(--radius-md);
    border-left: 4px solid var(--border);
  }

  .notification-item.notification-success {
    border-left-color: var(--green-500);
    background: rgba(34, 197, 94, 0.08);
  }

  .notification-item.notification-error {
    border-left-color: var(--red-500);
    background: rgba(239, 68, 68, 0.08);
  }

  .notification-item.notification-warning {
    border-left-color: var(--yellow-500);
    background: rgba(234, 179, 8, 0.08);
  }

  .notification-item.notification-info {
    border-left-color: var(--orange-500);
    background: rgba(223, 92, 5, 0.08);
  }

  .notification-icon {
    font-size: 18px;
    flex-shrink: 0;
    margin-top: 2px;
  }

  .notification-content {
    flex: 1;
    min-width: 0;
  }

  .notification-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .notification-message {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: var(--sp-1);
    word-wrap: break-word;
  }

  .notification-time {
    font-size: 10px;
    color: var(--text-muted);
    margin-top: var(--sp-2);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .notifications-empty {
    padding: var(--sp-8) var(--sp-4);
    text-align: center;
  }

  .empty-icon {
    font-size: 48px;
    margin-bottom: var(--sp-3);
  }

  .empty-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: var(--sp-2);
  }

  .empty-desc {
    font-size: 12px;
    color: var(--text-muted);
  }
</style>
