<script lang="ts">
  // Small playback status glyph (Windows Media Player style):
  // - playing: four bouncing equalizer bars;
  // - loading: a travelling wave of dots (loading or buffering);
  // - paused: four still, dimmed bars.
  // Reduced motion keeps the three shapes distinct without animating them.
  import type { PlayStatus } from '../music-engine';

  let {
    status,
    size = 14,
    inherit = false,
  }: { status: PlayStatus | null | undefined; size?: number; /** use the text colour */ inherit?: boolean } = $props();

  const LABELS: Record<PlayStatus, string> = { loading: 'Loading…', playing: 'Playing', paused: 'Paused' };
</script>

{#if status}
  <span
    class="pi {status}"
    role="img"
    aria-label={LABELS[status]}
    title={LABELS[status]}
    style:--pi-size="{size}px"
    style:color={inherit ? 'inherit' : undefined}
    data-status={status}
  >
    <i></i><i></i><i></i><i></i>
  </span>
{/if}

<style>
  .pi {
    --pi-bar: calc(var(--pi-size) / 5);
    display: inline-flex;
    align-items: flex-end;
    justify-content: center;
    gap: calc(var(--pi-bar) / 2);
    width: var(--pi-size);
    height: var(--pi-size);
    flex: none;
    color: var(--accent);
    vertical-align: middle;
  }
  i {
    display: block;
    width: var(--pi-bar);
    height: 100%;
    border-radius: 1px;
    background: currentColor;
    transform-origin: bottom;
  }

  /* playing: equalizer */
  .playing i {
    animation: pi-eq 0.9s ease-in-out infinite alternate;
  }
  .playing i:nth-child(1) {
    animation-duration: 0.8s;
  }
  .playing i:nth-child(2) {
    animation-duration: 1.05s;
    animation-delay: -0.4s;
  }
  .playing i:nth-child(3) {
    animation-duration: 0.7s;
    animation-delay: -0.2s;
  }
  .playing i:nth-child(4) {
    animation-duration: 0.95s;
    animation-delay: -0.6s;
  }
  @keyframes pi-eq {
    0% {
      transform: scaleY(0.2);
    }
    50% {
      transform: scaleY(1);
    }
    100% {
      transform: scaleY(0.45);
    }
  }

  /* loading: a wave of dots travelling left to right */
  .loading {
    align-items: center;
    color: var(--text-muted);
  }
  .loading i {
    height: var(--pi-bar);
    border-radius: 50%;
    animation: pi-wave 1s ease-in-out infinite;
  }
  .loading i:nth-child(2) {
    animation-delay: 0.15s;
  }
  .loading i:nth-child(3) {
    animation-delay: 0.3s;
  }
  .loading i:nth-child(4) {
    animation-delay: 0.45s;
  }
  @keyframes pi-wave {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.35;
    }
    30% {
      transform: translateY(calc(var(--pi-size) / -3));
      opacity: 1;
    }
  }

  /* paused: still bars */
  .paused {
    color: var(--text-muted);
    opacity: 0.7;
  }
  .paused i:nth-child(1) {
    transform: scaleY(0.45);
  }
  .paused i:nth-child(2) {
    transform: scaleY(0.8);
  }
  .paused i:nth-child(3) {
    transform: scaleY(0.3);
  }
  .paused i:nth-child(4) {
    transform: scaleY(0.6);
  }

  @media (prefers-reduced-motion: reduce) {
    .pi i {
      animation: none !important;
    }
    .playing i:nth-child(odd) {
      transform: scaleY(0.6);
    }
    .loading i {
      opacity: 0.6;
    }
  }
</style>
