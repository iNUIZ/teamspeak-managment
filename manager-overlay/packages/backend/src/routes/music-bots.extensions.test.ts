import assert from 'node:assert/strict';
import { test } from 'node:test';
import express from 'express';
import { musicBotRoutes } from './music-bots.routes.js';
import { errorHandler } from '../middleware/error-handler.js';

async function fixture(role = 'admin') {
  const rows = new Map<string, { key: string; value: string }>();
  let writes = 0;
  const app = express(); app.use(express.json());
  app.use((req, _res, next) => { (req as any).user = { id: 1, role, username: 'fixture-admin' }; next(); });
  app.locals.prisma = {
    musicBot: { findUnique: async ({ where }: any) => [7, 8].includes(where.id) ? { id: where.id, serverConfigId: 3 } : null },
    chatCommand: { findMany: async ({ where }: any) => { assert.equal(where.serverConfigId, 3); return [{ name: 'welcome', description: 'fixture', enabled: true }]; } },
    appSetting: {
      findUnique: async ({ where }: any) => rows.get(where.key) ?? null,
      upsert: async ({ where, create, update }: any) => { writes++; const row = rows.has(where.key) ? { key: where.key, ...update } : create; rows.set(where.key, row); return row; },
    },
  };
  app.use('/bots', musicBotRoutes); app.use(errorHandler);
  const server = app.listen(0); await new Promise<void>(resolve => server.once('listening', resolve));
  const origin = `http://127.0.0.1:${(server.address() as any).port}/bots`;
  const put = (path: string, body: unknown) => fetch(origin + path, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  return { origin, put, rows, writes: () => writes, close: () => new Promise<void>(resolve => { server.close(() => resolve()); server.closeAllConnections(); }) };
}

test('bot command settings are admin-only, validated and path-scoped', async () => {
  const denied = await fixture('viewer');
  try {
    assert.equal((await fetch(denied.origin + '/7/extensions')).status, 403);
    assert.equal((await denied.put('/7/extensions', { disabledCommands: ['play'] })).status, 403);
    assert.equal((await denied.put('/7/tts', {})).status, 403);
    assert.equal(denied.writes(), 0);
  } finally { await denied.close(); }
  const f = await fixture();
  try {
    assert.equal((await f.put('/0/extensions', { disabledCommands: [] })).status, 400);
    assert.equal((await f.put('/999/extensions', { disabledCommands: [] })).status, 404);
    assert.equal((await f.put('/7/extensions', { disabledCommands: ['../x'] })).status, 400);
    assert.equal(f.writes(), 0);
    assert.equal((await f.put('/7/extensions', { botId: 8, disabledCommands: ['next'] })).status, 200);
    const a = await (await fetch(f.origin + '/7/extensions')).json();
    const b = await (await fetch(f.origin + '/8/extensions')).json();
    assert.deepEqual(a.disabledCommands, ['skip']);
    assert.deepEqual(b.disabledCommands, []);
    assert.equal(a.serverConfigId, 3);
    assert.equal(a.custom[0].name, 'welcome');
  } finally { await f.close(); }
});

test('TTS route writes selected bot only and rejects invalid settings before persistence', async () => {
  const f = await fixture();
  const settings = { enabled: false, language: 'en', maxCharacters: 150, cooldownSeconds: 20, volume: 0.5 };
  f.rows.set('tts_settings', { key: 'tts_settings', value: JSON.stringify({ ...settings, language: 'tr' }) });
  try {
    assert.equal((await f.put('/7/tts', { ...settings, volume: 50 })).status, 400);
    assert.equal(f.writes(), 0);
    assert.equal((await f.put('/7/tts', settings)).status, 200);
    assert.equal((await (await fetch(f.origin + '/7/tts')).json()).settings.language, 'en');
    assert.equal((await (await fetch(f.origin + '/8/tts')).json()).settings.language, 'tr');
    assert.equal(JSON.parse(f.rows.get('tts_settings')!.value).language, 'tr');
  } finally { await f.close(); }
});
