// Send a fight's creatures to the token sheet (Tokens → Sheet), one token per copy,
// at each creature's size. Creatures without a token on the mirror are reported.
import { api } from '../../lib/api';
import { app } from '../../lib/state.svelte';

export type TokenLine = { hash: string; name: string; count: number };

/** Adds the lines to the token sheet and opens it. */
export async function printTokens(lines: TokenLine[]): Promise<void> {
  // same creature on several lines (e.g. scaled copies): one sheet item
  const by = new Map<string, TokenLine>();
  for (const l of lines) {
    if (l.count < 1) continue;
    const x = by.get(l.hash);
    by.set(l.hash, x ? { ...x, count: x.count + l.count } : { ...l });
  }
  if (!by.size) return app.toast('info', 'No creatures to print.');
  const missing: string[] = [];
  let added = 0;
  // one call per creature, so one missing token doesn't block the rest
  for (const l of by.values()) {
    try {
      await api.tokenSheetAddCreatures([{ hash: l.hash, count: Math.min(99, l.count) }]);
      added += l.count;
    } catch {
      missing.push(l.name);
    }
  }
  if (missing.length) app.toast('error', `No token for: ${missing.join(', ')}`, added ? `${added} token(s) were added.` : undefined);
  else app.toast('success', `${added} token(s) added to the sheet`);
  if (added) app.navigate('tokens.sheet');
}
