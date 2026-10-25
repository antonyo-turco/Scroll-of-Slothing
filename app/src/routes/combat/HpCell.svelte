<script lang="ts">
  // One row's HP: death saves or the defeat skull, current/max HP with its bar,
  // temporary HP with its bar, and the ± strips that open on click or focus.
  import Icon from '../../lib/components/Icon.svelte';
  import type { CombatRow } from '../../lib/types';

  const HP_STEPS = [-10, -5, -1, 1, 5, 10];
  const TMP_STEPS = [-5, -1, 1, 5, 10];

  let {
    r,
    name,
    locked,
    editing,
    invert,
    first,
    open,
    wound,
    down,
    deathSaves,
    onOpen,
    onSetHp,
    onArrowHp,
    onSetTemp,
    onArrowTemp,
    onStepHp,
    onStepTemp,
    onDeathSaves,
    onDefeat,
  }: {
    r: CombatRow;
    /** The row's label, for accessible names. */
    name: string;
    locked: boolean;
    /** The row's ✎ is on: max HP can change. */
    editing: boolean;
    /** HP is tracked as damage taken. */
    invert: boolean;
    /** Top row: strips open below instead of above. */
    first: boolean;
    open: 'hp' | 'tmp' | null;
    wound: string;
    /** At 0 HP or below. */
    down: boolean;
    /** Death saves of a dying hero; null for everyone else. */
    deathSaves: { s: number; f: number } | null;
    onOpen: (kind: 'hp' | 'tmp') => void;
    onSetHp: (field: 'hp_current' | 'hp_max', raw: string) => void;
    onArrowHp: (e: KeyboardEvent) => void;
    onSetTemp: (raw: string) => void;
    onArrowTemp: (e: KeyboardEvent) => void;
    onStepHp: (d: number) => void;
    onStepTemp: (d: number) => void;
    onDeathSaves: (s: number, f: number) => void;
    onDefeat: () => void;
  } = $props();

  /** Share of HP left, 0–100 (as damage: of HP not yet lost). */
  const pct = $derived.by(() => {
    if (r.hp_current === null || !r.hp_max) return 0;
    const cur = invert ? r.hp_max - r.hp_current : r.hp_current;
    return Math.min(100, Math.max(0, (cur / r.hp_max) * 100));
  });
  /** Green when full, yellow above 50%, orange above 25%, red below. */
  const color = $derived(pct >= 100 ? '#2fb344' : pct > 50 ? '#d6c21a' : pct > 25 ? '#f08c00' : '#d9381e');
  const tmpPct = $derived(r.hp_temp && r.hp_max ? Math.min(100, (r.hp_temp / r.hp_max) * 100) : r.hp_temp ? 100 : 0);
  const sign = (d: number) => `${d > 0 ? '+' : ''}${d}`;
</script>

