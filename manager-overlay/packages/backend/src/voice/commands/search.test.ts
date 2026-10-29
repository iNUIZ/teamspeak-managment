import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import type { YouTubeSearchResult } from '../audio/youtube.js';
import { handleSearch, qualityScore, searchInternals } from './search.js';
import { setYtCookieFile } from '../audio/youtube.js';
import type { CommandContext } from './context.js';
import type { VoiceBot } from '../voice-bot.js';

const item = (id: string, title: string, artist = 'Artist', duration = 210): YouTubeSearchResult => ({
  id, title, artist, duration, thumbnail: '',
});

describe('mixed !ara ranking', () => {
  it('explains missing cookies instead of pretending that no songs matched', async () => {
    setYtCookieFile(null);
    const replies: string[] = [];
    const context = { reply: async (_bot: unknown, _clid: number, text: string) => { replies.push(text); } } as unknown as CommandContext;
    await handleSearch(context, 1, {} as VoiceBot, 12, 'semicenk');
    assert.equal(replies.length, 1);
    assert.match(replies[0], /cookies.txt/);
    assert.doesNotMatch(replies[0], /sonuç bulunamadı/);
  });
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
  it('labels real Spotify results and queues their Spotify link for YouTube matching', () => {
    const ranked = searchInternals.mergeAndRank('kuzu kuzu', [
      { sourceLabel: 'Spotify', items: [{ ...item('spotify:abc', 'Kuzu Kuzu', 'Tarkan'), url: 'https://open.spotify.com/track/abc' }] },
      { sourceLabel: 'YouTube', items: [item('yt1', 'Kuzu Kuzu', 'Tarkan')] },
    ]);
    const spotify = ranked.find((result) => result.sourceLabel === 'Spotify')!;
    assert.equal(spotify.url, 'https://open.spotify.com/track/abc');
    assert.equal(ranked.find((result) => result.id === 'yt1')!.url, 'https://www.youtube.com/watch?v=yt1');
    assert.match(searchInternals.formatResults('kuzu kuzu', ranked), /\[Spotify\] Tarkan - Kuzu Kuzu/);
  });

  it('shows duration and does not claim a YouTube match is Spotify audio', () => {
    const results = searchInternals.mergeAndRank('song', [{ sourceLabel: 'Spotify match', items: [item('1', 'Song')] }]);
    const text = searchInternals.formatResults('song', results);
    assert.match(text, /3:30/);
    assert.match(text, /\[YouTube\]/);
    assert.ok(!text.includes('Spotify'));
  });
});
