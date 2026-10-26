<script lang="ts">
  // The rolls window at the bottom of the side panel: the latest roll, the
  // history when open; open or collapsed is remembered on this computer.
  import type { RollLine } from './combat-ui';

  let { rolls, onClear }: { rolls: RollLine[]; onClear: () => void } = $props();

  const KEY = 'combat.rollsOpen';
  let open = $state(true);
  try {
    open = localStorage.getItem(KEY) !== '0';
  } catch {
    /* storage unavailable */
  }
  function toggle() {
    open = !open;
    try {
      localStorage.setItem(KEY, open ? '1' : '0');
    } catch {
      /* ignore */
    }
  }
  const good = (l: RollLine) => l.tag === 'CRIT' || l.tag === 'SAVED';
  const bad = (l: RollLine) => l.tag === 'MISS' || l.tag === 'FAILED';
</script>

<section class="roll-win" aria-label="Rolls">
  <header>
    <button class="rw-toggle" onclick={toggle} aria-expanded={open}>
      <span aria-hidden="true">{open ? '▾' : '▸'}</span> Rolls
      {#if !open}
        {@const l = rolls[0]}
        <span class="rw-last" class:crit={good(l)} class:miss={bad(l)}>{l.total} · {l.entry}</span>
      {/if}
    </button>
    <button class="btn ghost rw-clear" onclick={onClear} title="Clear the rolls">Clear</button>
  </header>
  {#if open}
    <ol class="roll-log" aria-live="polite">
      {#each rolls as l, i (l.id)}
        <li class:latest={i === 0} class:crit={good(l)} class:miss={bad(l)}>
          <span class="rl-total">{l.total}</span>
          <span class="rl-what">{l.entry} {l.kind === 'hit' ? (l.entry.endsWith('save') ? '' : 'to hit') : 'damage'}{l.tag ? ` · ${l.tag}` : ''}<small>{l.who} · {l.detail}</small></span>
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .roll-win {
    position: sticky;
    bottom: calc(-1 * var(--space-4));
    z-index: 2;
    margin: var(--space-3) calc(-1 * var(--space-2)) calc(-1 * var(--space-4));
    border: 1px solid var(--border);
    border-radius: 10px 10px 0 0;
    background: var(--surface);
    box-shadow: 0 -8px 24px rgb(0 0 0 / 0.3);
  }
  header {
    display: flex;
    align-items: center;
  }
  .rw-toggle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-weight: 700;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }
  .rw-last {
    margin-left: auto;
    font-weight: 600;
    color: var(--text-muted);
  }
  .rw-last.crit {
    color: #2fb344;
  }
  .rw-last.miss {
    color: #e5533d;
  }
  .rw-clear {
    margin-right: 6px;
  }
  .roll-log {
    list-style: none;
    margin: 0;
    padding: 0 8px 8px;
    display: grid;
    gap: 4px;
    max-height: 32vh;
    overflow: auto;
  }
  .roll-log li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    opacity: 0.7;
  }
  .roll-log li.latest {
    opacity: 1;
    padding: 8px 12px;
    border: 1px solid var(--accent);
  }
  .roll-log li.crit {
    border-color: #2fb344;
  }
  .roll-log li.miss {
    border-color: #e5533d;
  }
  .rl-total {
    min-width: 2.5ch;
    font-size: 18px;
    font-weight: 800;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .latest .rl-total {
    font-size: 26px;
  }
  .rl-what {
    display: grid;
    font-size: 14px;
  }
  .rl-what small {
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
