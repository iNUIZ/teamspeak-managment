import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import type { YouTubeSearchResult } from '../audio/youtube.js';
import { qualityScore, searchInternals } from './search.js';

const item = (id: string, title: string, artist = 'Artist', duration = 210): YouTubeSearchResult => ({
  id, title, artist, duration, thumbnail: '',
});

describe('mixed !ara ranking', () => {
  it('mixes sources by quality instead of fixed source buckets', () => {
    const ranked = searchInternals.mergeAndRank('artist song', [
      { sourceLabel: 'YouTube', items: [item('yt', 'unrelated reaction', 'Someone')] },
      { sourceLabel: 'Spotify match', items: [item('sp', 'Song Official Audio')] },
      { sourceLabel: 'YouTube Music', items: [item('ym', 'Song')] },
    ]);
    assert.deepEqual(ranked.map((result) => result.id), ['sp', 'ym', 'yt']);
  });

  it('deduplicates the same playable video and caps the list at ten', () => {
    const candidates = Array.from({ length: 12 }, (_, index) => item(String(index), `Song ${index}`));
    const ranked = searchInternals.mergeAndRank('song', [
      { sourceLabel: 'YouTube Music', items: candidates },
      { sourceLabel: 'YouTube', items: [candidates[0]] },
    ]);
    assert.equal(ranked.length, 10);
    assert.equal(new Set(ranked.map((result) => result.id)).size, 10);
  });

  it('penalises low-signal variants and rewards music sources', () => {
    assert.ok(
      qualityScore(item('a', 'Song Official Audio'), 'song', 'YouTube Music')
        > qualityScore(item('b', 'Song slowed reverb reaction'), 'song', 'YouTube'),
    );
  });
});
