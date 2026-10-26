<script lang="ts">
  // Actions on the rows selected with Shift+click (or S on a focused name).
  let {
    count,
    locked,
    amount = $bindable(null),
    ability = $bindable('dex'),
    dc = $bindable(null),
    half = $bindable(true),
    onHp,
    onSaves,
    onCondition,
    onClear,
  }: {
    count: number;
    locked: boolean;
    amount: number | null;
    ability: string;
    dc: number | null;
    half: boolean;
    onHp: (kind: 'damage' | 'heal' | 'temp') => void;
    onSaves: () => void;
    onCondition: () => void;
    onClear: () => void;
  } = $props();
</script>

<div class="group-bar" role="toolbar" aria-label="Group actions">
  <span class="gb-count">{count} selected</span>
  <input class="input num gb-amount" type="number" min="1" placeholder="Amount" aria-label="Amount" bind:value={amount}
    onkeydown={(e) => e.key === 'Enter' && onHp('damage')} />
  <button class="btn gb-dmg" disabled={!amount || locked} onclick={() => onHp('damage')}>Damage</button>
  <button class="btn gb-heal" disabled={!amount || locked} onclick={() => onHp('heal')}>Heal</button>
  <button class="btn gb-temp" disabled={!amount || locked} onclick={() => onHp('temp')}>Temp HP</button>
  <span class="gb-sep"></span>
  <select class="input compact" aria-label="Save ability" bind:value={ability}>
    {#each ['str', 'dex', 'con', 'int', 'wis', 'cha'] as a (a)}<option value={a}>{a.toUpperCase()}</option>{/each}
  </select>
  <input class="input num" type="number" min="1" placeholder="DC" aria-label="Save DC" bind:value={dc} />
  <button class="btn" onclick={onSaves}>Roll saves</button>
  <label class="check gb-half" title="With save results, creatures that saved take half"><input type="checkbox" bind:checked={half} /> Half on save</label>
  <span class="gb-sep"></span>
  <button class="btn" disabled={locked} onclick={onCondition}>Condition…</button>
  <span class="grow"></span>
  <button class="btn ghost" onclick={onClear} title="Esc">Clear</button>
</div>

<style>
  .group-bar {
    position: sticky;
    top: calc(-1 * var(--space-3));
    z-index: 4;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--accent);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.3);
    font-size: 15px;
  }
  .gb-count {
    font-weight: 700;
    margin-right: 4px;
  }
  .num {
    width: 52px;
    text-align: center;
  }
  .gb-amount {
    width: 90px;
  }
  .gb-dmg,
  .gb-heal,
  .gb-temp {
    color: #fff;
  }
  .gb-dmg {
    background: #c9341c;
    border-color: #c9341c;
  }
  .gb-heal {
    background: #2a9a3f;
    border-color: #2a9a3f;
  }
  .gb-temp {
    background: #2f7fd1;
    border-color: #2f7fd1;
  }
  .gb-sep {
    width: 1px;
    align-self: stretch;
    background: var(--border);
  }
  .gb-half {
    font-size: 14px;
  }
  .grow {
    flex: 1;
  }
</style>
