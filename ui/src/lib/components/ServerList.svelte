<script lang="ts">
  interface Server {
    name: string;
    hostname: string;
    city: string;
    country: string;
    country_code: string;
    ip: string;
    status: string;
    load: number;
    flag?: string; // optional — the list computes the emoji via getFlag(country_code)
  }

  interface Props {
    servers: Server[];
    selectedCountry: string;
    isLoading: boolean;
    onSelectServer: (countryCode: string) => void;
    onLoadServers?: () => void;
  }

  let {
    servers = [],
    selectedCountry = 'NL',
    isLoading = false,
    onSelectServer,
    onLoadServers
  }: Props = $props();

  const COUNTRY_FLAGS: Record<string, string> = {
    US: '🇺🇸', GB: '🇬🇧', DE: '🇩🇪', NL: '🇳🇱', CH: '🇨🇭', FR: '🇫🇷',
    JP: '🇯🇵', SG: '🇸🇬', CA: '🇨🇦', AU: '🇦🇺', SE: '🇸🇪', NO: '🇳🇴',
    DK: '🇩🇰', FI: '🇫🇮', AT: '🇦🇹', BE: '🇧🇪', IT: '🇮🇹', ES: '🇪🇸',
    PT: '🇵🇹', IE: '🇮🇪', PL: '🇵🇱', CZ: '🇨🇿', RO: '🇷🇴', HU: '🇭🇺',
    BR: '🇧🇷', MX: '🇲🇽', AR: '🇦🇷', IN: '🇮🇳', KR: '🇰🇷', HK: '🇭🇰',
    TW: '🇹🇼', IL: '🇮🇱', TR: '🇹🇷', ZA: '🇿🇦', UA: '🇺🇦', RU: '🇷🇺',
    BG: '🇧🇬', HR: '🇭🇷', LV: '🇱🇻', IS: '🇮🇸', NZ: '🇳🇿', PH: '🇵🇭',
    MY: '🇲🇾', TH: '🇹🇭', CO: '🇨🇴', CL: '🇨🇱', PE: '🇵🇪', EG: '🇪🇬'
  };

  function getFlag(cc: string): string {
    return COUNTRY_FLAGS[cc?.toUpperCase()] ?? '🌍';
  }

  let searchQuery = $state('');

  let filteredServers = $derived.by(() => {
    if (!searchQuery.trim()) return servers;
    const q = searchQuery.toLowerCase();
    return servers.filter(s =>
      s.country.toLowerCase().includes(q) ||
      s.city.toLowerCase().includes(q) ||
      s.country_code.toLowerCase().includes(q) ||
      s.hostname.toLowerCase().includes(q)
    );
  });

  let groupedServers = $derived.by(() => {
    const groups: Record<string, Server[]> = {};
    for (const s of filteredServers) {
      const key = `${s.country_code}_${s.country}`;
      if (!groups[key]) {
        groups[key] = [];
      }
      groups[key].push(s);
    }

    const entries = Object.entries(groups).map(([key, entries]) => {
      const cc = entries[0].country_code;
      const country = entries[0].country;
      return { cc, country, entries: entries.sort((a, b) => a.load - b.load) };
    });

    return entries.sort((a, b) => a.country.localeCompare(b.country));
  });

  let expandedCountries = $state<Set<string>>(new Set());

  function toggleCountry(cc: string) {
    const newSet = new Set(expandedCountries);
    if (newSet.has(cc)) {
      newSet.delete(cc);
    } else {
      newSet.add(cc);
    }
    expandedCountries = newSet;
  }

  let favorites = $state<Set<string>>(new Set());

  function toggleFavorite(hostname: string) {
    const newSet = new Set(favorites);
    if (newSet.has(hostname)) {
      newSet.delete(hostname);
    } else {
      newSet.add(hostname);
    }
    favorites = newSet;
  }

  let favoriteServers = $derived.by(() => {
    return servers.filter(s => favorites.has(s.hostname));
  });

  function handleSelectServer(server: Server) {
    onSelectServer(server.country_code);
  }
</script>

