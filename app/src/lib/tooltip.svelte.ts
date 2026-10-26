// A styled tooltip for any element, SVG included: `use:tooltip={{ title, lines }}`.
// One shared floating card (components/TooltipHost.svelte, mounted in App) shows
// the content of whichever element is hovered or focused, above it when there is
// room, otherwise below. For a plain ⓘ explanation use InfoTip instead.

export interface TipContent {
  title: string;
  /** Body lines, each shown as a bullet. */
  lines?: string[];
  /** A muted line under the body. */
  note?: string;
  /** Colour of the dot before the title (a CSS colour); none when omitted. */
  color?: string;
}

class TipState {
  content = $state<TipContent | null>(null);
  open = $state(false);
  /** Anchor: the element's horizontal centre and its top (above) or bottom (below) edge. */
  x = $state(0);
  y = $state(0);
  place = $state<'above' | 'below'>('above');
  owner: Element | null = null;
}

export const tip = new TipState();

// Room needed above the element before the tip flips below it.
const ROOM_ABOVE = 140;

export function tooltip(node: Element, content: TipContent) {
  let current = content;
  const show = () => {
    const r = node.getBoundingClientRect();
    tip.owner = node;
    tip.content = current;
    tip.place = r.top > ROOM_ABOVE ? 'above' : 'below';
    tip.x = r.left + r.width / 2;
    tip.y = tip.place === 'above' ? r.top : r.bottom;
    tip.open = true;
  };
  const hide = () => {
    if (tip.owner === node) tip.open = false;
  };
  const onKey = (e: Event) => (e as KeyboardEvent).key === 'Escape' && hide();
  node.addEventListener('mouseenter', show);
  node.addEventListener('mouseleave', hide);
  node.addEventListener('focus', show);
  node.addEventListener('blur', hide);
  node.addEventListener('keydown', onKey);
  return {
    update(next: TipContent) {
      current = next;
      if (tip.owner === node && tip.open) tip.content = next;
    },
    destroy() {
      hide();
      node.removeEventListener('mouseenter', show);
      node.removeEventListener('mouseleave', hide);
      node.removeEventListener('focus', show);
      node.removeEventListener('blur', hide);
      node.removeEventListener('keydown', onKey);
    },
  };
}
