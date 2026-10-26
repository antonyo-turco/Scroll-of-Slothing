<script lang="ts">
  // Keeps timed clocks advancing on every route (see lib/clocks.svelte.ts).
  import { onMount } from 'svelte';
  import { clocks } from '../clocks.svelte';
  import { app } from '../state.svelte';

  onMount(() => {
    void clocks.refresh();
    return () => clocks.dispose();
  });
  // Changes from the MCP server or another view re-plan the timer.
  let seen = app.dataVersion;
  $effect(() => {
    if (app.dataVersion !== seen) {
      seen = app.dataVersion;
      void clocks.refresh();
    }
  });
</script>
