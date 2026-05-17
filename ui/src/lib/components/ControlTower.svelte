<script lang="ts">
  interface Props {
    isActive?: boolean;
    config?: {
      adBlocking?: boolean;
      trackerBlocking?: boolean;
      malwareProtection?: boolean;
      phishingProtection?: boolean;
      adultContent?: boolean;
      adsBlocked?: number;
      trackersBlocked?: number;
      threatsBlocked?: number;
      customBlocklist?: string[];
      dnsProvider?: string;
    };
    onSave?: (config: unknown) => void;
  }

  let {
    isActive = true,
    config = {},
    onSave
  }: Props = $props();

  let filters = $state({
    adBlocking: config.adBlocking ?? true,
    trackerBlocking: config.trackerBlocking ?? true,
    malwareProtection: config.malwareProtection ?? true,
    phishingProtection: config.phishingProtection ?? true,
    adultContent: config.adultContent ?? false
  });

  let stats = $state({
    adsBlocked: config.adsBlocked ?? 12847,
    trackersBlocked: config.trackersBlocked ?? 5234,
    threatsBlocked: config.threatsBlocked ?? 89
  });

  let dnsProvider = $state(config.dnsProvider ?? 'privado');
  let customBlocklist = $state<string[]>(config.customBlocklist ?? []);
  let newDomain = $state('');

  const dnsProviders = [
    { id: 'privado', name: 'Privado DNS', description: 'Privacy-focused DNS' },
    { id: 'cloudflare', name: 'Cloudflare (1.1.1.1)', description: 'Fast & secure' },
    { id: 'google', name: 'Google (8.8.8.8)', description: 'Reliable service' },
    { id: 'quad9', name: 'Quad9 (9.9.9.9)', description: 'Security-focused' },
    { id: 'custom', name: 'Custom', description: 'Enter your own DNS' }
  ];

  const filterCategories = [
    { id: 'adBlocking', icon: '🚫', title: 'Ad Blocking', description: 'Block advertisements across websites' },
    { id: 'trackerBlocking', icon: '👁️', title: 'Tracker Blocking', description: 'Prevent tracking scripts' },
    { id: 'malwareProtection', icon: '🛡️', title: 'Malware Protection', description: 'Block known malware domains' },
    { id: 'phishingProtection', icon: '🎣', title: 'Phishing Protection', description: 'Prevent phishing attacks' },
    { id: 'adultContent', icon: '🔞', title: 'Adult Content', description: 'Block adult websites' }
  ];

  function toggleFilter(filterId: keyof typeof filters) {
    filters[filterId] = !filters[filterId];
    saveConfig();
  }

  function handleDnsChange(providerId: string) {
    dnsProvider = providerId;
    saveConfig();
  }

  function addDomain() {
    const domain = newDomain.trim().toLowerCase();
    if (domain && !customBlocklist.includes(domain) && domain.includes('.')) {
      customBlocklist = [...customBlocklist, domain];
      newDomain = '';
      saveConfig();
    }
  }

  function removeDomain(domain: string) {
    customBlocklist = customBlocklist.filter(d => d !== domain);
    saveConfig();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      addDomain();
    }
  }

  function saveConfig() {
    if (onSave) {
      onSave({
        adBlocking: filters.adBlocking,
        trackerBlocking: filters.trackerBlocking,
        malwareProtection: filters.malwareProtection,
        phishingProtection: filters.phishingProtection,
        adultContent: filters.adultContent,
        dnsProvider,
        customBlocklist
      });
    }
  }

  function formatNumber(num: number): string {
    if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
    if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
    return num.toString();
  }
</script>

