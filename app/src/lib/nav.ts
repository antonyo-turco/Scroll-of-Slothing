// Navigation model: 7 rail destinations (+ settings) with optional sub-views.
// Route ids are persisted (settings.start_view) — never rename them.

import type { Component } from 'svelte';

export type IconName =
  | 'library' | 'token' | 'scissors' | 'cards' | 'map' | 'flag' | 'music' | 'settings'
  | 'search' | 'chevron-left' | 'chevron-right' | 'x' | 'check' | 'loader' | 'command';

export interface View {
  id: string;
  label: string;
  /** Phase in which the view becomes functional (see docs/roadmap.md). */
  phase: number;
  load: () => Promise<{ default: Component }>;
}

export interface Destination {
  id: string;
  label: string;
  icon: IconName;
  views: View[];
  disabled?: boolean;
  hint?: string;
}

const placeholder = () => import('../routes/Placeholder.svelte');

export const DESTINATIONS: Destination[] = [
  { id: 'library', label: 'Library', icon: 'library',
    views: [{ id: 'library', label: 'Library', phase: 1, load: () => import('../routes/library/Library.svelte') }] },
  { id: 'tokens', label: 'Tokens', icon: 'token', views: [
    { id: 'tokens.cutter', label: 'Cutter', phase: 2, load: () => import('../routes/tokens/TokenCutter.svelte') },
    { id: 'tokens.sheet', label: 'Sheet', phase: 2, load: () => import('../routes/tokens/TokenSheet.svelte') },
  ] },
  { id: 'cutouts', label: 'Cut-outs', icon: 'scissors', views: [
    { id: 'cutouts.cutter', label: 'Cutter', phase: 3, load: () => import('../routes/cutouts/ObjectCutter.svelte') },
    { id: 'cutouts.sheet', label: 'Sheet', phase: 3, load: () => import('../routes/cutouts/ObjectSheet.svelte') },
  ] },
  { id: 'cards', label: 'Cards', icon: 'cards',
    views: [{ id: 'cards', label: 'Cards', phase: 4, load: () => import('../routes/cards/Cards.svelte') }] },
  { id: 'maps', label: 'Maps', icon: 'map', views: [
    { id: 'maps.paginator', label: 'Paginator', phase: 5, load: placeholder },
    { id: 'maps.aoe', label: 'AoE templates', phase: 5, load: placeholder },
  ] },
  { id: 'campaigns', label: 'Campaigns', icon: 'flag',
    views: [{ id: 'campaigns', label: 'Campaigns', phase: 1, load: () => import('../routes/Campaigns.svelte') }] },
  { id: 'music', label: 'Music', icon: 'music', views: [], disabled: true,
    hint: 'Planned — see docs/features/music' },
];

export const SETTINGS_DEST: Destination = {
  id: 'settings', label: 'Settings', icon: 'settings',
  views: [{ id: 'settings', label: 'Settings', phase: 0, load: () => import('../routes/Settings.svelte') }],
};

export const ALL_VIEWS: View[] = [...DESTINATIONS, SETTINGS_DEST].flatMap((d) => d.views);

export function findView(id: string): { dest: Destination; view: View } | undefined {
  for (const dest of [...DESTINATIONS, SETTINGS_DEST]) {
    const view = dest.views.find((v) => v.id === id);
    if (view) return { dest, view };
  }
  return undefined;
}

export const isRoute = (id: string): boolean => ALL_VIEWS.some((v) => v.id === id);