<div class="hp-cell">
  {#if deathSaves && deathSaves.f < 3}
    {@const ds = deathSaves}
    <div class="death" role="group" aria-label="Death saves of {name}">
      <span class="ds-row ok" title={ds.s >= 3 ? 'Stable' : 'Successes'}>
        {#each [0, 1, 2] as i (i)}<button class="ds" class:on={ds.s > i} aria-label="Success {i + 1}" aria-pressed={ds.s > i} disabled={locked}
          onclick={() => onDeathSaves(ds.s > i ? i : i + 1, ds.f)}></button>{/each}
      </span>
      <span class="ds-row bad" title="Failures">
        {#each [0, 1, 2] as i (i)}<button class="ds" class:on={ds.f > i} aria-label="Failure {i + 1}" aria-pressed={ds.f > i} disabled={locked}
          onclick={() => onDeathSaves(ds.s, ds.f > i ? i : i + 1)}></button>{/each}
      </span>
      {#if ds.s >= 3}<span class="stable">Stable</span>{/if}
    </div>
  {:else if down}
    <button class="skull" aria-label="Set {name} aside as defeated" title="Defeated: move to the list below the tracker"
      onclick={onDefeat} disabled={locked}><Icon name="skull" size={18} /></button>
  {:else}
    <span class="skull-slot" aria-hidden="true"></span>
  {/if}

  <!-- clicking or tabbing into the HP opens its strip -->
  <div class="hp-part" title={wound} role="presentation" onclick={() => onOpen('hp')} onfocusin={() => onOpen('hp')}>
    <div class="hp-inputs">
      <input class="input num" aria-label="Current HP of {name}" value={r.hp_current ?? ''} onchange={(e) => onSetHp('hp_current', e.currentTarget.value)}
        onkeydown={onArrowHp} title="↑/↓ ±1 · Shift ±5 · Ctrl ±10" disabled={locked} />
      <span class="slash">/</span>
      <input class="input num" aria-label="Max HP of {name}" value={r.hp_max ?? ''} onchange={(e) => onSetHp('hp_max', e.currentTarget.value)}
        disabled={locked || !editing} title={editing ? 'Max HP' : 'Click ✎ to change the max HP'} />
    </div>
    <span class="hpbar" aria-hidden="true"><span style:width="{pct}%" style:background={color}></span></span>
    {#if open === 'hp' && !locked}
      <div class="pop" class:below={first}>
        <span class="steps" role="group" aria-label="Change the HP of {name}">
          {#each HP_STEPS as d (d)}
            <button class="btn ghost step" class:minus={d < 0} aria-label="{sign(d)} HP to {name}" onclick={() => onStepHp(d)}>{sign(d)}</button>
          {/each}
        </span>
      </div>
    {/if}
  </div>
  <div class="tmp-part" role="presentation" onclick={() => onOpen('tmp')} onfocusin={() => onOpen('tmp')}>
    <input class="input num temp" class:has={!!r.hp_temp} aria-label="Temporary HP of {name}" placeholder="+tmp" title="Temporary hit points (lost first)"
      value={r.hp_temp ?? ''} onchange={(e) => onSetTemp(e.currentTarget.value)} onkeydown={onArrowTemp} disabled={locked} />
    <span class="hpbar tmpbar" aria-hidden="true"><span style:width="{tmpPct}%"></span></span>
    {#if open === 'tmp' && !locked}
      <div class="pop" class:below={first}>
        <span class="steps tmp-steps" role="group" aria-label="Change the temporary HP of {name}">
          {#each TMP_STEPS as d (d)}
            <button class="btn ghost step" aria-label="{sign(d)} temporary HP to {name}" onclick={() => onStepTemp(d)}>{sign(d)}</button>
          {/each}
        </span>
      </div>
    {/if}
  </div>
</div>

<style>
  .hp-cell {
    display: flex;
    gap: 6px;
    align-items: center;
    white-space: nowrap;
  }
  .hp-part,
  .tmp-part {
    position: relative;
    display: grid;
    gap: 3px;
    cursor: pointer;
  }
  .hp-inputs {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .num {
    width: 52px;
    text-align: center;
  }
  .slash {
    text-align: center;
  }
  /* .num sets a narrower width, so this needs the extra classes */
  .input.num.temp {
    width: 60px;
    padding: 0 4px;
    font-size: 13px;
    color: #4aa3ff;
    border-color: color-mix(in srgb, #4aa3ff 55%, var(--border));
  }
  .input.num.temp::placeholder {
    color: color-mix(in srgb, #4aa3ff 65%, transparent);
  }
  .input.num.temp.has {
    border-color: #4aa3ff;
    font-weight: 700;
  }
  .hpbar {
    height: 4px;
    border-radius: 999px;
    overflow: hidden;
  }
  .hpbar > span {
    display: block;
    height: 100%;
    border-radius: inherit;
  }
  .tmpbar > span {
    background: #4aa3ff;
  }
  /* the strip opens above what was clicked, centred on it; the top row opens below */
  .pop {
    position: absolute;
    left: 50%;
    bottom: 100%;
    transform: translateX(-50%);
    z-index: 6;
    padding-bottom: 6px;
  }
  .pop.below {
    bottom: auto;
    top: 100%;
    padding-bottom: 0;
    padding-top: 6px;
  }
  .steps {
    display: flex;
    gap: 2px;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 8px 28px rgb(0 0 0 / 0.35);
    animation: pop-in 0.1s ease-out;
  }
  .tmp-steps {
    border-color: color-mix(in srgb, #4aa3ff 55%, var(--border));
  }
  @keyframes pop-in {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: none; }
  }
  .step {
    min-width: 40px;
    height: 32px;
    padding: 0 6px;
    border-radius: 7px;
    font-size: 15px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: #2fb344;
  }
  .step:hover {
    background: color-mix(in srgb, currentColor 14%, transparent);
  }
  .step.minus {
    color: #e5533d;
  }
  .tmp-steps .step {
    color: #4aa3ff;
  }
  .skull,
  .skull-slot {
    width: 30px;
    flex: none;
  }
  .skull {
    display: grid;
    place-items: center;
    height: 32px;
    border: 1px solid color-mix(in srgb, #d9381e 60%, var(--border));
    border-radius: 8px;
    background: color-mix(in srgb, #d9381e 18%, transparent);
    color: #e5533d;
    cursor: pointer;
  }
  .skull:hover {
    background: color-mix(in srgb, #d9381e 32%, transparent);
  }
  .death {
    display: grid;
    gap: 3px;
    flex: none;
  }
  .ds-row {
    display: flex;
    gap: 3px;
  }
  .ds {
    width: 12px;
    height: 12px;
    padding: 0;
    border: 2px solid currentColor;
    border-radius: 50%;
    background: transparent;
    cursor: pointer;
  }
  .ds.on {
    background: currentColor;
  }
  .ok {
    color: #2fb344;
  }
  .bad {
    color: #e5533d;
  }
  .stable {
    font-size: 11px;
    font-weight: 700;
    color: #2fb344;
  }
</style>
