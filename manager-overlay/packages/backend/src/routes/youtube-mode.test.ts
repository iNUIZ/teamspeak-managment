import assert from 'node:assert/strict';
import express from 'express';
import { test } from 'node:test';
import { settingsRoutes } from './settings.routes.js';
import { errorHandler } from '../middleware/error-handler.js';
import { getYouTubeAuthMode, setYouTubeAuthMode } from '../voice/audio/youtube.js';

test('playback mode validates input, requires admin, persists and audits the explicit opt-in', async () => {
  const app = express(); app.use(express.json());
  let role = 'viewer'; let fail = false;
  const settings: unknown[] = []; const audits: any[] = [];
  const prisma = {
    appSetting: { upsert: async (row: unknown) => { if (fail) throw new Error('database unavailable'); settings.push(row); return row; } },
    adminAuditEvent: { create: async ({ data }: any) => { audits.push(data); return data; } },
    $transaction: async (fn: any) => fn(prisma),
  };
  app.locals.prisma = prisma;
  app.use((req, _res, next) => { (req as any).user = { id: 1, role, username: 'test' }; next(); });
  app.use('/api/settings', settingsRoutes); app.use(errorHandler);
  const server = app.listen(0, '127.0.0.1');
  await new Promise<void>((resolve) => server.once('listening', resolve));
  const addr = server.address(); assert.ok(addr && typeof addr !== 'string');
  const url = `http://127.0.0.1:${addr.port}/api/settings/yt-cookies/mode`;
  const put = (mode: string) => fetch(url, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ mode }) });
  try {
    assert.equal((await put('po-token')).status, 403);
    role = 'admin';
    assert.equal((await put('invalid-mode')).status, 400);
    assert.equal(settings.length, 0);
    setYouTubeAuthMode('cookies');
    fail = true;
    assert.equal((await put('po-token')).status, 500);
    assert.equal(getYouTubeAuthMode(), 'cookies', 'failed persistence must not change active mode');
    fail = false;
    assert.equal((await put('po-token')).status, 200);
    assert.equal(getYouTubeAuthMode(), 'po-token');
    assert.equal(settings.length, 1);
    assert.equal(audits[0].action, 'settings.youtube_auth_mode_changed');
    assert.ok(!JSON.stringify(audits).includes('cookie-value'));
  } finally {
    setYouTubeAuthMode('po-token');
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
});
