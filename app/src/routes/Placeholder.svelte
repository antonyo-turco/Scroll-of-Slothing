<script lang="ts">
  import { findView } from '../lib/nav';
  import { app } from '../lib/state.svelte';

  const found = $derived(findView(app.route));
</script>

<section class="placeholder">
  <div class="card inner">
    <h2>{found?.view.label}</h2>
    <p class="muted">
      This view arrives in <strong>Phase {found?.view.phase}</strong> of the rewrite. Its full specification is in
      <code>docs/features/</code>.
    </p>
  </div>
</section>

<style>
  .placeholder {
    display: grid;
    place-items: center;
    height: 100%;
    padding: var(--space-6);
  }
  .inner {
    max-width: 440px;
    padding: var(--space-6);
    text-align: center;
  }
  h2 {
    margin: 0 0 var(--space-2);
    font-size: 18px;
  }
  code {
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
