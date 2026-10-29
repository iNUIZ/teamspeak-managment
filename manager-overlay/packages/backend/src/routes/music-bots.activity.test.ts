import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import express from 'express';
import { musicBotRoutes } from './music-bots.routes.js';
import { errorHandler } from '../middleware/error-handler.js';

function fixture() {
  const recorded: Array<{ botId: number; entry: Record<string, unknown> }> = [];
  const bot = {
    id: 1,
    currentConfig: { id: 1, serverConfigId: 1, volume: 50 },
    async applyVolume(v: number) { this.currentConfig.volume = v; },
  };
  const app = express();
  app.use(express.json());
  app.use((req, _res, next) => { (req as any).user = { id: 1, role: 'admin', username: 'admin' }; next(); });
  app.locals.prisma = {
    musicBot: { update: async () => ({}) },
    adminAuditEvent: { create: async () => ({ id: 1 }), updateMany: async () => ({ count: 1 }) },
  };
  app.locals.voiceBotManager = {
    getBot: (id: number) => (id === 1 ? bot : undefined), on() {}, off() {},
    activity: {
      record: async (botId: number, entry: Record<string, unknown>) => { recorded.push({ botId, entry }); },
      list: async () => [{ at: '2026-10-29T20:34:21.000Z', via: 'teamspeak', who: 'BeBeK', action: '!vol 1' }],
    },
  };
  app.use('/api/music-bots', musicBotRoutes);
  app.use(errorHandler);
  return { app, recorded };
}

async function send(app: express.Express, method: string, path: string, body?: unknown) {
  const server = app.listen(0);
  await new Promise<void>((r) => server.once('listening', () => r()));
  const { port } = server.address() as { port: number };
  try {
    const res = await fetch(`http://127.0.0.1:${port}${path}`, {
      method, headers: { 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body),
    });
    return { status: res.status, body: await res.json() as any };
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
}

describe('bot activity', () => {
  it('records a successful panel action as the panel user in command form', async () => {
    const f = fixture();
    assert.equal((await send(f.app, 'POST', '/api/music-bots/1/volume', { volume: 72 })).status, 200);
    await new Promise((r) => setImmediate(r));
    assert.deepEqual(f.recorded, [{ botId: 1, entry: { via: 'panel', who: 'admin', action: '!vol 72' } }]);
  });

  it('does not record refused actions', async () => {
    const f = fixture();
    assert.equal((await send(f.app, 'POST', '/api/music-bots/1/volume', { volume: 'loud' })).status, 400);
    await new Promise((r) => setImmediate(r));
    assert.deepEqual(f.recorded, []);
  });

  it('lists who did what for a known bot only', async () => {
    const f = fixture();
    const ok = await send(f.app, 'GET', '/api/music-bots/1/activity');
    assert.equal(ok.status, 200);
    assert.equal(ok.body[0].who, 'BeBeK');
    assert.equal((await send(f.app, 'GET', '/api/music-bots/9/activity')).status, 404);
  });

  it('rejects a search shorter than two characters before searching', async () => {
    const f = fixture();
    assert.equal((await send(f.app, 'POST', '/api/music-bots/1/search', { query: 'a' })).status, 400);
  });
});
