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
      { sourceLabel: 'YouTube', items: [item('yt000000001', 'unrelated reaction', 'Someone'), item('yt000000002', 'Song Official Audio')] },
      { sourceLabel: 'Spotify', items: [item('spotify:ym', 'Song')] },
    ]);
    assert.deepEqual(ranked.map((result) => result.id), ['spotify:ym', 'yt000000002', 'yt000000001']);
  });

  it('drops channels and playlists and cleans "- Topic" channel names', () => {
    const ranked = searchInternals.mergeAndRank('song', [
      { sourceLabel: 'YouTube', items: [item('UCchannelid0000000000', 'Song'), item('PLplaylist', 'Song'), item('abcdefghijk', 'Song', 'Tarkan - Topic')] },
    ]);
    assert.deepEqual(ranked.map((result) => result.id), ['abcdefghijk']);
    assert.equal(ranked[0].artist, 'Tarkan');
  });

  it('deduplicates the same playable video and caps the list at ten', () => {
    const candidates = Array.from({ length: 12 }, (_, index) => item(`video${String(index).padStart(6, '0')}`, `Song ${index}`));
    const ranked = searchInternals.mergeAndRank('song', [
      { sourceLabel: 'YouTube', items: candidates },
      { sourceLabel: 'YouTube', items: [candidates[0]] },
    ]);
    assert.equal(ranked.length, 10);
    assert.equal(new Set(ranked.map((result) => result.id)).size, 10);
  });

  it('penalises low-signal variants and rewards music sources', () => {
    assert.ok(
      qualityScore(item('a', 'Song Official Audio'), 'song', 'YouTube')
        > qualityScore(item('b', 'Song slowed reverb reaction'), 'song', 'YouTube'),
    );
  });
  it('labels real Spotify results and queues their Spotify link for YouTube matching', () => {
    const ranked = searchInternals.mergeAndRank('kuzu kuzu', [
      { sourceLabel: 'Spotify', items: [{ ...item('spotify:abc', 'Kuzu Kuzu', 'Tarkan'), url: 'https://open.spotify.com/track/abc' }] },
      { sourceLabel: 'YouTube', items: [item('yt000000001', 'Kuzu Kuzu', 'Tarkan')] },
    ]);
    const spotify = ranked.find((result) => result.sourceLabel === 'Spotify')!;
    assert.equal(spotify.url, 'https://open.spotify.com/track/abc');
    assert.equal(ranked.find((result) => result.id === 'yt000000001')!.url, 'https://www.youtube.com/watch?v=yt000000001');
    assert.match(searchInternals.formatResults('kuzu kuzu', ranked), /\[Spotify\] Tarkan - Kuzu Kuzu/);
  });

  it('shows duration and labels YouTube results as YouTube', () => {
    const results = searchInternals.mergeAndRank('song', [{ sourceLabel: 'YouTube', items: [item('aaaaaaaaaaa', 'Song')] }]);
    const text = searchInternals.formatResults('song', results);
    assert.match(text, /3:30/);
    assert.match(text, /\[YouTube\]/);
    assert.ok(!text.includes('Spotify'));
  });
});
