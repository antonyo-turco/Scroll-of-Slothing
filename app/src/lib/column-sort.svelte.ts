// Sortable list columns (docs/styleguide.md, "Lists"): clicking a column header
// sorts by it, clicking it again flips the direction. Header buttons use
// `.list-head .sort`, with `aria-sort` and an ▲/▼ arrow on the active column.

export type Compare<T> = (a: T, b: T) => number;

export class ColumnSort<K extends string> {
  key = $state<K>() as K;
  asc = $state(true);

  /**
   * @param key the column sorted by at first
   * @param ascFirst columns that start ascending (A–Z); the others start descending
   *   (biggest, newest first)
   */
  constructor(
    key: K,
    asc: boolean,
    private ascFirst: K[] = [],
  ) {
    this.key = key;
    this.asc = asc;
  }

  /** Header click: sort by this column, or flip it if it is already the one. */
  by(key: K): void {
    if (this.key === key) this.asc = !this.asc;
    else {
      this.key = key;
      this.asc = this.ascFirst.includes(key);
    }
  }

  aria(key: K): 'ascending' | 'descending' | 'none' {
    return this.key === key ? (this.asc ? 'ascending' : 'descending') : 'none';
  }

  arrow(key: K): string {
    return this.key === key ? (this.asc ? ' ▲' : ' ▼') : '';
  }

  /** A sorted copy. `tie` breaks ties in a fixed order, whatever the direction. */
  apply<T>(items: T[], by: Record<K, Compare<T>>, tie?: Compare<T>): T[] {
    const dir = this.asc ? 1 : -1;
    const cmp = by[this.key];
    return [...items].sort((a, b) => dir * cmp(a, b) || (tie ? tie(a, b) : 0));
  }
}

/** Case-insensitive text compare for name columns. */
export const byText = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' });
