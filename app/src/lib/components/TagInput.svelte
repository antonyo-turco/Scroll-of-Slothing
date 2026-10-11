<script lang="ts">
  // Tag chips with an "Add tag…" field (Enter or comma adds, × removes) and
  // suggestions from the existing tags. Used by the Library inspector and the
  // save-to-library dialog.
  let {
    values,
    suggestions = [],
    onchange,
    label = 'Add tag',
  }: {
    values: string[];
    suggestions?: string[];
    onchange: (values: string[]) => void;
    label?: string;
  } = $props();

  let input = $state('');
  const uid = $props.id();
  const listId = `tag-suggestions-${uid}`;

  function add() {
    const t = input.trim().replace(/,$/, '').trim();
    input = '';
    if (t && !values.some((x) => x.toLowerCase() === t.toLowerCase())) onchange([...values, t]);
  }
</script>

<div class="chips">
  {#each values as t (t)}
    <span class="chip">{t}<button type="button" aria-label="Remove tag {t}" onclick={() => onchange(values.filter((x) => x !== t))}>×</button></span>
  {/each}
  <input
    class="chip-input"
    placeholder="Add tag…"
    aria-label={label}
    list={listId}
    bind:value={input}
    onkeydown={(e) => {
      if (e.key === 'Enter' || e.key === ',') {
        e.preventDefault();
        e.stopPropagation();
        add();
      }
    }}
    onblur={add}
  />
  <datalist id={listId}>
    {#each suggestions.filter((s) => !values.includes(s)) as s (s)}<option value={s}></option>{/each}
  </datalist>
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    min-height: 32px;
    padding: 3px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .chips:focus-within {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 4px 2px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    font-size: 12px;
  }
  .chip button {
    border: 0;
    background: none;
    cursor: pointer;
    padding: 0 4px;
    color: var(--text-muted);
  }
  .chip-input {
    flex: 1;
    min-width: 80px;
    border: 0;
    background: transparent;
    padding: 2px 4px;
  }
  .chip-input:focus {
    outline: none;
  }
</style>
