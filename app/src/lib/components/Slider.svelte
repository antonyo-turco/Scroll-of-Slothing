<script lang="ts">
  // Inspector slider: label + value on top, − / range / + below so it fits the
  // inspector at any value width. Holding − or + repeats the step.
  let {
    label,
    value = $bindable(),
    min,
    max,
    step = 1,
    oninput,
  }: {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    oninput?: (v: number) => void;
  } = $props();

  const decimals = $derived((String(step).split('.')[1] ?? '').length);

  /** Snaps and clamps; false when the value did not change (at a limit). */
  function set(v: number): boolean {
    const snapped = min + Math.round((v - min) / step) * step;
    const next = Number(Math.min(max, Math.max(min, snapped)).toFixed(decimals));
    if (next === value) return false;
    value = next;
    oninput?.(next);
    return true;
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function stop() {
    clearTimeout(timer);
    timer = undefined;
  }
  $effect(() => stop);

  function hold(dir: number) {
    stop();
    if (!set(value + dir * step)) return;
    // a button disabled at the limit gets no pointerup, so stop there too
    const again = (delay: number) => {
      timer = setTimeout(() => (set(value + dir * step) ? again(60) : stop()), delay);
    };
    again(400);
  }
</script>

<div class="slider">
  <div class="top">
    <span class="lbl">{label}</span>
    <span class="val">{value}</span>
  </div>
  <div class="ctl">
    <button
      class="btn icon"
      aria-label="Decrease {label}"
      disabled={value <= min}
      onpointerdown={(e) => e.button === 0 && hold(-1)}
      onpointerup={stop}
      onpointerleave={stop}
      onpointercancel={stop}
      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), set(value - step))}>−</button
    >
    <input
      type="range"
      aria-label={label}
      {min}
      {max}
      {step}
      {value}
      oninput={(e) => set(Number(e.currentTarget.value))}
    />
    <button
      class="btn icon"
      aria-label="Increase {label}"
      disabled={value >= max}
      onpointerdown={(e) => e.button === 0 && hold(1)}
      onpointerup={stop}
      onpointerleave={stop}
      onpointercancel={stop}
      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), set(value + step))}>+</button
    >
  </div>
</div>

<style>
  .slider {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .top {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    font-size: 12px;
  }
  .lbl {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .val {
    font-variant-numeric: tabular-nums;
  }
  .ctl {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) 28px;
    align-items: center;
    gap: var(--space-2);
  }
  .ctl .btn {
    width: 28px;
    min-height: 28px;
    font-size: 16px;
    line-height: 1;
    touch-action: manipulation;
  }
  input {
    width: 100%;
    min-width: 0;
    margin: 0;
    accent-color: var(--accent);
  }
</style>
