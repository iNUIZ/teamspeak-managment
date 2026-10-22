import assert from 'node:assert/strict';
import { test } from 'node:test';
import express from 'express';
import { messageRoutes } from './messages.routes.js';
import { AppError, errorHandler } from '../middleware/error-handler.js';

async function fixture(role = 'admin') {
  const calls: any[] = []; const audits: any[] = []; let fail = false;
  const app = express(); app.use(express.json());
  app.use((req, _res, next) => { (req as any).user = { id: 7, username: 'review-admin', role }; next(); });
  app.locals.connectionPool = { getClient: (id: number) => { assert.equal(id, 3); return { execute: async (sid: number, command: string, params: any) => {
    calls.push({ sid, command, params }); if (fail) throw new AppError(403, 'Recipient is unavailable'); return [];
  } }; } };
  app.locals.prisma = { adminAuditEvent: { create: async ({ data }: any) => { audits.push(data); return { id: 'audit-1' }; }, updateMany: async ({ data }: any) => { audits.push(data); return { count: 1 }; } } };
  app.use('/servers/:configId/vs/:sid/messages', messageRoutes); app.use(errorHandler);
  const server = app.listen(0); await new Promise<void>(resolve => server.once('listening', resolve));
  const url = `http://127.0.0.1:${(server.address() as any).port}/servers/3/vs/2/messages`;
  return { calls, audits, setFail: () => { fail = true; }, post: (body: any) => fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }), close: () => new Promise<void>(resolve => { server.close(() => resolve()); server.closeAllConnections(); }) };
}
test('named online delivery uses selected SID and live chat, never offline mailbox', async () => {
  const f = await fixture(); try {
    const response = await f.post({ mode: 'chat', clid: 11, message: 'private-body-secret', subject: 'ignored', arbitrary: 'ignored' });
    assert.equal(response.status, 201); assert.deepEqual(await response.json(), { accepted: true, mode: 'chat' });
    assert.deepEqual(f.calls, [{ sid: 2, command: 'clientinfo', params: { clid: 11 } }, { sid: 2, command: 'sendtextmessage', params: { targetmode: 1, target: 11, msg: 'private-body-secret' } }]);
    assert.ok(!JSON.stringify(f.audits).includes('private-body-secret')); assert.equal(f.audits[0].actorUserId, 7);
  } finally { await f.close(); }
});
test('legacy offline mode remains supported, allow-listed and secret-free in audit', async () => {
  const f = await fixture(); try {
    assert.equal((await f.post({ cluid: 'abcdefghijklmnopqrstuvwx=', subject: 'private-subject', message: 'private-text', targetmode: 3 })).status, 201);
    assert.deepEqual(f.calls[0], { sid: 2, command: 'messageadd', params: { cluid: 'abcdefghijklmnopqrstuvwx=', subject: 'private-subject', message: 'private-text' } });
    assert.ok(!JSON.stringify(f.audits).includes('private-')); assert.ok(!JSON.stringify(f.audits).includes('abcdefghijkl'));
  } finally { await f.close(); }
});
test('refuses a recycled client ID instead of sending to a different identity', async () => {
  const f = await fixture(); try {
    assert.equal((await f.post({ mode: 'chat', clid: 11, expectedUid: 'abcdefghijklmnopqrstuvwx=', message: 'private-text' })).status, 409);
    assert.equal(f.calls.length, 1); assert.equal(f.calls[0].command, 'clientinfo');
    assert.equal(f.audits.at(-1).outcome, 'failure');
  } finally { await f.close(); }
});
test('invalid inputs and viewer writes never dispatch; remote failure is not accepted', async () => {
  const viewer = await fixture('viewer'); try { assert.equal((await viewer.post({ mode: 'chat', clid: 11, message: 'test' })).status, 403); assert.equal(viewer.calls.length, 0); } finally { await viewer.close(); }
  const f = await fixture(); try {
    for (const body of [{ mode: 'other', message: 'x' }, { mode: 'chat', clid: -1, message: 'x' }, { mode: 'chat', clid: 1, message: ' ' }, { mode: 'offline', cluid: 'bad', subject: 'x', message: 'x' }]) assert.equal((await f.post(body)).status, 400);
    assert.equal(f.calls.length, 0); f.setFail(); assert.equal((await f.post({ mode: 'chat', clid: 11, message: 'x' })).status, 403);
    assert.equal(f.calls.length, 1); assert.equal(f.audits.at(-1).outcome, 'failure');
  } finally { await f.close(); }
});
