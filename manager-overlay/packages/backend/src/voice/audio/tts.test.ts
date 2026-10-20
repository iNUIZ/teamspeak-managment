import { test } from 'node:test';
import assert from 'node:assert/strict';
import { botTtsSettingsKey, loadTtsSettings, parseSpeech, parseTtsSettings, synthesizeSpeech } from './tts.js';
const defaults = { enabled: true, language: 'tr' as const, maxCharacters: 100, cooldownSeconds: 10, volume: 1 };
test('bot-specific TTS settings are isolated and legacy settings remain read-only fallback', async () => {
  const rows = new Map([['tts_settings', JSON.stringify(defaults)], [botTtsSettingsKey(1), JSON.stringify({ ...defaults, language: 'en', enabled: false })]]);
  const prisma = { appSetting: { findUnique: async ({ where }: any) => rows.has(where.key) ? { value: rows.get(where.key) } : null } } as any;
  assert.equal((await loadTtsSettings(prisma, 1)).enabled, false);
  assert.equal((await loadTtsSettings(prisma, 1)).language, 'en');
  assert.deepEqual(await loadTtsSettings(prisma, 2), defaults);
  assert.equal(rows.size, 2, 'reads do not alter legacy or other bot settings');
  assert.throws(() => botTtsSettingsKey(-1));
});
test('TR and EN voice selection and bounded speech', () => {
  assert.deepEqual(parseSpeech('Merhaba', defaults), { language: 'tr', text: 'Merhaba' });
  assert.deepEqual(parseSpeech('--en Hello', defaults), { language: 'en', text: 'Hello' });
  assert.equal(parseSpeech('$(touch /tmp/unsafe); <hello>', defaults).text, '$(touch /tmp/unsafe); <hello>');
  assert.equal(parseSpeech('[[ phonemes ]]\u0000hello', defaults).text, 'phonemes  hello');
  assert.throws(() => parseSpeech('', defaults));
  assert.throws(() => parseSpeech('x'.repeat(101), defaults));
});
test('settings reject abusive limits, non-finite values and unknown voices', () => {
  assert.deepEqual(parseTtsSettings(defaults), defaults);
  for (const update of [{ maxCharacters: 10000 }, { cooldownSeconds: 0 }, { volume: NaN }, { language: '../../etc' }, { enabled: 'yes' }]) {
    assert.throws(() => parseTtsSettings({ ...defaults, ...update }));
  }
});
test('disabled TTS never launches the voice engine', async () => {
  await assert.rejects(synthesizeSpeech(1, 'Hello', { ...defaults, enabled: false }), /disabled/);
});
