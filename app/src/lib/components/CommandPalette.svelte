<script lang="ts">
  import { ALL_VIEWS, findView } from '../nav';
  import { app } from '../state.svelte';

  interface Command {
    id: string;
    label: string;
    run: () => void;
  }

  let query = $state('');
  let selected = $state(0);
  let input: HTMLInputElement | undefined = $state();

  const commands: Command[] = [
    ...ALL_VIEWS.map((v) => {
      const d = findView(v.id)!.dest;
      const label = d.views.length > 1 ? `Go to ${d.label} › ${v.label}` : `Go to ${d.label}`;
      return { id: `go:${v.id}`, label, run: () => app.navigate(v.id) };
    }),
    { id: 'toggle-sidebar', label: 'Toggle sidebar', run: () => app.toggleSidebar() },
    { id: 'theme-light', label: 'Theme: light', run: () => void app.updateSettings({ theme: 'light' }) },
    { id: 'theme-dark', label: 'Theme: dark', run: () => void app.updateSettings({ theme: 'dark' }) },
    { id: 'theme-system', label: 'Theme: follow system', run: () => void app.updateSettings({ theme: 'system' }) },
  ];

  const results = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    const words = q.split(/\s+/);
    return commands.filter((c) => words.every((w) => c.label.toLowerCase().includes(w)));
  });

  $effect(() => {
    if (app.paletteOpen) {
      query = '';
      selected = 0;
      queueMicrotask(() => input?.focus());
    }
  });

  function close() {
    app.paletteOpen = false;
  }

  function exec(c: Command | undefined) {
    if (!c) return;
    close();
    c.run();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowDown') {
      selected = Math.min(selected + 1, results.length - 1);
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      selected = Math.max(selected - 1, 0);
      e.preventDefault();
    } else if (e.key === 'Enter') exec(results[selected]);
  }
</script>

{#if app.paletteOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={close}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="palette card" role="dialog" tabindex="-1" aria-modal="true" aria-label="Command palette" onclick={(e) => e.stopPropagation()}>
      <input
        bind:this={input}
        bind:value={query}
        oninput={() => (selected = 0)}
        onkeydown={onKey}
        class="input"
        placeholder="Type a command or view…"
        aria-label="Command"
      />
      <ul role="listbox">
        {#each results as c, i (c.id)}
          <li>
            <button
              role="option"
              aria-selected={i === selected}
              class:sel={i === selected}
              onmouseenter={() => (selected = i)}
              onclick={() => exec(c)}>{c.label}</button
            >
          </li>
        {:else}
          <li class="muted empty">No matching commands</li>
        {/each}
      </ul>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.25);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 50;
  }
  .palette {
    width: min(560px, 92vw);
    box-shadow: var(--shadow);
    padding: var(--space-2);
  }
  .input {
    width: 100%;
    min-height: 40px;
    font-size: 15px;
  }
  ul {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    max-height: 50vh;
    overflow: auto;
  }
  li button {
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  li button.sel {
    background: var(--accent-soft);
  }
  .empty {
    padding: var(--space-3);
  }
</style>
