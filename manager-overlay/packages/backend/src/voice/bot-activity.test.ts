import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BotActivityLog, describeChatCommand } from './bot-activity.js';

function memoryPrisma() {
  const rows = new Map<string, string>();
  return {
    rows,
    appSetting: {
      findUnique: async ({ where }: any) => (rows.has(where.key) ? { key: where.key, value: rows.get(where.key)! } : null),
      upsert: async ({ where, create, update }: any) => { rows.set(where.key, rows.has(where.key) ? update.value : create.value); },
    },
  };
}

test('records who did what, newest first, and keeps the last 50', async () => {
  const prisma = memoryPrisma();
  let n = 0;
  const log = new BotActivityLog(prisma as any, () => new Date(Date.UTC(2026, 9, 29, 20, 34, n++)));
  await log.record(4, { via: 'teamspeak', who: 'BeBeK', action: '!vol 1' });
  await log.record(4, { via: 'panel', who: 'admin', action: '!pause' });
  const entries = await log.list(4);
  assert.deepEqual(entries.map((e) => `${e.who} ${e.action} ${e.via}`), ['admin !pause panel', 'BeBeK !vol 1 teamspeak']);
  for (let i = 0; i < 60; i++) await log.record(4, { via: 'teamspeak', who: 'x', action: `!skip ${i}` });
  assert.equal((await log.list(4)).length, 50);
  assert.deepEqual(await log.list(3), []);
});

test('never stores spoken text or remote details and names unknown callers', async () => {
  assert.equal(describeChatCommand('say', 'secret words', null), '!say');
  assert.equal(describeChatCommand('remote', 'Bot', null), '!remote');
  assert.equal(describeChatCommand('play', '', 3), '!ara sonucu #3');
  assert.equal(describeChatCommand('vol', '40', null), '!vol 40');
  const prisma = memoryPrisma();
  const log = new BotActivityLog(prisma as any);
  await log.record(1, { via: 'teamspeak', who: '  ', action: '!np' });
  assert.equal((await log.list(1))[0].who, 'Bilinmeyen');
});

test('a failing store does not throw into the command path', async () => {
  const log = new BotActivityLog({ appSetting: { findUnique: async () => { throw new Error('db down'); } } } as any);
  await log.record(1, { via: 'panel', who: 'admin', action: '!skip' });
});
