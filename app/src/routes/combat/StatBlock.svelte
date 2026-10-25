<script lang="ts">
  // Stat block of the combatant whose turn it is: a 5etools creature from the
  // bestiary cache, or a hero from the saved parties.
  import type { Hero } from '../../lib/types';

  type Mon = Record<string, unknown>;
  let { mon, hero, name, sub }: { mon: Mon | null; hero: Hero | null; name: string; sub?: string } = $props();

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

  /** Flattens 5etools entries (strings, {entries}, lists) into paragraphs. */
  function paras(entries: unknown): string[] {
    if (!Array.isArray(entries)) return typeof entries === 'string' ? [plain(entries)] : [];
    const out: string[] = [];
    for (const e of entries) {
      if (typeof e === 'string') out.push(plain(e));
      else if (e && typeof e === 'object') {
        const o = e as Record<string, unknown>;
        if (Array.isArray(o.items)) for (const it of o.items) out.push(`• ${typeof it === 'string' ? plain(it) : [str((it as Record<string, unknown>).name), ...paras((it as Record<string, unknown>).entries ?? (it as Record<string, unknown>).entry)].join(' ')}`);
        else out.push(...paras(o.entries));
      }
    }
    return out;
  }
  const blocks = (key: string): { name: string; text: string[] }[] => {
    const v = mon?.[key];
    if (!Array.isArray(v)) return [];
    return v.map((b: Record<string, unknown>) => ({ name: plain(String(b.name ?? '')), text: paras(b.entries ?? b.headerEntries) }));
  };
  const spellcasting = () => {
    const v = mon?.spellcasting;
    if (!Array.isArray(v)) return [];
    return v.map((sc: Record<string, unknown>) => {
      const lines = paras(sc.headerEntries);
      const will = sc.will ? [`At will: ${str(sc.will)}`] : [];
      const daily = sc.daily ? Object.entries(sc.daily as Record<string, unknown>).map(([k, x]) => `${k.replace('e', '/day each').replace(/^(\d)$/, '$1/day')}: ${str(x)}`) : [];
      const spells = sc.spells ? Object.entries(sc.spells as Record<string, Record<string, unknown>>).map(([lvl, x]) => `${lvl === '0' ? 'Cantrips' : `Level ${lvl}`}${x.slots ? ` (${x.slots} slots)` : ''}: ${str(x.spells)}`) : [];
      return { name: plain(String(sc.name ?? 'Spellcasting')), text: [...lines, ...will, ...daily, ...spells] };
    });
  };
  const SECTIONS: [string, string][] = [['action', 'Actions'], ['bonus', 'Bonus actions'], ['reaction', 'Reactions'], ['legendary', 'Legendary actions'], ['mythic', 'Mythic actions']];
</script>

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
    {#each [...blocks('trait'), ...spellcasting()] as b, i (i)}
      <p class="entry"><b>{b.name}.</b>{' '}{b.text.join(' ')}</p>
    {/each}
    {#each SECTIONS as [key, title] (key)}
      {#if blocks(key).length}
        <h4>{title}</h4>
        {#each blocks(key) as b, i (i)}<p class="entry">{#if b.name}<b>{b.name}.</b>{' '}{/if}{b.text.join(' ')}</p>{/each}
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
</style>
