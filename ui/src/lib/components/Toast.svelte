<script lang="ts">
  interface ToastItem {
    id: string;
    title: string;
    message: string;
    type: 'info' | 'success' | 'warning' | 'error';
  }

  interface Props {
    toasts: ToastItem[];
    onDismiss: (id: string) => void;
  }

  let { toasts = [], onDismiss }: Props = $props();

  function getIcon(type: string): string {
    switch (type) {
      case 'success': return '\u2705';
      case 'error': return '\u274C';
      case 'warning': return '\u26A0\uFE0F';
      default: return '\u2139\uFE0F';
    }
  }

  function getBorderColor(type: string): string {
    switch (type) {
      case 'success': return 'var(--green-500)';
      case 'error': return 'var(--red-500)';
      case 'warning': return 'var(--yellow-500)';
      default: return 'var(--orange-500)';
    }
  }
</script>

{#if toasts.length > 0}
  <div class="toast-container">
    {#each toasts as toast (toast.id)}
      <div class="toast-item" style="border-left-color: {getBorderColor(toast.type)}">
        <span class="toast-icon">{getIcon(toast.type)}</span>
        <div class="toast-content">
          <div class="toast-title">{toast.title}</div>
          <div class="toast-message">{toast.message}</div>
        </div>
        <button class="toast-close" onclick={() => onDismiss(toast.id)}>&times;</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    top: 12px;
    right: 12px;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 340px;
    pointer-events: none;
  }

  .toast-item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    background: var(--bg-card, #1C1E22);
    border: 1px solid var(--border, #30343B);
    border-left: 4px solid var(--orange-500);
    border-radius: var(--radius-md, 12px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    animation: toast-in 0.25s ease;
    pointer-events: auto;
  }

  .toast-icon {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .toast-content {
    flex: 1;
    min-width: 0;
  }

  .toast-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary, #fff);
  }

  .toast-message {
    font-size: 11px;
    color: var(--text-muted, #A0A5B1);
    margin-top: 2px;
    word-wrap: break-word;
  }

  .toast-close {
    background: none;
    border: none;
    color: var(--text-muted, #A0A5B1);
    font-size: 18px;
    cursor: pointer;
    padding: 0 2px;
    flex-shrink: 0;
    line-height: 1;
    transition: color 0.15s;
  }

  .toast-close:hover {
    color: var(--text-primary, #fff);
  }

  @keyframes toast-in {
    from { opacity: 0; transform: translateX(20px); }
    to { opacity: 1; transform: translateX(0); }
  }
</style>
