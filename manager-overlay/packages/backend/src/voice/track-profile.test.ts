import { test } from 'node:test';
import assert from 'node:assert/strict';
import { trackCoverUrl, trackDescription, fetchTrackCover } from './track-profile.js';
import type { QueueItem } from './playlist/queue.js';
import { VoiceBot } from './voice-bot.js';
const track: QueueItem = { id: '1', title: 'Şarkı', artist: 'Sanatçı', duration: 203,
  source: 'youtube', filePath: '', sourceUrl: 'https://www.youtube.com/watch?v=GfTBpCuu6yI' };
test('track description has title, artist and duration; idle clears it', () => {
  assert.equal(trackDescription(track), 'Şarkı • Sanatçı • 3:23');
  assert.equal(trackDescription(null), '');
  assert.ok(!trackDescription({ ...track, title: '[url]bad\ntext[/url]' }).includes('['));
  assert.ok(trackDescription({ ...track, title: 'a'.repeat(2000) }).length < 500);
  assert.ok(Buffer.byteLength(trackDescription({ ...track, title: 'ş🎵'.repeat(500) }), 'utf8') <= 200);
});
test('only canonical YouTube artwork, never arbitrary media URLs', async () => {
  assert.equal(trackCoverUrl(track), 'https://i.ytimg.com/vi/GfTBpCuu6yI/mqdefault.jpg');
  assert.equal(trackCoverUrl({ ...track, sourceUrl: 'http://127.0.0.1/private' }), null);
  assert.equal(trackCoverUrl({ ...track, source: 'radio' }), null);
  assert.equal(trackCoverUrl(null), null);
  await assert.rejects(fetchTrackCover('https://i.ytimg.com.evil.test/x', new AbortController().signal));
  const cancelled = new AbortController(); cancelled.abort();
  await assert.rejects(fetchTrackCover('https://i.ytimg.com/vi/GfTBpCuu6yI/mqdefault.jpg', cancelled.signal));
});
test('profile starts once with audio, resets on idle, does not change manual avatar mode', () => {
  const bot: any = new VoiceBot({ id: 1, serverConfigId: 1, name: 'Test', serverHost: '127.0.0.1', serverPort: 9987,
    nickname: 'Test', volume: 50, avatarMode: 'now-playing' });
  bot._status = 'connected';
  bot.client.sendCommand = () => {};
  const calls: unknown[] = [];
  bot.applyAvatar = (...args: unknown[]) => { calls.push(args); return Promise.resolve(); };
  bot._nowPlaying = track;
  assert.equal(calls.length, 0); // selection alone is not playback
  bot.startTrackProfile(); bot.startTrackProfile();
  assert.equal(calls.length, 1);
  bot._nowPlaying = null; bot.resetNickname();
  assert.deepEqual(calls[1], [{}, true]);
  bot.config.avatarMode = 'custom'; bot._nowPlaying = { ...track, id: '2' };
  bot.startTrackProfile();
  assert.equal(calls.length, 2);
});
