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
      { sourceLabel: 'YouTube', items: [item('yt000000001', 'Kuzu Kuzu (Live)', 'Tarkan')] },
    ]);
    const spotify = ranked.find((result) => result.sourceLabel === 'Spotify')!;
    assert.equal(spotify.url, 'https://open.spotify.com/track/abc');
    assert.equal(ranked.find((result) => result.id === 'yt000000001')!.url, 'https://www.youtube.com/watch?v=yt000000001');
    assert.match(searchInternals.formatResults('kuzu kuzu', ranked), /\*\*1\.\*\* Kuzu Kuzu — \*Tarkan\* · `3:30` · Spotify/);
  });

  it('shows duration and leaves the YouTube source implicit', () => {
    const results = searchInternals.mergeAndRank('song', [{ sourceLabel: 'YouTube', items: [item('aaaaaaaaaaa', 'Song')] }]);
    const text = searchInternals.formatResults('song', results);
    assert.match(text, /`3:30`/);
    assert.ok(!text.includes('YouTube') && !text.includes('Spotify'));
  });

  it('cleans upload noise, takes the artist from the title and drops films and duplicates', () => {
    const ranked = searchInternals.mergeAndRank('birader', [{ sourceLabel: 'YouTube', items: [
      item('aaaaaaaaaaa', 'Heijan & Muti - BİRADER (Official Video) #HERMANO', 'Heijan and Muti', 205),
      item('bbbbbbbbbbb', 'B4TU INC. - BİRADER (MUSIC VIDEO)', 'B4TU INC.', 117),
      item('ccccccccccc', 'BİRADER', 'B4TU INC.', 116),
      item('ddddddddddd', 'Birader – Dema Tu Dikenî | 8 Track | [HD] | #AVAEntertainment', 'AVA Media', 359),
      item('fffffffffff', 'Birader (Osman Öztunç)', 'ZeMahşer Osman Öztunç TV (Osman Öztunç)', 311),
      item('eeeeeeeeeee', 'KAÇMA BİRADER / Yerli Komedi Filmi Full İzle', 'İlhan Dilek', 5827),
    ] }]);
    const shown = ranked.map((r) => `${r.artist} | ${r.title}`);
    assert.ok(shown.includes('Heijan & Muti | BİRADER'));
    assert.ok(shown.includes('Birader | Dema Tu Dikenî'));
    assert.ok(shown.includes('Osman Öztunç | Birader'));
    assert.equal(shown.filter((s) => s === 'B4TU INC. | BİRADER').length, 1);
    assert.ok(!ranked.some((r) => r.id === 'eeeeeeeeeee'));
  });

  it('keeps every result line whole when the chat message is split', () => {
    const ranked = searchInternals.mergeAndRank('song', [{ sourceLabel: 'YouTube', items: Array.from({ length: 10 }, (_, i) =>
      item(`video${String(i).padStart(6, '0')}`, `Çok uzun bir şarkı başlığı numara ${i} ve daha fazlası burada`, 'Sanatçı Adı')) }]);
    const lines = searchInternals.formatResults('song', ranked).split('\n');
    assert.equal(lines.filter((l) => /^\*\*\d+\.\*\*/.test(l)).length, 10);
    assert.ok(lines.every((l) => Buffer.byteLength(l) < 200));
  });
});
