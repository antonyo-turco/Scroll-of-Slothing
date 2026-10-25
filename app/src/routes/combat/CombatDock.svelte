<script lang="ts">
  // Bottom of the tracker, flush on the status bar: the Defeated list and the
  // fight's difficulty strip.
  import Icon from '../../lib/components/Icon.svelte';
  import { RATING_LABEL } from '../../lib/difficulty';
  import type { CombatRow } from '../../lib/types';
  import type { Fight } from './combat-ui';

  let {
    defeated,
    fight,
    locked,
    label,
    onRestore,
    onRemove,
  }: {
    defeated: CombatRow[];
    fight: Fight | null;
    locked: boolean;
    label: (r: CombatRow) => string;
    onRestore: (r: CombatRow) => void;
    onRemove: (r: CombatRow) => void;
  } = $props();
</script>

<div class="dock">
  {#if defeated.length}
    <details class="defeated">
      <summary><Icon name="skull" size={18} /> Defeated ({defeated.length})</summary>
      <ul>
        {#each defeated as r (r.id)}
          <li>
            <span class="d-name">{label(r)}</span>
            {#if r.source}<span class="src">{r.source}</span>{/if}
            <span class="muted d-hp">HP {r.hp_current ?? '—'} / {r.hp_max ?? '—'}</span>
            <span class="grow"></span>
            <button class="btn ghost" onclick={() => onRestore(r)} disabled={locked} title="Back into the turn order"><Icon name="undo" size={16} /> Restore</button>
            <button class="btn ghost icon danger" aria-label="Remove {label(r)} for good" onclick={() => onRemove(r)} disabled={locked}>✕</button>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
  {#if fight}
    <div class="diffbar" data-testid="combat-difficulty">
      <span class="db-title">Difficulty</span>
      <span class="db-rating rating-{fight.d.rating}">{RATING_LABEL[fight.d.rating]}</span>
      <div class="db-track" role="img"
        aria-label="{RATING_LABEL[fight.d.rating]}: {fight.d.adjusted} XP against Low {fight.d.budget.low}, Moderate {fight.d.budget.moderate}, High {fight.d.budget.high}"
        title="Low ≤ {fight.d.budget.low.toLocaleString()} · Moderate ≤ {fight.d.budget.moderate.toLocaleString()} · High ≤ {fight.d.budget.high.toLocaleString()} · Deadly ≤ {(2 * fight.d.budget.high).toLocaleString()} · TPK above">
        {#each fight.zones as z (z.cls)}<span class="db-zone {z.cls}" style:width="{z.w}%"></span>{/each}
        <span class="db-marker" style:left="{fight.pos}%"></span>
      </div>
      <span class="db-xp" title="{fight.enemies} enemies vs {fight.levels.length} heroes (levels {fight.levels.join(', ')}){fight.unknown ? `; ${fight.unknown} without CR` : ''}">
        {fight.d.adjusted.toLocaleString()} XP{fight.d.multiplier !== 1 ? ` ×${fight.d.multiplier}` : ''}
      </span>
    </div>
  {/if}
</div>

<style>
  .dock {
    position: sticky;
    bottom: calc(-1 * var(--space-3));
    z-index: 3;
    display: grid;
    margin: auto calc(-1 * var(--space-3)) calc(-1 * var(--space-3));
  }
  .defeated,
  .diffbar {
    border-top: 1px solid var(--border);
    background: var(--surface-2);
  }
  .defeated {
    width: 100%;
  }
  .defeated summary {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #e5533d;
    height: 44px;
    padding: 0 14px;
    cursor: pointer;
    font-size: 15px;
    font-weight: 600;
  }
  .defeated ul {
    max-height: 30vh;
    overflow: auto;
    list-style: none;
    margin: 0;
    padding: 0 8px 8px;
  }
  .defeated li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-top: 1px solid var(--border);
    font-size: 15px;
  }
  .d-name {
    font-weight: 600;
    text-decoration: line-through;
    color: var(--text-muted);
  }
  .d-hp {
    font-size: 13px;
  }
  .src {
    margin-left: 6px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .grow {
    flex: 1;
  }
  .diffbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 14px;
  }
  .db-title {
    font-weight: 600;
    font-size: 15px;
  }
  .db-rating {
    min-width: 7ch;
    font-weight: 800;
    font-size: 15px;
  }
  .db-rating.rating-trivial,
  .db-rating.rating-low { color: #2fb344; }
  .db-rating.rating-moderate { color: #d6c21a; }
  .db-rating.rating-high { color: #f08c00; }
  .db-rating.rating-deadly { color: #d9381e; }
  .db-rating.rating-tpk { color: var(--text); }
  .db-track {
    position: relative;
    flex: 1;
    display: flex;
    height: 8px;
    border-radius: 999px;
    overflow: hidden;
    cursor: help;
  }
  .db-zone.low { background: #2fb344; }
  .db-zone.moderate { background: #d6c21a; }
  .db-zone.high { background: #f08c00; }
  .db-zone.deadly { background: #d9381e; }
  .db-zone.tpk { background: #111; }
  .db-marker {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.6);
  }
  .db-xp {
    min-width: 9ch;
    text-align: right;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
</style>
