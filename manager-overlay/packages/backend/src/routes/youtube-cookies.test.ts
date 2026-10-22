import assert from 'node:assert/strict';
import express from 'express';
import { test } from 'node:test';
import { settingsRoutes } from './settings.routes.js';
import { errorHandler } from '../middleware/error-handler.js';

test('legacy mode changes are admin-only and cannot enable the removed provider', async () => {
  const app = express(); app.use(express.json());
  let role = 'viewer';
  app.use((req, _res, next) => { (req as any).user = { id: 1, role, username: 'test' }; next(); });
  app.use('/api/settings', settingsRoutes); app.use(errorHandler);
  const server = app.listen(0, '127.0.0.1');
  await new Promise<void>((resolve) => server.once('listening', resolve));
  const addr = server.address(); assert.ok(addr && typeof addr !== 'string');
  const url = `http://127.0.0.1:${addr.port}/api/settings/yt-cookies`;
  const put = (mode: string) => fetch(`${url}/mode`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ mode }) });
  try {
    assert.equal((await put('po-token')).status, 403);
    role = 'admin';
    assert.equal((await put('po-token')).status, 410);
    assert.equal((await put('cookies')).status, 410);
    const status = await (await fetch(url)).json() as any;
    assert.equal(status.mode, 'cookies');
    assert.equal('provider' in status, false);
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
});
