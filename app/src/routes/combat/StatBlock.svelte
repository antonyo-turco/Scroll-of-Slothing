<script lang="ts">
  // Stat block of the combatant whose turn it is: a 5etools creature from the
  // bestiary cache, or a hero from the saved parties. With `onRoll`, to-hit and
  // damage tags are roll buttons; with `onUse`, limited abilities get pips.
  import type { Hero } from '../../lib/types';

  type Mon = Record<string, unknown>;
  export type RollRequest = { entry: string; kind: 'hit' | 'damage'; value: string; shift: boolean; ctrl: boolean };
  let {
    mon,
    hero,
    name,
    sub,
    usage = null,
    onRoll,
    onUse,
    lair = null,
  }: {
    mon: Mon | null;
    hero: Hero | null;
    name: string;
    sub?: string;
    /** Spent uses by ability name, `__la` legendary actions spent. */
    usage?: Record<string, unknown> | null;
    onRoll?: (r: RollRequest) => void;
    onUse?: (key: string, spent: number) => void;
    /** A legendary group (lair actions, regional effects) to show instead of a creature. */
    lair?: Mon | null;
  } = $props();

  // ── roll buttons: text and roll segments of a 5etools string ──
  type Seg = { text: string } | { roll: 'hit' | 'damage'; value: string; label: string };
  function segs(raw: string): Seg[] {
    const out: Seg[] = [];
    const re = /\{@(\w+)\s*([^}]*)\}/g;
    let last = 0;
    for (let m = re.exec(raw); m; m = re.exec(raw)) {
      if (m.index > last) out.push({ text: raw.slice(last, m.index) });
      const tag = m[1];
      const body = m[2].split('|')[0].trim();
      if (onRoll && tag === 'hit') out.push({ roll: 'hit', value: body, label: Number(body) >= 0 ? `+${body}` : body });
      else if (onRoll && (tag === 'damage' || tag === 'dice')) out.push({ roll: 'damage', value: body, label: body });
      else if (onRoll && tag === 'd20') out.push({ roll: 'hit', value: body, label: Number(body) >= 0 ? `+${body}` : body });
      else out.push({ text: plain(m[0]) });
      last = m.index + m[0].length;
    }
    if (last < raw.length) out.push({ text: raw.slice(last) });
    return out;
  }

  // ── limited abilities: "(3/Day)" uses and "{@recharge 5}" ──
  const usesOf = (raw: string) => {
    const m = plain(raw).match(/\((\d+)\/Day(?: each)?\)/i);
    return m ? Number(m[1]) : 0;
  };
  const rechargeOf = (raw: string) => {
    const m = raw.match(/\{@recharge\s*(\d*)\}/) ?? plain(raw).match(/\(Recharge (\d)/i);
    return m ? Number(m[1]) || 6 : 0;
  };
  const spent = (key: string) => Number(usage?.[key] ?? 0) || 0;
  /** The ability name without its "(3/Day)" or "(Recharge 5–6)" note. */
  const bare = (raw: string) => plain(raw).replace(/\s*\((?:\d+\/Day[^)]*|Recharge[^)]*)\)/gi, '').trim();
  const laMax = $derived(mon ? Number(mon.legendaryActions ?? (Array.isArray(mon.legendary) && mon.legendary.length ? 3 : 0)) || 0 : 0);

  /** `{@hit 5}` → `+5`, `{@damage 2d6 + 3}` → `2d6 + 3`, `{@spell fire bolt|phb}` → `fire bolt`. */
  function plain(s: string): string {
    return s.replace(/\{@(\w+)\s*([^}]*)\}/g, (_, tag: string, body: string) => {
      const text = body.split('|')[0];
      if (tag === 'hit') return Number(text) >= 0 ? `+${text}` : text;
      if (tag === 'h') return 'Hit: ';
      if (tag === 'atk' || tag === 'atkr') return ({ mw: 'Melee Weapon Attack:', rw: 'Ranged Weapon Attack:', 'mw,rw': 'Melee or Ranged Weapon Attack:', ms: 'Melee Spell Attack:', rs: 'Ranged Spell Attack:', m: 'Melee Attack Roll:', r: 'Ranged Attack Roll:' } as Record<string, string>)[text] ?? '';
      if (tag === 'recharge') return `(Recharge ${text || 6}${text && text !== '6' ? '–6' : ''})`;
      if (tag === 'dc') return `DC ${text}`;
      return text;
    });
  }

  const SIZE: Record<string, string> = { T: 'Tiny', S: 'Small', M: 'Medium', L: 'Large', H: 'Huge', G: 'Gargantuan' };
  const ABIL = ['str', 'dex', 'con', 'int', 'wis', 'cha'] as const;
  const mod = (n: number) => {
    const m = Math.floor((n - 10) / 2);
    return m >= 0 ? `+${m}` : `${m}`;
  };
  const str = (v: unknown): string => {
    if (v == null) return '';
    if (typeof v === 'string') return plain(v);
    if (typeof v === 'number') return String(v);
    if (Array.isArray(v)) return v.map(str).filter(Boolean).join(', ');
    if (typeof v === 'object') {
      const o = v as Record<string, unknown>;
      if ('ac' in o) return `${o.ac}${o.from ? ` (${str(o.from)})` : ''}${o.condition ? ` ${str(o.condition)}` : ''}`;
      if ('special' in o) return str(o.special);
      if ('cr' in o) return str(o.cr);
      if ('type' in o && typeof o.type === 'string') {
        const tags = Array.isArray(o.tags) ? ` (${(o.tags as unknown[]).map((t) => (typeof t === 'string' ? t : str((t as Record<string, unknown>).tag))).join(', ')})` : '';
        return `${o.type}${tags}`;
      }
    }
    return '';
  };
  const speed = (v: unknown) => {
    if (!v || typeof v !== 'object') return str(v);
    return Object.entries(v as Record<string, unknown>)
      .filter(([k]) => k !== 'canHover')
      .map(([k, x]) => {
        const n = typeof x === 'object' && x ? (x as Record<string, unknown>).number : x;
        return `${k === 'walk' ? '' : `${k} `}${n} ft.`;
      })
      .join(', ');
  };
  const kv = (v: unknown) =>
    v && typeof v === 'object' && !Array.isArray(v)
      ? Object.entries(v as Record<string, unknown>).map(([k, x]) => `${k[0].toUpperCase()}${k.slice(1)} ${str(x)}`).join(', ')
      : str(v);

  /** Flattens 5etools entries (strings, {entries}, lists) into paragraphs, tags kept. */
  function paras(entries: unknown): string[] {
    if (!Array.isArray(entries)) return typeof entries === 'string' ? [entries] : [];
    const out: string[] = [];
    for (const e of entries) {
      if (typeof e === 'string') out.push(e);
      else if (e && typeof e === 'object') {
        const o = e as Record<string, unknown>;
        if (Array.isArray(o.items)) for (const it of o.items) out.push(`• ${typeof it === 'string' ? it : [String((it as Record<string, unknown>).name ?? ''), ...paras((it as Record<string, unknown>).entries ?? (it as Record<string, unknown>).entry)].join(' ')}`);
        else out.push(...paras(o.entries));
      }
    }
    return out;
  }
  type Block = { key: string; name: string; uses: number; recharge: number; text: string[] };
  const blocks = (key: string, src: Mon | null = mon): Block[] => {
    const v = src?.[key];
    if (!Array.isArray(v)) return [];
    return v.map((b: Record<string, unknown>) => {
      const raw = String(b.name ?? '');
      return { key: bare(raw), name: bare(raw), uses: usesOf(raw), recharge: rechargeOf(raw), text: paras(b.entries ?? b.headerEntries) };
    });
  };
  const spellcasting = () => {
    const v = mon?.spellcasting;
    if (!Array.isArray(v)) return [];
    return v.map((sc: Record<string, unknown>) => {
      const lines = paras(sc.headerEntries);
      const will = sc.will ? [`At will: ${str(sc.will)}`] : [];
      const daily = sc.daily ? Object.entries(sc.daily as Record<string, unknown>).map(([k, x]) => `${k.replace('e', '/day each').replace(/^(\d)$/, '$1/day')}: ${str(x)}`) : [];
      const spells = sc.spells ? Object.entries(sc.spells as Record<string, Record<string, unknown>>).map(([lvl, x]) => `${lvl === '0' ? 'Cantrips' : `Level ${lvl}`}${x.slots ? ` (${x.slots} slots)` : ''}: ${str(x.spells)}`) : [];
      const n = bare(String(sc.name ?? 'Spellcasting'));
      return { key: n, name: n, uses: 0, recharge: 0, text: [...lines, ...will, ...daily, ...spells] };
    });
  };
  /** The ability the pointer is in, so a crit carries over to its damage roll. */
  let currentEntry = $state('');
  const SECTIONS: [string, string][] = [['action', 'Actions'], ['bonus', 'Bonus actions'], ['reaction', 'Reactions'], ['legendary', 'Legendary actions'], ['mythic', 'Mythic actions']];
