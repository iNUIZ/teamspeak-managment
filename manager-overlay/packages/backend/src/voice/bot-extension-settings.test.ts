import assert from 'node:assert/strict';
import { test } from 'node:test';
import { botExtensionsKey, canonicalBotCommand, parseBotExtensions, readBotExtensions } from './bot-extension-settings.js';
test('extension command switches validate, deduplicate and normalize aliases', () => {
  assert.deepEqual(parseBotExtensions({ disabledCommands: ['skip', 'next', 'volume'] }), { disabledCommands: ['skip', 'vol'] });
  assert.equal(canonicalBotCommand('come'), 'here');
  for (const disabledCommands of [null, ['../secret'], ['!play'], [4], Array(257).fill('play')]) assert.throws(() => parseBotExtensions({ disabledCommands }));
  assert.throws(() => botExtensionsKey(0));
});
test('command switches are isolated by bot and fresh reads see changes', async () => {
  const rows = new Map([[botExtensionsKey(1), JSON.stringify({ disabledCommands: ['ara'] })]]);
  const prisma = { appSetting: { findUnique: async ({ where }: any) => rows.has(where.key) ? { value: rows.get(where.key) } : null } } as any;
  assert.deepEqual((await readBotExtensions(prisma, 1)).disabledCommands, ['ara']);
  assert.deepEqual((await readBotExtensions(prisma, 2)).disabledCommands, []);
  rows.set(botExtensionsKey(1), JSON.stringify({ disabledCommands: [] }));
  assert.deepEqual((await readBotExtensions(prisma, 1)).disabledCommands, []);
});
