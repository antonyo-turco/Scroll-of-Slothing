<script lang="ts">
  // Floating dice roller (docs/features/dice-roller.md). Compact: a pill with the
  // latest roll. Expanded: quick dice, the expression line and the recent rolls;
  // any pointer or focus outside collapses it back to Compact. Hidden: nothing.
  import { onMount, tick } from 'svelte';
  import { dice } from '../dice.svelte';
  import Icon from './Icon.svelte';

  const QUICK = ['d4', 'd6', 'd8', 'd10', 'd12', 'd20', 'd100'];
  const MARGIN = 12;

  let root: HTMLDivElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let list: HTMLOListElement | undefined = $state();
  let line = $state('');
  let shake = $state(false);
  let dragging = $state(false);
  let vw = $state(window.innerWidth);
  let vh = $state(window.innerHeight);
  let size = $state({ w: 320, h: 56 });

  onMount(() => {
    void dice.load();
    const onResize = () => ((vw = window.innerWidth), (vh = window.innerHeight));
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });

  // Track the widget's own size so it stays clamped inside the window.
  $effect(() => {
    if (!root) return;
    const ro = new ResizeObserver(() => {
      if (root) size = { w: root.offsetWidth, h: root.offsetHeight };
    });
    ro.observe(root);
    return () => ro.disconnect();
  });

  // Default corner is bottom-right; a saved spot is clamped to the window. When the
  // widget grows on expand it grows upward/leftward rather than off-screen.
  const pos = $derived.by(() => {
    const maxX = Math.max(MARGIN, vw - size.w - MARGIN);
    const maxY = Math.max(MARGIN, vh - size.h - MARGIN);
    const x = dice.x ?? maxX;
    const y = dice.y ?? maxY;
    return { x: Math.min(Math.max(MARGIN, x), maxX), y: Math.min(Math.max(MARGIN, y), maxY) };
  });

  // Collapse on any pointer-down or focus outside the widget.
  $effect(() => {
    if (dice.mode !== 'expanded') return;
    const outside = (t: EventTarget | null) => !!root && t instanceof Node && !root.contains(t);
    const onDown = (e: PointerEvent) => outside(e.target) && dice.collapse();
    const onFocus = (e: FocusEvent) => outside(e.target) && dice.collapse();
    window.addEventListener('pointerdown', onDown, true);
    window.addEventListener('focusin', onFocus, true);
    return () => {
      window.removeEventListener('pointerdown', onDown, true);
      window.removeEventListener('focusin', onFocus, true);
    };
  });

  // Focus the expression line when asked (open from rail / key / palette).
  $effect(() => {
    if (dice.focusTick && dice.mode === 'expanded') void tick().then(() => input?.focus());
  });

  // Keep the newest roll in view.
  $effect(() => {
    void dice.shown.length;
    void dice.latest;
    void tick().then(() => list?.scrollTo({ top: list.scrollHeight }));
  });

  async function roll(notation: string) {
    if (!(await dice.roll(notation))) {
      shake = true;
      setTimeout(() => (shake = false), 350);
      return false;
    }
    return true;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!line.trim()) return;
    if (await roll(line)) line = '';
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      dice.collapse();
      (document.getElementById('content') as HTMLElement | null)?.focus();
    }
  }

  // Drag by the grip (expanded) or the pill (compact); a pill press without movement expands it.
  function startDrag(e: PointerEvent, clickExpands: boolean) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const start = { px: e.clientX, py: e.clientY, x: pos.x, y: pos.y };
    let moved = false;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - start.px;
      const dy = ev.clientY - start.py;
      if (!moved && Math.hypot(dx, dy) < 4) return;
      moved = true;
      dragging = true;
      dice.moveTo(start.x + dx, start.y + dy);
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up);
      dragging = false;
      if (moved) {
        dice.moveTo(pos.x, pos.y); // store the clamped spot
        dice.endMove();
      } else if (clickExpands) {
        dice.expand();
      }
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
  }

  function onPillKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      dice.expand();
    }
  }
</script>

