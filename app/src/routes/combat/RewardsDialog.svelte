<script lang="ts" module>
  /** A defeated enemy, as the rewards list it. */
  export type Fallen = { id: string; name: string; cr: string | null; xp: number | null; themes: string[] };
</script>

<script lang="ts">
  // End of the fight: the defeated enemies' XP split among the heroes, and each
  // creature's individual treasure (2024 DMG, coins by CR) rolled on demand.
  import Modal from '../../lib/components/Modal.svelte';
  import { app } from '../../lib/state.svelte';
  import { coinTotal, rollIndividualTreasure, splitXp, type CoinRoll } from '../../lib/rewards';

  let {
    open = $bindable(false),
    fallen,
    heroes,
    onNote,
  }: {
    open: boolean;
    fallen: Fallen[];
    /** Heroes in the fight, the default split. */
    heroes: number;
    /** Appends the summary to the encounter notes. */
    onNote: (text: string) => void;
  } = $props();

  let split = $state(1);
  let coins = $state<Record<string, CoinRoll | null>>({});
  $effect(() => {
    if (open) {
      split = Math.max(1, heroes);
      coins = {};
    }
  });

  const total = $derived(fallen.reduce((a, f) => a + (f.xp ?? 0), 0));
  const unknown = $derived(fallen.filter((f) => f.xp === null).length);
  const share = $derived(splitXp(total, split));
  const rolled = $derived(fallen.some((f) => f.id in coins));
  const themes = $derived([...new Set(fallen.flatMap((f) => f.themes))]);

  function rollAll() {
    coins = Object.fromEntries(fallen.map((f) => [f.id, rollIndividualTreasure(f.cr)]));
  }
  function rollOne(f: Fallen) {
    coins = { ...coins, [f.id]: rollIndividualTreasure(f.cr) };
  }

  const summary = $derived(
    [
      `Rewards: ${total.toLocaleString()} XP${split > 1 ? ` (${share.each.toLocaleString()} each for ${split}${share.rest ? `, ${share.rest} left over` : ''})` : ''}`,
      `Defeated: ${fallen.map((f) => f.name).join(', ')}`,
      rolled && coinTotal(Object.values(coins)) ? `Coins: ${coinTotal(Object.values(coins))}` : '',
      themes.length ? `Treasure themes: ${themes.join(', ')}` : '',
    ].filter(Boolean).join('\n'),
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(summary);
      app.toast('success', 'Rewards copied');
    } catch (e) {
      app.error('Could not copy', e);
    }
  }
</script>

<Modal title="Fight over: rewards" bind:open width={760}>
  <div class="rw" data-testid="rewards">
    <div class="xp">
      <div class="big"><b>{total.toLocaleString()}</b> XP</div>
      <label class="field split">Split among
        <input class="input num" type="number" min="1" max="20" aria-label="Heroes to split the XP among" bind:value={split}
          onchange={() => (split = Math.max(1, Math.round(split || 1)))} />
        heroes
      </label>
      <div class="big"><b>{share.each.toLocaleString()}</b> XP each</div>
      {#if share.rest}<span class="muted">{share.rest} left over</span>{/if}
    </div>
    {#if unknown}<p class="note">{unknown} creature(s) have no CR and give no XP here.</p>{/if}

    <div class="list">
      <div class="head"><span>Creature</span><span>CR</span><span class="num">XP</span><span>Coins</span><span></span></div>
      {#each fallen as f (f.id)}
        <div class="line">
          <span class="name">{f.name}{#if f.themes.length}<span class="themes" title="Treasure themes (2024 DMG)">{f.themes.join(', ')}</span>{/if}</span>
          <span>{f.cr ?? '—'}</span>
          <span class="num">{f.xp?.toLocaleString() ?? '—'}</span>
          <span title={coins[f.id]?.detail ?? ''}>{f.id in coins ? (coins[f.id] ? `${coins[f.id]!.amount.toLocaleString()} ${coins[f.id]!.coin}` : 'No CR') : ''}</span>
          <button class="btn ghost" onclick={() => rollOne(f)} disabled={!f.cr} aria-label="Roll coins for {f.name}" title="Roll its individual treasure">🎲</button>
        </div>
      {/each}
    </div>

    <div class="coins">
      <button class="btn" onclick={rollAll}>{rolled ? 'Roll all again' : 'Roll individual treasure'}</button>
      {#if rolled}<span class="big">{coinTotal(Object.values(coins)) || 'No coins'}</span>{/if}
      <span class="muted">2024 DMG: CR 0–4 3d6 gp, 5–10 2d8 × 10 gp, 11–16 2d10 × 10 pp, 17+ 2d8 × 100 pp.</span>
    </div>
    {#if themes.length}
      <p class="note">Treasure themes for a hoard or magic items: <b>{themes.join(', ')}</b> (roll those in the DMG).</p>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={copy}>Copy</button>
    <button class="btn" onclick={() => { onNote(summary); app.toast('success', 'Added to the encounter notes'); }}>Add to encounter notes</button>
    <button class="btn primary" onclick={() => (open = false)}>Done</button>
  {/snippet}
</Modal>

<style>
  .rw {
    display: grid;
    gap: var(--space-4);
    font-size: 16px;
  }
  .xp,
  .coins {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-4);
  }
  .big {
    font-size: 20px;
  }
  .big b {
    font-size: 26px;
  }
  .split {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  input.num {
    width: 72px;
  }
  .list {
    display: grid;
    max-height: 40vh;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .head,
  .line {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 60px 80px 140px 44px;
    align-items: center;
    gap: var(--space-3);
    padding: 6px var(--space-3);
  }
  .head {
    font-weight: 600;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
  }
  .line + .line {
    border-top: 1px solid var(--border);
  }
  .name {
    font-weight: 600;
  }
  .themes {
    margin-left: 8px;
    font-weight: 400;
    font-size: 14px;
    color: var(--text-muted);
  }
</style>
