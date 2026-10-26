<script lang="ts">
  // Everything that pops up over the tracker: concentration checks, the
  // recharge and legendary-action notices, the turn card and the round splash.
  import type { ConcAlert, LegendItem, RechargeNote, TurnCard } from './combat-ui';

  let {
    concAlerts,
    recharge,
    legend,
    turnCard,
    splash,
    onDismissConc,
    onRecharged,
  }: {
    concAlerts: ConcAlert[];
    recharge: RechargeNote[] | null;
    legend: LegendItem[] | null;
    turnCard: TurnCard | null;
    splash: number | null;
    onDismissConc: (id: number) => void;
    onRecharged: (item: RechargeNote) => void;
  } = $props();
</script>

{#if concAlerts.length}
  <div class="conc-stack" role="alert" aria-live="assertive">
    {#each concAlerts as a (a.id)}
      <div class="conc" data-testid="concentration-alert">
        <span class="conc-text"><b>{a.who}</b> is concentrating and took {a.damage} damage.<br />Constitution save, <b>DC {a.dc}</b>.</span>
        <button class="btn ghost icon" aria-label="Dismiss" onclick={() => onDismissConc(a.id)}>✕</button>
      </div>
    {/each}
  </div>
{/if}

<!-- one column at the top centre: recharge and legendary side by side, the turn card below -->
<div class="notices">
  {#if recharge || legend}
    <div class="notice-row">
      {#if recharge}
        <div class="note-card recharge-notice" role="status" aria-live="polite" data-testid="recharge-notice">
          <span class="ln-label">Recharge</span>
          {#each recharge as it (it.who + it.ability)}
            <span class="ln-item"><b>{it.who}</b> {it.ability}
              <span class="rc-res">{it.need}{it.need < 6 ? '–6' : ''}</span>
              <button class="rc-ok" onclick={() => onRecharged(it)} aria-label="{it.ability} recharged" title="It recharged">✓</button>
            </span>
          {/each}
        </div>
      {/if}
      {#if legend}
        <div class="note-card legend-notice" role="status" aria-live="polite" data-testid="legendary-notice">
          <span class="ln-label">Legendary actions</span>
          {#each legend as it (it.who)}
            <span class="ln-item"><b>{it.who}</b>
              <span class="ln-pips" aria-label="{it.left} left">{#each Array(it.left) as _, i (i)}<span class="ln-pip"></span>{/each}</span>
            </span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
  {#if turnCard}
    {@const c = turnCard}
    <div class="turn-card" role="status" aria-live="polite" data-testid="turn-card">
      <div class="tc-now" class:hero={c.now.hero} class:ally={c.now.side === 'ally'} class:neutral={c.now.side === 'neutral'}>
        <span class="tc-label">Round {c.round} · Now</span>
        <span class="tc-name">{c.now.name}{c.now.count > 1 ? ` ×${c.now.count}` : ''}</span>
      </div>
      {#if c.next}
        <div class="tc-next" class:hero={c.next.hero} class:ally={c.next.side === 'ally'} class:neutral={c.next.side === 'neutral'}>
          <span class="tc-label">Next</span>
          <span class="tc-name small">{c.next.name}{c.next.count > 1 ? ` ×${c.next.count}` : ''}</span>
        </div>
      {/if}
    </div>
  {/if}
</div>

{#if splash !== null}
  <div class="splash" aria-live="polite" data-testid="round-splash"><span class="splash-label">Round</span><span class="splash-num">{splash}</span></div>
{/if}

<style>
  .conc-stack {
    position: fixed;
    top: clamp(96px, 16vh, 136px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 90;
    display: grid;
    gap: var(--space-2);
  }
  .conc {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 420px;
    padding: 14px 16px;
    border: 2px solid #009f7a;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 16px;
    animation: conc-in 0.25s ease-out;
  }
  .conc-text {
    flex: 1;
  }
  @keyframes conc-in {
    from { transform: translateY(-12px); opacity: 0; }
    to { transform: none; opacity: 1; }
  }
  /* a short window keeps the notices clear of the toolbar */
  .notices {
    position: fixed;
    top: clamp(56px, 10vh, 72px);
    left: 50%;
    z-index: 86;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-height: 60vh;
    pointer-events: none;
  }
  .notice-row {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    align-items: stretch;
    gap: 8px;
  }
  .note-card {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    max-width: 80vw;
    padding: 10px 18px;
    border: 1px solid #e0a400;
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 12px 36px rgb(0 0 0 / 0.35);
    font-size: 15px;
    animation: note-life 5s ease forwards;
  }
  .recharge-notice {
    border-color: #e5533d;
    pointer-events: auto;
    animation-duration: 15s;
  }
  .recharge-notice .ln-label {
    color: #e5533d;
  }
  .legend-notice {
    animation-duration: 4s;
  }
  .rc-ok {
    width: 30px;
    height: 30px;
    margin-left: 4px;
    border: 1px solid #e5533d;
    border-radius: 8px;
    background: color-mix(in srgb, #e5533d 18%, transparent);
    color: #e5533d;
    font-size: 16px;
    font-weight: 800;
    cursor: pointer;
  }
  .rc-ok:hover {
    background: color-mix(in srgb, #e5533d 32%, transparent);
  }
  .rc-res {
    font-size: 14px;
    color: var(--text-muted);
  }
  .ln-label {
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #e0a400;
  }
  .ln-item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .ln-pips {
    display: inline-flex;
    gap: 3px;
  }
  .ln-pip {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #e0a400;
  }
  .turn-card {
    display: flex;
    align-items: stretch;
    min-width: 440px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.4);
    overflow: hidden;
    animation: note-life 3.2s ease forwards;
  }
  .tc-now,
  .tc-next {
    --edge: #d9381e;
    display: grid;
    gap: 2px;
    padding: 14px 22px 14px 20px;
    box-shadow: inset 5px 0 0 var(--edge);
  }
  .tc-now {
    flex: 1.4;
  }
  .tc-next {
    flex: 1;
    border-left: 1px solid var(--border);
    background: var(--surface-2);
  }
  .tc-now.hero,
  .tc-next.hero {
    --edge: #4aa3ff;
  }
  .tc-now.ally,
  .tc-next.ally {
    --edge: #f2f2f2;
  }
  :global(html[data-theme='light']) .tc-now.ally,
  :global(html[data-theme='light']) .tc-next.ally {
    --edge: #64748b;
  }
  .tc-now.neutral,
  .tc-next.neutral {
    --edge: #6b6b6b;
  }
  .tc-label {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .tc-name {
    font-size: 26px;
    font-weight: 800;
    line-height: 1.15;
  }
  .tc-name.small {
    font-size: 19px;
    font-weight: 700;
  }
  @keyframes note-life {
    0% { opacity: 0; transform: translateY(-10px); }
    8% { opacity: 1; transform: none; }
    85% { opacity: 1; transform: none; }
    100% { opacity: 0; transform: translateY(-6px); }
  }
  .splash {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-content: center;
    justify-items: center;
    pointer-events: none;
    background: radial-gradient(circle, color-mix(in srgb, var(--accent) 35%, transparent), rgb(0 0 0 / 0.55));
    animation: splash-bg 1.7s ease forwards;
  }
  .splash-label {
    font-size: 28px;
    letter-spacing: 0.4em;
    text-transform: uppercase;
    color: #fff;
    animation: splash-in 1.7s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
  }
  .splash-num {
    font-size: 160px;
    font-weight: 800;
    line-height: 1;
    color: #fff;
    text-shadow: 0 8px 40px rgb(0 0 0 / 0.5);
    animation: splash-in 1.7s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
  }
  @keyframes splash-bg {
    0% { opacity: 0; }
    15% { opacity: 1; }
    75% { opacity: 1; }
    100% { opacity: 0; }
  }
  @keyframes splash-in {
    0% { transform: scale(0.6); opacity: 0; }
    20% { transform: scale(1.05); opacity: 1; }
    30% { transform: scale(1); }
    75% { transform: scale(1); opacity: 1; }
    100% { transform: scale(1.15); opacity: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .note-card,
    .turn-card {
      animation: none;
    }
    .splash-label,
    .splash-num {
      animation: splash-bg 1.7s linear forwards;
    }
  }
</style>
