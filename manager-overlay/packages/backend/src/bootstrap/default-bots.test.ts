import assert from 'node:assert/strict';
import test from 'node:test';
import { seedDefaultBots, DEFAULT_MUSIC_BOT, DEFAULT_RADIO_BOT } from './default-bots.js';

function fakes(existingBots = 0, stationId: number | null = 7) {
  const settings = new Map<string, string>();
  const saved = new Map<number, any>();
  const configured = new Map<number, any>();
  const created: any[] = [];
  const prisma: any = {
    appSetting: {
      findUnique: async ({ where }: any) => (settings.has(where.key) ? { key: where.key } : null),
      create: async ({ data }: any) => { settings.set(data.key, data.value); },
    },
    musicBot: {
      count: async () => existingBots,
      update: async ({ where, data }: any) => { saved.set(where.id, JSON.parse(data.automationSettings)); },
    },
    radioStation: { findFirst: async () => (stationId ? { id: stationId } : null) },
  };
  const manager = {
    createBot: async (data: any) => { created.push(data); return { id: created.length }; },
    getBot: (id: number) => ({ automation: { configure: (s: any) => { configured.set(id, s); } } }),
  };
  return { prisma, manager, created, saved, configured };
}

test('a fresh server gets a music bot and a radio bot that plays a station', async () => {
  const f = fakes();
  assert.deepEqual(await seedDefaultBots(f.prisma, f.manager, 1), [1, 2]);
  assert.deepEqual(f.created.map((b) => b.nickname), [DEFAULT_MUSIC_BOT, DEFAULT_RADIO_BOT]);
  assert.ok(f.created.every((b) => b.autoStart));
  assert.equal(f.saved.get(1).enabled, false);
  assert.equal(f.saved.get(1).stayInChannel, true);
  assert.deepEqual(f.saved.get(2).source, { kind: 'radio', id: 7 });
  assert.equal(f.saved.get(2).enabled, true);
  assert.deepEqual(f.configured.get(2), f.saved.get(2));
});

test('it runs once: a second boot creates nothing', async () => {
  const f = fakes();
  await seedDefaultBots(f.prisma, f.manager, 1);
  assert.deepEqual(await seedDefaultBots(f.prisma, f.manager, 1), []);
  assert.equal(f.created.length, 2);
});

test('a server that already has bots is left alone', async () => {
  const f = fakes(3);
  assert.deepEqual(await seedDefaultBots(f.prisma, f.manager, 1), []);
  assert.equal(f.created.length, 0);
});

test('with no saved stations the radio bot still exists, Auto-DJ off', async () => {
  const f = fakes(0, null);
  await seedDefaultBots(f.prisma, f.manager, 1);
  assert.equal(f.saved.get(2).enabled, false);
  assert.equal(f.saved.get(2).source, null);
});
