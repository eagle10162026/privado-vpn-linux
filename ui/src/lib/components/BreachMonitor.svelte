<script lang="ts">
  import { untrack } from 'svelte';
  interface BreachEntry {
    name: string; title: string; domain: string; date: string;
    data_classes: string[]; description: string;
  }

  interface BreachResult {
    breached: boolean; breaches: BreachEntry[]; count: number;
  }

  interface Props {
    onCheck: (email: string) => Promise<BreachResult>;
    onBack: () => void;
    userEmail?: string;
  }

  let { onCheck, onBack, userEmail = '' }: Props = $props();

  let email = $state(untrack(() => userEmail)); // seed once from prop
  let checking = $state(false);
  let result = $state<BreachResult | null>(null);
  let error = $state('');
  let expandedBreach = $state<string | null>(null);

  async function handleCheck() {
    if (!email.trim()) return;
    checking = true;
    error = '';
    try {
      result = await onCheck(email.trim());
    } catch (e) {
      error = String(e);
    }
    checking = false;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !checking) handleCheck();
  }
</script>

<div class="breach-page">
  <div class="breach-header">
    <button class="back-btn" onclick={onBack}>&#8592; Back</button>
    <span class="breach-title">Data Breach Monitor</span>
  </div>

  <div class="breach-card card">
    <div class="breach-icon">&#128272;</div>
    <div class="breach-status">
      {#if checking}
        Checking...
      {:else if result}
        {result.breached ? `Found in ${result.count} breach${result.count !== 1 ? 'es' : ''}` : 'No breaches found'}
      {:else}
        Check if your email has been compromised
      {/if}
    </div>
    <div class="breach-desc">
      Searches known data breaches for your email address
    </div>
  </div>

  <div class="breach-input-group">
    <input
      type="email"
      bind:value={email}
      placeholder="your@email.com"
      class="breach-input"
      disabled={checking}
      onkeydown={handleKeyDown}
    />
    <button class="btn-check" onclick={handleCheck} disabled={checking || !email.trim()}>
      {checking ? 'Checking...' : 'Check'}
    </button>
  </div>

  {#if error}
    <div class="breach-error">{error}</div>
  {/if}

  {#if result && result.breached}
    <div class="breaches-list card">
      <div class="breaches-header">
        <span class="breaches-label">Breaches ({result.count})</span>
      </div>
      {#each result.breaches as breach (breach.name)}
        <div class="breach-item">
          <button class="breach-item-header" onclick={() => expandedBreach = expandedBreach === breach.name ? null : breach.name}>
            <div class="breach-item-info">
              <div class="breach-item-name">{breach.title || breach.name}</div>
              <div class="breach-item-date">{breach.domain} &mdash; {breach.date}</div>
            </div>
            <span class="breach-chevron">{expandedBreach === breach.name ? '&#9660;' : '&#9654;'}</span>
          </button>
          {#if expandedBreach === breach.name}
            <div class="breach-detail">
              {#if breach.data_classes && breach.data_classes.length > 0}
                <div class="breach-data-label">Exposed data:</div>
                <div class="breach-data-list">
                  {#each breach.data_classes as dc}
                    <span class="breach-data-tag">{dc}</span>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if result && !result.breached}
    <div class="clean-card card">
      <div class="clean-icon">&#128994;</div>
      <div class="clean-title">All Clear</div>
      <div class="clean-desc">Your email was not found in any known data breaches.</div>
    </div>
  {/if}
</div>

<style>
  .breach-page { display: flex; flex-direction: column; gap: 16px; }
  .breach-header {
    display: flex; align-items: center; gap: 12px;
    padding-bottom: 12px; border-bottom: 1px solid var(--border);
  }
  .back-btn {
    background: none; color: var(--orange-500); font-size: 16px;
    font-weight: 600; padding: 4px 8px; border-radius: var(--radius-md);
  }
  .back-btn:hover { background: var(--bg-hover); }
  .breach-title { font-size: 16px; font-weight: 600; }

  .breach-card {
    display: flex; flex-direction: column; align-items: center;
    text-align: center; padding: 32px 20px; gap: 8px;
  }
  .breach-icon { font-size: 48px; }
  .breach-status { font-size: 16px; font-weight: 600; color: var(--text-primary); }
  .breach-desc { font-size: 12px; color: var(--text-muted); }

  .breach-input-group { display: flex; gap: 8px; }
  .breach-input {
    flex: 1; padding: 10px 14px; font-size: 14px;
    background: var(--bg-input); border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-primary);
  }
  .breach-input:focus { border-color: var(--orange-500); }
  .breach-input:disabled { opacity: 0.5; }

  .btn-check {
    padding: 10px 24px; background: var(--orange-500); color: white;
    font-weight: 600; font-size: 14px; border-radius: var(--radius-pill);
    cursor: pointer; white-space: nowrap;
  }
  .btn-check:hover:not(:disabled) { background: var(--orange-400); }
  .btn-check:disabled { background: var(--text-disabled); cursor: not-allowed; }

  .breach-error {
    padding: 10px 14px; background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--red-500); border-radius: var(--radius-md);
    color: var(--red-500); font-size: 13px;
  }

  .breaches-list { padding: 0; overflow: hidden; }
  .breaches-header {
    padding: 12px 16px; border-bottom: 1px solid var(--border);
  }
  .breaches-label { font-size: 14px; font-weight: 600; color: var(--red-500); }

  .breach-item { border-bottom: 1px solid var(--border); }
  .breach-item:last-child { border-bottom: none; }
  .breach-item-header {
    display: flex; align-items: center; justify-content: space-between;
    width: 100%; padding: 12px 16px; background: none; color: var(--text-primary);
    cursor: pointer; text-align: left; transition: background 0.1s;
  }
  .breach-item-header:hover { background: var(--bg-hover); }
  .breach-item-info { flex: 1; }
  .breach-item-name { font-size: 14px; font-weight: 600; }
  .breach-item-date { font-size: 11px; color: var(--text-muted); margin-top: 2px; }
  .breach-chevron { font-size: 10px; color: var(--text-muted); flex-shrink: 0; }

  .breach-detail {
    padding: 8px 16px 14px; background: var(--bg-card-secondary);
    border-top: 1px solid var(--border);
  }
  .breach-data-label { font-size: 11px; color: var(--text-muted); margin-bottom: 6px; text-transform: uppercase; letter-spacing: 0.3px; }
  .breach-data-list { display: flex; flex-wrap: wrap; gap: 6px; }
  .breach-data-tag {
    padding: 3px 10px; background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2); border-radius: var(--radius-pill);
    font-size: 11px; color: var(--red-500);
  }

  .clean-card {
    display: flex; flex-direction: column; align-items: center;
    text-align: center; padding: 32px 20px; gap: 8px;
  }
  .clean-icon { font-size: 48px; }
  .clean-title { font-size: 16px; font-weight: 600; color: var(--green-500); }
  .clean-desc { font-size: 12px; color: var(--text-muted); }
</style>