<div class="control-tower-page">
  <div class="status-banner" class:active={isActive}>
    <span class="status-icon">{isActive ? '🛡️' : '⚠️'}</span>
    <div class="status-text">
      <div class="status-title">Control Tower {isActive ? 'Active' : 'Inactive'}</div>
      <div class="status-desc">{isActive ? 'Filtering enabled on all devices' : 'Content filtering is off'}</div>
    </div>
  </div>

  <div class="stats-grid card">
    <div class="stat-item">
      <div class="stat-value">{formatNumber(stats.adsBlocked)}</div>
      <div class="stat-label">Ads Blocked</div>
    </div>
    <div class="stat-item">
      <div class="stat-value">{formatNumber(stats.trackersBlocked)}</div>
      <div class="stat-label">Trackers Blocked</div>
    </div>
    <div class="stat-item">
      <div class="stat-value">{formatNumber(stats.threatsBlocked)}</div>
      <div class="stat-label">Threats Prevented</div>
    </div>
  </div>

  <div class="filters-section card">
    <div class="section-title">Filter Categories</div>
    {#each filterCategories as category (category.id)}
      <div class="filter-row">
        <div class="filter-info">
          <div class="filter-icon">{category.icon}</div>
          <div class="filter-text">
            <div class="filter-title">{category.title}</div>
            <div class="filter-desc">{category.description}</div>
          </div>
        </div>
        <label class="toggle">
          <input
            type="checkbox"
            checked={filters[category.id as keyof typeof filters]}
            onchange={() => toggleFilter(category.id as keyof typeof filters)}
          />
          <span class="toggle-track"></span>
          <span class="toggle-thumb"></span>
        </label>
      </div>
    {/each}
  </div>

  <div class="dns-section card">
    <div class="section-title">DNS Provider</div>
    <div class="dns-options">
      {#each dnsProviders as provider (provider.id)}
        <label class="dns-option">
          <input
            type="radio"
            name="dns"
            value={provider.id}
            checked={dnsProvider === provider.id}
            onchange={() => handleDnsChange(provider.id)}
          />
          <div class="dns-label">
            <div class="dns-name">{provider.name}</div>
            <div class="dns-desc">{provider.description}</div>
          </div>
        </label>
      {/each}
    </div>
  </div>

  <div class="blocklist-section card">
    <div class="section-title">Custom Blocklist</div>
    <div class="blocklist-input-group">
      <input
        type="text"
        placeholder="example.com"
        bind:value={newDomain}
        onkeydown={handleKeyDown}
        class="blocklist-input"
      />
      <button class="btn-add" onclick={addDomain}>Add</button>
    </div>
    {#if customBlocklist.length > 0}
      <div class="blocklist-items">
        {#each customBlocklist as domain (domain)}
          <div class="blocklist-item">
            <span class="blocklist-domain">{domain}</span>
            <button class="btn-remove" onclick={() => removeDomain(domain)}>✕</button>
          </div>
        {/each}
      </div>
    {:else}
      <div class="blocklist-empty">No custom domains blocked yet</div>
    {/if}
  </div>
</div>

<style>
  .control-tower-page {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    padding: var(--sp-4);
  }

  .status-banner {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: var(--sp-4);
    background: linear-gradient(135deg, rgba(34, 197, 94, 0.15), rgba(34, 197, 94, 0.08));
    border: 1px solid rgba(34, 197, 94, 0.3);
    border-radius: var(--radius-lg);
    transition: all 0.3s ease;
  }

  .status-banner.active {
    border-color: var(--green-500);
    background: linear-gradient(135deg, rgba(34, 197, 94, 0.2), rgba(34, 197, 94, 0.1));
  }

  .status-icon {
    font-size: 24px;
    flex-shrink: 0;
  }

  .status-text {
    flex: 1;
  }

  .status-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .status-desc {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--sp-3);
  }

  .stat-item {
    text-align: center;
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border-radius: var(--radius-md);
  }

  .stat-value {
    font-size: 20px;
    font-weight: 700;
    color: var(--orange-500);
    font-family: var(--font-mono);
  }

  .stat-label {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 4px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .filters-section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: var(--sp-2);
  }

  .filter-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border-radius: var(--radius-md);
    transition: background 0.15s;
  }

  .filter-row:hover {
    background: var(--bg-hover);
  }

  .filter-info {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
    flex: 1;
  }

  .filter-icon {
    font-size: 20px;
    flex-shrink: 0;
    margin-top: 2px;
  }

  .filter-text {
    flex: 1;
  }

  .filter-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .filter-desc {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .dns-section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .dns-options {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }

  .dns-option {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
  }

  .dns-option:hover {
    background: var(--bg-hover);
  }

  .dns-option input[type="radio"] {
    margin-top: 4px;
    cursor: pointer;
    accent-color: var(--orange-500);
  }

  .dns-label {
    flex: 1;
    min-width: 0;
  }

  .dns-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .dns-desc {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .blocklist-section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .blocklist-input-group {
    display: flex;
    gap: var(--sp-2);
  }

  .blocklist-input {
    flex: 1;
    padding: 10px 14px;
    font-size: 13px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }

  .blocklist-input:focus {
    border-color: var(--orange-500);
  }

  .blocklist-input::placeholder {
    color: var(--text-placeholder);
  }

  .btn-add {
    padding: 10px 18px;
    background: var(--orange-500);
    color: white;
    font-weight: 600;
    font-size: 13px;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
    white-space: nowrap;
  }

  .btn-add:hover {
    background: var(--orange-400);
  }

  .btn-add:active {
    background: var(--orange-600);
  }

  .blocklist-items {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }

  .blocklist-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border-radius: var(--radius-md);
  }

  .blocklist-domain {
    font-size: 13px;
    font-family: var(--font-mono);
    color: var(--text-primary);
  }

  .btn-remove {
    background: transparent;
    color: var(--red-500);
    font-size: 16px;
    padding: 4px 8px;
    cursor: pointer;
    transition: color 0.15s;
  }

  .btn-remove:hover {
    color: #dc2626;
  }

  .blocklist-empty {
    padding: var(--sp-4);
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
