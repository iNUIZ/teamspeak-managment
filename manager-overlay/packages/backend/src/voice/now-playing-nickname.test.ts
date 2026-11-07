import assert from 'node:assert/strict';
import test from 'node:test';
import { nowPlayingNickname } from './voice-bot.js';

test('the whole nickname becomes the track, without the bot name in front', () => {
  assert.equal(nowPlayingNickname('Tarkan - Kuzu Kuzu', 'Müzik Botu'), '♪ Tarkan - Kuzu Kuzu');
});

test('long titles are cut to the 30-character TeamSpeak limit', () => {
  const nick = nowPlayingNickname('A very long song title that never seems to end at all', 'Bot');
  assert.equal([...nick].length, 30);
  assert.ok(nick.startsWith('♪ A very long'));
  assert.ok(nick.endsWith('…'));
});

test('an empty title keeps the bot name', () => {
  assert.equal(nowPlayingNickname('   ', 'Müzik Botu'), 'Müzik Botu');
});
