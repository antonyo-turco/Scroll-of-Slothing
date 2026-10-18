<script lang="ts">
  import { DESTINATIONS, SETTINGS_DEST, findView, type Destination } from '../nav';
  import { app } from '../state.svelte';
  import { musicUi } from '../music-ui.svelte';
  import Icon from './Icon.svelte';
  import PlayIndicator from './PlayIndicator.svelte';

  const current = $derived(findView(app.route)?.dest.id);

  function go(d: Destination) {
    if (d.disabled || d.views.length === 0) return;
    app.navigate(d.views[0].id);
  }
</script>

<nav class="rail" class:collapsed={app.sidebarCollapsed} aria-label="Main">
  <div class="brand">
    {#if !app.sidebarCollapsed}
      <span class="title">Scroll of Sleuthing</span>
    {/if}
    <button
      class="btn ghost icon"
      onclick={() => app.toggleSidebar()}
      aria-label={app.sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      title={app.sidebarCollapsed ? 'Expand' : 'Collapse'}
    >
      <Icon name={app.sidebarCollapsed ? 'chevron-right' : 'chevron-left'} />
    </button>
  </div>

  <ul>
    {#each DESTINATIONS as d (d.id)}
      <li class:group-start={d.groupStart}>
        <button
          class="item"
          class:active={current === d.id}
          disabled={d.disabled}
          aria-current={current === d.id ? 'page' : undefined}
          title={d.hint ?? d.label}
          onclick={() => go(d)}
        >
          <Icon name={d.icon} />
          {#if !app.sidebarCollapsed}
            <span>{d.label}</span>
            {#if d.disabled}<span class="badge">planned</span>{/if}
          {/if}
          {#if d.id === 'music' && musicUi.activity}
            <span class="rail-pi"><PlayIndicator status={musicUi.activity} size={12} /></span>
          {/if}
        </button>
      </li>
    {/each}
  </ul>

  <div class="bottom">
    <button
      class="item"
      class:active={current === SETTINGS_DEST.id}
      aria-current={current === SETTINGS_DEST.id ? 'page' : undefined}
      title="Settings"
      onclick={() => go(SETTINGS_DEST)}
    >
      <Icon name="settings" />
      {#if !app.sidebarCollapsed}<span>Settings</span>{/if}
    </button>
  </div>
</nav>

<style>
  .rail {
    width: var(--rail-w);
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-right: 1px solid var(--border);
    transition: width 120ms ease;
  }
  .rail.collapsed {
    width: var(--rail-w-collapsed);
  }
  .brand {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--topbar-h);
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .collapsed .brand {
    justify-content: center;
    padding: 0;
  }
  .title {
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.01em;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: var(--space-2);
    display: grid;
    gap: 2px;
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 36px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    text-align: left;
  }
  .collapsed .item {
    justify-content: center;
    padding: 0;
  }
  .item:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }
  .item.active {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
  }
  .item:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .item {
    position: relative;
  }
  .rail-pi {
    margin-left: auto;
    display: inline-flex;
  }
  .collapsed .rail-pi {
    position: absolute;
    top: 3px;
    right: 6px;
  }
  /* Separator above a new group of destinations (print tools | audio tools). */
  .group-start {
    margin-top: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  .badge {
    margin-left: auto;
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--border);
  }
  .bottom {
    margin-top: auto;
    padding: var(--space-2);
    border-top: 1px solid var(--border);
  }
</style>