{#if dice.mode !== 'hidden'}
  <!-- Escape anywhere inside the widget collapses it. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={root}
    class="dice"
    class:expanded={dice.mode === 'expanded'}
    class:dragging
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    role="region"
    aria-label="Dice roller"
    onkeydown={onKey}
  >
    {#if dice.mode === 'compact'}
      <div
        class="pill"
        role="button"
        tabindex="0"
        title="Dice roller: click to roll, drag to move"
        aria-label={dice.latest ? `Latest roll ${dice.latest.expression}: ${dice.latest.total}. Open the dice roller` : 'Open the dice roller'}
        onpointerdown={(e) => startDrag(e, true)}
        onkeydown={onPillKey}
      >
        <Icon name="dice" />
        {#if dice.latest}
          <span class="pill-expr">{dice.latest.label ?? dice.latest.expression}</span>
          <span class="pill-total" data-testid="dice-latest">{dice.latest.total}</span>
        {:else}
          <span class="pill-expr">Roll dice</span>
        {/if}
      </div>
      <button class="btn ghost icon hide" title="Hide (D)" aria-label="Hide the dice roller" onclick={() => dice.hide()}>
        <Icon name="x" size={16} />
      </button>
    {:else}
      <div class="card panel">
        <header>
          <div class="grip" role="presentation" title="Drag to move" onpointerdown={(e) => startDrag(e, false)}>
            <Icon name="dice" />
            <span class="title">Dice</span>
          </div>
          {#if dice.rolls.length}
            <button class="btn ghost small" onclick={() => dice.clear()}>Clear</button>
          {/if}
          <button class="btn ghost icon" title="Collapse" aria-label="Collapse the dice roller" onclick={() => dice.collapse()}>
            <Icon name="chevron-right" size={16} />
          </button>
          <button class="btn ghost icon" title="Hide (D)" aria-label="Hide the dice roller" onclick={() => dice.hide()}>
            <Icon name="x" size={16} />
          </button>
        </header>

        <ol class="rolls" bind:this={list} aria-live="polite" aria-label="Recent rolls">
          {#each dice.shown as r, i (r.id)}
            <li class:newest={i === dice.shown.length - 1}>
              <div class="roll-head">
                <span class="expr">
                  {r.label ?? r.expression}
                  {#if r.tag}<span class="tag" class:bad={r.tag === 'MISS' || r.tag === 'FAILED'}>{r.tag}</span>{/if}
                </span>
                <span class="total">{r.total}</span>
              </div>
              {#if r.who || r.detail !== String(r.total)}
                <div class="detail">{[r.who, r.label ? `${r.expression}: ${r.detail}` : r.detail].filter(Boolean).join(' · ')}</div>
              {/if}
            </li>
          {:else}
            <li class="empty">Click a die or type a roll like <code>2d6+3</code>.</li>
          {/each}
        </ol>

        <div class="quick" role="group" aria-label="Quick dice">
          {#each QUICK as d (d)}
            <button class="btn" onclick={() => roll(d)}>{d}</button>
          {/each}
          <button class="btn" title="Advantage: 2d20, keep the higher" onclick={() => roll('adv')}>Adv</button>
          <button class="btn" title="Disadvantage: 2d20, keep the lower" onclick={() => roll('dis')}>Dis</button>
        </div>

        <form class="line" class:shake onsubmit={submit}>
          <input
            bind:this={input}
            bind:value={line}
            class="input"
            placeholder="2d6+3, d20-1, adv+5, d%"
            aria-label="Dice expression"
            aria-invalid={dice.error ? 'true' : undefined}
            aria-describedby={dice.error ? 'dice-err' : undefined}
            oninput={() => (dice.error = null)}
            autocomplete="off"
            spellcheck="false"
          />
          <button class="btn primary" type="submit" disabled={!line.trim()}>Roll</button>
        </form>
        {#if dice.error}<div class="err" id="dice-err" role="alert">{dice.error}</div>{/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .dice {
    position: fixed;
    z-index: 40;
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .dice.dragging {
    filter: drop-shadow(0 8px 20px rgb(0 0 0 / 0.25));
    user-select: none;
  }

  /* Compact */
  .pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 44px;
    padding: 0 var(--space-4);
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    box-shadow: var(--shadow);
    color: var(--text);
    cursor: grab;
    touch-action: none;
  }
  .pill:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .pill-expr {
    font-size: var(--fs-md);
    color: var(--text-muted);
  }
  .pill-total {
    min-width: 2ch;
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-contrast);
    font-size: var(--fs-lg);
    font-weight: 700;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .hide {
    background: var(--surface);
    box-shadow: var(--shadow);
    border-radius: 999px;
  }

  /* Expanded */
  .panel {
    width: 340px;
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .grip {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .title {
    font-weight: 600;
    font-size: var(--fs-lg);
  }
  .btn.small {
    font-size: var(--fs-sm);
  }
  .rolls {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 240px;
    overflow-y: auto;
    display: grid;
    gap: var(--space-1);
  }
  .rolls li {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  /* An inset ring: an outline would be clipped by the scrolling list. */
  .rolls li.newest {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .tag {
    margin-left: var(--space-1);
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-contrast);
    font-size: var(--fs-xs);
    font-weight: 700;
    vertical-align: middle;
  }
  .tag.bad {
    background: var(--danger);
    color: var(--surface);
  }
  .rolls li.empty {
    color: var(--text-muted);
    font-size: var(--fs-sm);
    background: transparent;
  }
  .roll-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .expr {
    font-size: var(--fs-md);
  }
  .total {
    font-size: var(--fs-xl);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .newest .total {
    font-size: 26px;
  }
  .detail {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .quick {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: var(--space-1);
  }
  .quick .btn {
    justify-content: center;
    padding-inline: 0;
  }
  .line {
    display: flex;
    gap: var(--space-2);
  }
  .line .input {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-md);
  }
  .err {
    color: var(--danger);
    font-size: var(--fs-sm);
    margin-top: calc(-1 * var(--space-2));
  }
  .shake {
    animation: shake 300ms ease;
  }
  @keyframes shake {
    20%, 60% { transform: translateX(-5px); }
    40%, 80% { transform: translateX(5px); }
  }
  @media (prefers-reduced-motion: reduce) {
    .shake {
      animation: none;
    }
  }
</style>