<div class="servers-page">
  <div class="search-bar">
    <input
      type="text"
      bind:value={searchQuery}
      placeholder="Search country or city..."
      class="search-input"
    />
  </div>

  {#if isLoading}
    <div class="loading-state">
      <div class="spinner"></div>
      <div class="loading-text">Loading servers...</div>
    </div>
  {:else if servers.length === 0}
    <div class="empty-state">
      <div class="empty-icon">🌍</div>
      <div class="empty-title">No servers loaded</div>
      <div class="empty-desc">Tap the button below to fetch the latest server list</div>
      {#if onLoadServers}
        <button class="btn-load" onclick={onLoadServers}>Load Servers</button>
      {/if}
    </div>
  {:else}
    <div class="servers-list">
      {#if favoriteServers.length > 0}
        <div class="section-favorites">
          <div class="section-header">
            <span class="section-icon">⭐</span>
            <span class="section-title">Favorites ({favoriteServers.length})</span>
          </div>
          {#each favoriteServers as server}
            <button
              class="server-row"
              class:selected={server.country_code === selectedCountry}
              onclick={() => handleSelectServer(server)}
            >
              <div class="server-left">
                <span class="server-flag">{getFlag(server.country_code)}</span>
                <div class="server-info">
                  <div class="server-name">{server.city || server.name}</div>
                  <div class="server-detail">{server.hostname}</div>
                </div>
              </div>
              <div class="server-right">
                <div class="server-load">
                  <div class="load-bar">
                    <div class="load-fill" style="width: {server.load}%"></div>
                  </div>
                  <div class="load-text">{server.load.toFixed(0)}%</div>
                </div>
                <span
                  role="button"
                  tabindex="0"
                  class="btn-favorite"
                  class:active={favorites.has(server.hostname)}
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleFavorite(server.hostname);
                  }}
                  onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); toggleFavorite(server.hostname); } }}
                >
                  ⭐
                </span>
                {#if server.country_code === selectedCountry}
                  <span class="server-check">✓</span>
                {/if}
              </div>
            </button>
          {/each}
        </div>
      {/if}

      <div class="section-countries">
        {#each groupedServers as { cc, country, entries }}
          {@const isExpanded = expandedCountries.has(cc)}
          <div class="country-group">
            <button
              class="country-header"
              onclick={() => toggleCountry(cc)}
            >
              <span class="country-toggle">{isExpanded ? '▼' : '▶'}</span>
              <span class="country-flag">{getFlag(cc)}</span>
              <span class="country-name">{country}</span>
              <span class="country-count">({entries.length})</span>
            </button>

            {#if isExpanded}
              <div class="country-servers">
                {#each entries as server}
                  <button
                    class="server-row"
                    class:selected={server.country_code === selectedCountry}
                    onclick={() => handleSelectServer(server)}
                  >
                    <div class="server-left">
                      <div class="server-info" style="margin-left: 0;">
                        <div class="server-name">{server.city || server.name}</div>
                        <div class="server-detail">{server.hostname}</div>
                      </div>
                    </div>
                    <div class="server-right">
                      <div class="server-load">
                        <div class="load-bar">
                          <div class="load-fill" style="width: {server.load}%"></div>
                        </div>
                        <div class="load-text">{server.load.toFixed(0)}%</div>
                      </div>
                      <span
                        role="button"
                        tabindex="0"
                        class="btn-favorite"
                        class:active={favorites.has(server.hostname)}
                        onclick={(e) => {
                          e.stopPropagation();
                          toggleFavorite(server.hostname);
                        }}
                        onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); toggleFavorite(server.hostname); } }}
                      >
                        ☆
                      </span>
                      {#if server.country_code === selectedCountry}
                        <span class="server-check">✓</span>
                      {/if}
                    </div>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .servers-page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 20px;
  }

  .search-bar {
    padding: 0 4px;
    sticky: 0;
    z-index: 10;
  }

  .search-input {
    width: 100%;
    padding: 10px 14px;
    font-size: 14px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }

  .search-input:focus {
    border-color: var(--orange-500);
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

  .btn-load {
    margin-top: 12px;
    background: var(--orange-500);
    color: white;
    font-weight: 600;
    font-size: 14px;
    padding: 10px 24px;
    border: none;
    border-radius: var(--radius-pill);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-load:hover {
    background: var(--orange-400);
  }

  .servers-list {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-favorites {
    padding: 0 4px;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 8px;
  }

  .section-icon {
    font-size: 14px;
  }

  .section-title {
    flex: 1;
  }

  .section-countries {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .country-group {
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .country-header {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 16px;
    background: var(--bg-card-secondary);
    border: 1px solid var(--border);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.1s;
  }

  .country-header:hover {
    background: var(--bg-hover);
  }

  .country-toggle {
    display: inline-block;
    width: 16px;
    text-align: center;
    font-size: 10px;
    color: var(--text-muted);
  }

  .country-flag {
    font-size: 20px;
  }

  .country-name {
    flex: 1;
  }

  .country-count {
    color: var(--text-muted);
    font-weight: normal;
    font-size: 12px;
  }

  .country-servers {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-top: none;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }

  .server-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 10px 16px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.1s;
  }

  .server-row:last-child {
    border-bottom: none;
  }

  .server-row:hover {
    background: var(--bg-hover);
  }

  .server-row.selected {
    background: rgba(223, 92, 5, 0.1);
  }

  .server-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }

  .server-flag {
    font-size: 20px;
    flex-shrink: 0;
  }

  .server-info {
    flex: 1;
    min-width: 0;
    margin-left: 0;
  }

  .server-name {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .server-detail {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .server-right {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
  }

  .server-load {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 48px;
  }

  .load-bar {
    width: 100%;
    height: 4px;
    background: var(--bg-input);
    border-radius: 2px;
    overflow: hidden;
  }

  .load-fill {
    height: 100%;
    background: var(--orange-500);
    transition: width 0.3s;
  }

  .load-text {
    font-size: 10px;
    color: var(--text-dim);
    font-family: var(--font-mono);
  }

  .btn-favorite {
    background: transparent;
    border: none;
    font-size: 14px;
    cursor: pointer;
    padding: 4px;
    opacity: 0.5;
    transition: opacity 0.15s;
  }

  .btn-favorite:hover {
    opacity: 1;
  }

  .btn-favorite.active {
    opacity: 1;
  }

  .server-check {
    color: var(--green-500);
    font-size: 16px;
    font-weight: 600;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