</script>

{#snippet text(line: string)}
  {#each segs(line) as sg, i (i)}{#if 'roll' in sg}<button class="roll {sg.roll}" title={sg.roll === 'hit' ? 'Roll to hit (Shift: advantage, Ctrl: disadvantage)' : 'Roll damage (Shift: critical)'}
      onclick={(e) => onRoll?.({ entry: currentEntry, kind: sg.roll, value: sg.value, shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })}>{sg.label}</button>{:else}{plain(sg.text)}{/if}{/each}
{/snippet}

{#snippet entry(b: Block)}
  <p class="entry" onpointerenter={() => (currentEntry = b.key)} onfocusin={() => (currentEntry = b.key)}>
    <b>{b.name}.</b>
    {#if b.recharge && onUse}
      <button class="recharge" class:used={spent(b.key) > 0} onclick={() => onUse!(b.key, spent(b.key) ? 0 : 1)}
        title={spent(b.key) ? 'Spent: rolls to recharge at the start of its turn (click to recharge now)' : 'Available (click when used)'}>
        Recharge {b.recharge}{b.recharge < 6 ? '–6' : ''} {spent(b.key) ? '✗' : '✓'}
      </button>
    {:else if b.recharge}<span class="muted">(Recharge {b.recharge}{b.recharge < 6 ? '–6' : ''})</span>{/if}
    {#if b.uses && onUse}
      <span class="pips" role="group" aria-label="{b.name}: {b.uses - spent(b.key)} of {b.uses} uses left today">
        {#each Array(b.uses) as _, i (i)}
          <button class="pip" class:used={i >= b.uses - spent(b.key)} aria-label="Use {i + 1}"
            onclick={() => onUse!(b.key, i >= b.uses - spent(b.key) ? spent(b.key) - 1 : spent(b.key) + 1)}></button>
        {/each}
        <span class="muted">{b.uses}/day</span>
      </span>
    {:else if b.uses}<span class="muted">({b.uses}/Day)</span>{/if}
    {' '}{#each b.text as line, i (i)}{@render text(line)}{' '}{/each}
  </p>
{/snippet}

<div class="sb" data-testid="stat-block">
  <h3 class="title">{name}</h3>
  {#if mon}
    <p class="meta">{[(mon.size as string[] | undefined)?.map((s) => SIZE[s] ?? s).join(' or '), str(mon.type)].filter(Boolean).join(' ')}{mon.alignment ? `, ${(mon.alignment as string[]).join(' ')}` : ''}</p>
    <dl class="props">
      <dt>AC</dt><dd>{str(mon.ac)}</dd>
      <dt>HP</dt><dd>{(mon.hp as Record<string, unknown>)?.average ?? str((mon.hp as Record<string, unknown>)?.special)}{(mon.hp as Record<string, unknown>)?.formula ? ` (${(mon.hp as Record<string, unknown>).formula})` : ''}</dd>
      <dt>Speed</dt><dd>{speed(mon.speed)}</dd>
    </dl>
    <div class="abil">
      {#each ABIL as a (a)}
        <div><span>{a.toUpperCase()}</span><b>{mon[a] ?? '—'}</b><small>{typeof mon[a] === 'number' ? mod(mon[a] as number) : ''}</small></div>
      {/each}
    </div>
    <dl class="props">
      {#if mon.save}<dt>Saves</dt><dd>{kv(mon.save)}</dd>{/if}
      {#if mon.skill}<dt>Skills</dt><dd>{kv(mon.skill)}</dd>{/if}
      {#if mon.vulnerable}<dt>Vulnerable</dt><dd>{str(mon.vulnerable)}</dd>{/if}
      {#if mon.resist}<dt>Resist</dt><dd>{str(mon.resist)}</dd>{/if}
      {#if mon.immune}<dt>Immune</dt><dd>{str(mon.immune)}</dd>{/if}
      {#if mon.conditionImmune}<dt>Cond. imm.</dt><dd>{str(mon.conditionImmune)}</dd>{/if}
      <dt>Senses</dt><dd>{[str(mon.senses), mon.passive != null ? `passive Perception ${mon.passive}` : ''].filter(Boolean).join(', ')}</dd>
      {#if mon.languages}<dt>Languages</dt><dd>{str(mon.languages)}</dd>{/if}
      <dt>CR</dt><dd>{str(mon.cr) || '—'}</dd>
    </dl>
    {#each [...blocks('trait'), ...spellcasting()] as b, i (i)}{@render entry(b)}{/each}
    {#each SECTIONS as [key, title] (key)}
      {#if blocks(key).length}
        <h4>
          {title}
          {#if key === 'legendary' && laMax && onUse}
            <span class="pips" role="group" aria-label="Legendary actions left">
              {#each Array(laMax) as _, i (i)}
                <button class="pip" class:used={i >= laMax - spent('__la')} aria-label="Legendary action {i + 1}"
                  onclick={() => onUse!('__la', i >= laMax - spent('__la') ? spent('__la') - 1 : spent('__la') + 1)}></button>
              {/each}
            </span>
          {/if}
        </h4>
        {#each blocks(key) as b, i (i)}{@render entry(b)}{/each}
      {/if}
    {/each}
  {:else if lair}
    <p class="meta">Lair · initiative 20 (loses ties)</p>
    {#each [['lairActions', 'Lair actions'], ['regionalEffects', 'Regional effects']] as [k, title] (k)}
      {#if Array.isArray(lair[k])}
        <h4>{title}</h4>
        {#each paras(lair[k]) as line, i (i)}<p class="entry">{@render text(line)}</p>{/each}
      {/if}
    {/each}
  {:else if hero}
    {#if sub}<p class="meta">{sub}</p>{/if}
    <dl class="props">
      {#if hero.player}<dt>Player</dt><dd>{hero.player}</dd>{/if}
      <dt>AC</dt><dd>{hero.ac ?? '—'}</dd>
      <dt>HP</dt><dd>{hero.hp_max ?? '—'}</dd>
      <dt>Initiative</dt><dd>{hero.initiative_bonus != null ? (hero.initiative_bonus >= 0 ? `+${hero.initiative_bonus}` : hero.initiative_bonus) : '—'}</dd>
      {#if hero.passive_perception != null}<dt>Passive Perc.</dt><dd>{hero.passive_perception}</dd>{/if}
      {#if hero.speed != null}<dt>Speed</dt><dd>{hero.speed} ft.</dd>{/if}
      {#if hero.spell_dc != null}<dt>Spell DC</dt><dd>{hero.spell_dc}</dd>{/if}
    </dl>
    {#if hero.notes}<p class="entry">{hero.notes}</p>{/if}
  {:else}
    {#if sub}<p class="meta">{sub}</p>{/if}
    <p class="muted">No stat block: a player not in a saved party, or a creature missing from the bestiary cache.</p>
  {/if}
</div>

<style>
  .sb {
    display: grid;
    gap: var(--space-3);
    font-size: 15px;
    line-height: 1.55;
  }
  .title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    color: #b0331f;
  }
  :global([data-theme='dark']) .title,
  :global(.dark) .title {
    color: #e0765f;
  }
  .meta {
    margin: 0;
    font-style: italic;
    color: var(--text-muted);
    font-size: 14px;
  }
  .props {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  .props dt {
    font-weight: 600;
  }
  .props dd {
    margin: 0;
  }
  .abil {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    text-align: center;
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  .abil div {
    display: grid;
  }
  .abil b {
    font-size: 17px;
  }
  .abil span {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .abil small {
    color: var(--text-muted);
  }
  h4 {
    margin: var(--space-3) 0 0;
    padding-bottom: 2px;
    border-bottom: 1px solid var(--border);
    font-size: 16px;
    font-variant: small-caps;
    font-weight: 700;
    color: #b0331f;
  }
  .entry {
    margin: 0;
  }
  .roll {
    display: inline;
    padding: 0 5px;
    margin: 0 1px;
    border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border));
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--text);
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
  .roll.damage {
    border-color: color-mix(in srgb, #d9381e 55%, var(--border));
    background: color-mix(in srgb, #d9381e 14%, transparent);
  }
  .roll:hover {
    filter: brightness(1.25);
  }
  .pips {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 8px;
    vertical-align: middle;
    font-size: 12px;
    font-variant: normal;
    font-weight: 400;
  }
  .pip {
    width: 14px;
    height: 14px;
    padding: 0;
    border: 2px solid var(--accent);
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
  }
  .pip.used {
    background: transparent;
    border-color: var(--text-muted);
  }
  .recharge {
    margin-left: 6px;
    padding: 1px 8px;
    border: 1px solid #2fb344;
    border-radius: 999px;
    background: color-mix(in srgb, #2fb344 14%, transparent);
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
  }
  .recharge.used {
    border-color: var(--text-muted);
    background: transparent;
    color: var(--text-muted);
  }
</style>
