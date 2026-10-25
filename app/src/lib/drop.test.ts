import { describe, expect, it } from 'vitest';
import { firstImagePath } from './api';

describe('dropped files', () => {
  it('opens the first image, whatever the extension case', () => {
    expect(firstImagePath(['notes.txt', 'C:\maps\Crypt.PNG', 'b.jpg'])).toBe('C:\maps\Crypt.PNG');
    expect(firstImagePath(['a.webp'])).toBe('a.webp');
  });
  it('ignores drops with no image', () => {
    expect(firstImagePath(['song.mp3', 'README'])).toBeUndefined();
    expect(firstImagePath([])).toBeUndefined();
  });
});
