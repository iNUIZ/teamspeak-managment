import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EventEmitter } from 'node:events';
import { PrivilegeKeyHistory, keyFingerprint } from './privilege-key-history.js';
function fixture() {
  const saved = new Map<string, any>(); const bridge = new EventEmitter() as any; const commands: string[] = [];
  bridge.retainSession = async () => true; bridge.releaseSession = async () => {}; bridge.isRegistered = () => true;
  bridge.executeCommand = async (_c: number, _s: number, command: string) => { commands.push(command); return ''; };
  const db = { appSetting: { findUnique: async ({ where }: any) => saved.get(where.key), findMany: async () => [...saved.values()],
    upsert: async ({ where, create, update }: any) => { saved.set(where.key, saved.has(where.key) ? { key: where.key, ...update } : create); } } } as any;
  return { saved, bridge, commands, service: new PrivilegeKeyHistory(db, bridge) };
}
test('records actual scoped tokenused events only; tokens and arbitrary payload fields never persist', async () => {
  const f = fixture(); await f.service.start(); try {
    f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', { clid: '7', token: 'untracked-secret' }); assert.equal((await f.service.list(1, 1)).items.length, 0);
    await f.service.setEnabled(1, 1, true); assert.deepEqual(f.commands, ['servernotifyregister event=tokenused']);
    const data = { clid: '7', cldbid: '22', cluid: 'test-uid', client_nickname: 'Fixture', token: 'raw-test-token-secret', password: 'must-not-persist', ip: 'private-ip' };
    f.bridge.emit('tsEvent', 2, 1, 'notifytokenused', data); f.bridge.emit('tsEvent', 1, 1, 'notifytextmessage', data);
    f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', data); f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', data);
    const history = await f.service.list(1, 1); assert.equal(history.items.length, 1); assert.equal(history.status, 'capturing');
    assert.equal(history.items[0].fingerprint, keyFingerprint(data.token)); assert.equal(history.items[0].databaseId, 22);
    const json = JSON.stringify([...f.saved.values()]); for (const secret of [data.token, data.password, data.ip]) assert.ok(!json.includes(secret));
    await f.service.setEnabled(1, 1, false); f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', { ...data, token: 'another-secret' });
    assert.equal((await f.service.list(1, 1)).items.length, 1);
  } finally { await f.service.stop(); }
});
test('reconnect requires renewed token subscription; history persists and is bounded', async () => {
  const f = fixture(); await f.service.start(); try {
    await f.service.setEnabled(1, 1, true); f.bridge.emit('sshDisconnected', 1, 1); assert.equal((await f.service.list(1, 1)).status, 'connecting');
    f.bridge.emit('sshConnected', 1, 1); await new Promise(resolve => setImmediate(resolve)); assert.equal(f.commands.length, 2);
    for (let i = 0; i < 210; i++) f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', { clid: '1', token: `fixture-token-${i}` });
    assert.equal((await f.service.list(1, 1)).items.length, 200);
    await f.service.stop(); await f.service.start(); assert.equal((await f.service.list(1, 1)).items.length, 200); assert.equal((await f.service.list(1, 1)).enabled, true);
  } finally { await f.service.stop(); }
});
test('failed token subscription never claims capture is healthy', async () => {
  const f = fixture(); f.bridge.executeCommand = async () => { throw new Error('TS error 2568: insufficient client permissions'); };
  await f.service.start(); try { assert.equal((await f.service.setEnabled(1, 1, true)).status, 'gap'); } finally { await f.service.stop(); }
});
test('records cached nickname only when it belongs to the event identity', async () => {
  const f = fixture(); f.bridge.clientCache = { lookup: () => ({ client_nickname: 'Known person', client_unique_identifier: 'known-uid', client_database_id: '9' }) };
  await f.service.start(); try {
    await f.service.setEnabled(1, 1, true);
    f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', { clid: '1', token: 'known-test-key', cluid: 'known-uid' });
    f.bridge.emit('tsEvent', 1, 1, 'notifytokenused', { clid: '1', token: 'recycled-test-key', cluid: 'different-uid' });
    const { items } = await f.service.list(1, 1);
    assert.equal(items[0].nickname, null); assert.equal(items[0].databaseId, null);
    assert.equal(items[1].nickname, 'Known person'); assert.equal(items[1].databaseId, 9);
  } finally { await f.service.stop(); }
});
