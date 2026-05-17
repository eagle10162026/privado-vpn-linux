<script lang="ts">
  interface Props {
    config?: {
      mode?: 'full' | 'split';
      domains?: string[];
    };
    onSave?: (config: unknown) => void;
  }

  let {
    config = {},
    onSave
  }: Props = $props();

  let mode = $state<'full' | 'split'>(config.mode ?? 'full');
  let selectedDomains = $state<string[]>(config.domains ?? []);
  let newDomain = $state('');

  const presetGroups = [
    {
      name: 'Streaming',
      emoji: '🎬',
      domains: ['netflix.com', 'hulu.com', 'disneyplus.com', 'primevideo.com', 'appletv.com']
    },
    {
      name: 'Social Media',
      emoji: '👥',
      domains: ['facebook.com', 'twitter.com', 'instagram.com', 'tiktok.com', 'snapchat.com', 'linkedin.com']
    },
    {
      name: 'Banking',
      emoji: '🏦',
      domains: ['chase.com', 'bankofamerica.com', 'wellsfargo.com', 'citibank.com', 'paypal.com']
    },
    {
      name: 'Shopping',
      emoji: '🛍️',
      domains: ['amazon.com', 'ebay.com', 'walmart.com', 'target.com', 'etsy.com']
    }
  ];

  let expandedGroups = $state<Set<string>>(new Set());

  function toggleMode(newMode: 'full' | 'split') {
    mode = newMode;
    saveConfig();
  }

  function toggleDomain(domain: string) {
    if (selectedDomains.includes(domain)) {
      selectedDomains = selectedDomains.filter(d => d !== domain);
    } else {
      selectedDomains = [...selectedDomains, domain];
    }
    saveConfig();
  }

  function addCustomDomain() {
    const domain = newDomain.trim().toLowerCase();
    if (domain && domain.includes('.') && !selectedDomains.includes(domain)) {
      selectedDomains = [...selectedDomains, domain];
      newDomain = '';
      saveConfig();
    }
  }

  function removeDomain(domain: string) {
    selectedDomains = selectedDomains.filter(d => d !== domain);
    saveConfig();
  }

  function addGroupDomains(domains: string[]) {
    const newDomains = domains.filter(d => !selectedDomains.includes(d));
    if (newDomains.length > 0) {
      selectedDomains = [...selectedDomains, ...newDomains];
      saveConfig();
    }
  }

  function toggleGroup(groupName: string) {
    if (expandedGroups.has(groupName)) {
      expandedGroups.delete(groupName);
    } else {
      expandedGroups.add(groupName);
    }
    expandedGroups = expandedGroups;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      addCustomDomain();
    }
  }

  function saveConfig() {
    if (onSave) {
      onSave({
        mode,
        domains: selectedDomains
      });
    }
  }

  function exportConfig() {
    const data = JSON.stringify({ mode, domains: selectedDomains }, null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'phantom-mode-config.json';
    a.click();
    URL.revokeObjectURL(url);
  }

  function importConfig(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (event) => {
      try {
        const data = JSON.parse(event.target?.result as string);
        if (data.mode && Array.isArray(data.domains)) {
          mode = data.mode;
          selectedDomains = data.domains;
          saveConfig();
        }
      } catch (err) {
        console.error('Import failed:', err);
      }
    };
    reader.readAsText(file);
    input.value = '';
  }
</script>

<div class="phantom-mode-page">
  <div class="header-card card">
    <div class="header-title">🔄 Phantom Mode</div>
    <div class="header-desc">Choose which domains route through the VPN tunnel</div>
  </div>

  <div class="mode-section card">
    <div class="section-title">Tunnel Mode</div>
    <div class="mode-options">
      <label class="mode-option" class:active={mode === 'full'}>
        <input
          type="radio"
          name="mode"
          value="full"
          checked={mode === 'full'}
          onchange={() => toggleMode('full')}
        />
        <div class="mode-content">
          <div class="mode-label">Full Tunnel</div>
          <div class="mode-desc">All traffic routes through VPN</div>
        </div>
      </label>
      <label class="mode-option" class:active={mode === 'split'}>
        <input
          type="radio"
          name="mode"
          value="split"
          checked={mode === 'split'}
          onchange={() => toggleMode('split')}
        />
        <div class="mode-content">
          <div class="mode-label">Split Tunnel</div>
          <div class="mode-desc">Only selected domains use VPN</div>
        </div>
      </label>
    </div>
  </div>

  {#if mode === 'split'}
    <div class="domains-section card">
      <div class="section-title">
        Active Domains
        <span class="domain-count">({selectedDomains.length})</span>
      </div>

      <div class="custom-domain-group">
        <div class="custom-input-group">
          <input
            type="text"
            placeholder="example.com"
            bind:value={newDomain}
            onkeydown={handleKeyDown}
            class="custom-input"
          />
          <button class="btn-add-domain" onclick={addCustomDomain}>Add</button>
        </div>
      </div>

      {#if selectedDomains.length > 0}
        <div class="active-domains">
          <div class="domains-label">Selected for VPN:</div>
          {#each selectedDomains as domain (domain)}
            <div class="domain-item">
              <span class="domain-text">{domain}</span>
              <button class="btn-domain-remove" onclick={() => removeDomain(domain)}>✕</button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="presets-label">Quick Add Presets:</div>
      <div class="preset-groups">
        {#each presetGroups as group (group.name)}
          <div class="preset-group">
            <button
              class="preset-header"
              onclick={() => toggleGroup(group.name)}
            >
              <span class="preset-emoji">{group.emoji}</span>
              <span class="preset-name">{group.name}</span>
              <span class="preset-chevron" class:expanded={expandedGroups.has(group.name)}>
                ▶
              </span>
            </button>

            {#if expandedGroups.has(group.name)}
              <div class="preset-content">
                <div class="preset-domains">
                  {#each group.domains as domain (domain)}
                    <label class="preset-domain-item">
                      <input
                        type="checkbox"
                        checked={selectedDomains.includes(domain)}
                        onchange={() => toggleDomain(domain)}
                      />
                      <span class="preset-domain-text">{domain}</span>
                    </label>
                  {/each}
                </div>
                <button
                  class="btn-add-all"
                  onclick={() => addGroupDomains(group.domains)}
                >
                  Add All {group.name}
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <div class="actions-footer card">
    <button class="btn-export" onclick={exportConfig}>📥 Export Config</button>
    <label class="btn-import">
      📤 Import Config
      <input
        type="file"
        accept=".json"
        onchange={importConfig}
        style="display: none;"
      />
    </label>
  </div>
</div>

<style>
  .phantom-mode-page {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    padding: var(--sp-4);
  }

  .header-card {
    background: linear-gradient(135deg, rgba(223, 92, 5, 0.15), rgba(223, 92, 5, 0.08));
    border: 1px solid rgba(223, 92, 5, 0.3);
  }

  .header-title {
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .header-desc {
    font-size: 13px;
    color: var(--text-muted);
    margin-top: var(--sp-2);
  }

  .mode-section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .domain-count {
    font-size: 12px;
    color: var(--orange-500);
    margin-left: var(--sp-2);
  }

  .mode-options {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }

  .mode-option {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border: 2px solid transparent;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all 0.15s;
  }

  .mode-option:hover {
    background: var(--bg-hover);
  }

  .mode-option.active {
    border-color: var(--orange-500);
    background: rgba(223, 92, 5, 0.1);
  }

  .mode-option input[type="radio"] {
    margin-top: 4px;
    cursor: pointer;
    accent-color: var(--orange-500);
  }

  .mode-content {
    flex: 1;
  }

  .mode-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .mode-desc {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .domains-section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }

  .custom-domain-group {
    margin-bottom: var(--sp-2);
  }

  .custom-input-group {
    display: flex;
    gap: var(--sp-2);
  }

  .custom-input {
    flex: 1;
    padding: 10px 14px;
    font-size: 13px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }

  .custom-input:focus {
    border-color: var(--orange-500);
  }

  .custom-input::placeholder {
    color: var(--text-placeholder);
  }

  .btn-add-domain {
    padding: 10px 18px;
    background: var(--green-500);
    color: white;
    font-weight: 600;
    font-size: 13px;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
    white-space: nowrap;
  }

  .btn-add-domain:hover {
    background: var(--green-600);
  }

  .active-domains {
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    border-radius: var(--radius-md);
    margin-bottom: var(--sp-2);
  }

  .domains-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    margin-bottom: var(--sp-2);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .domain-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-2) 0;
    border-bottom: 1px solid var(--border);
  }

  .domain-item:last-child {
    border-bottom: none;
  }

  .domain-text {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-primary);
  }

  .btn-domain-remove {
    background: transparent;
    color: var(--red-500);
    font-size: 14px;
    padding: 4px 8px;
    cursor: pointer;
    transition: color 0.15s;
  }

  .btn-domain-remove:hover {
    color: #dc2626;
  }

  .presets-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    margin-bottom: var(--sp-2);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .preset-groups {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }

  .preset-group {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .preset-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    cursor: pointer;
    transition: background 0.15s;
    text-align: left;
  }

  .preset-header:hover {
    background: var(--bg-hover);
  }

  .preset-emoji {
    font-size: 16px;
    flex-shrink: 0;
  }

  .preset-name {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .preset-chevron {
    font-size: 10px;
    color: var(--text-muted);
    transition: transform 0.2s;
  }

  .preset-chevron.expanded {
    transform: rotate(90deg);
  }

  .preset-content {
    padding: var(--sp-3);
    background: var(--bg-page);
    border-top: 1px solid var(--border);
  }

  .preset-domains {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    margin-bottom: var(--sp-3);
  }

  .preset-domain-item {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    cursor: pointer;
  }

  .preset-domain-item input[type="checkbox"] {
    accent-color: var(--orange-500);
    cursor: pointer;
  }

  .preset-domain-text {
    font-size: 12px;
    color: var(--text-secondary);
    font-family: var(--font-mono);
  }

  .btn-add-all {
    width: 100%;
    padding: var(--sp-2) var(--sp-3);
    background: var(--orange-500);
    color: white;
    font-weight: 600;
    font-size: 12px;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-add-all:hover {
    background: var(--orange-400);
  }

  .actions-footer {
    display: flex;
    gap: var(--sp-3);
    margin-top: var(--sp-2);
  }

  .btn-export,
  .btn-import {
    flex: 1;
    padding: var(--sp-3);
    background: var(--bg-card-secondary);
    color: var(--text-primary);
    font-weight: 600;
    font-size: 13px;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-export:hover,
  .btn-import:hover {
    background: var(--bg-hover);
  }
</style>
