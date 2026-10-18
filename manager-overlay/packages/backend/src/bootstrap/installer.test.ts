import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import bcrypt from 'bcryptjs';
import type { PrismaClient } from '../../generated/prisma/index.js';

test('installer stages only a hash, claims fresh setup once, and preserves existing users', async () => {
  const original = process.cwd();
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'teamhub-bootstrap-test-'));
  process.chdir(directory);
  try {
    const { prepareInstallerAdmin, applyInstallerAdmin } = await import('./installer.js');
    const users: any[] = [];
    const tx = { user: { count: async () => users.length, create: async ({ data }: any) => users.push(data) } };
    const prisma = { $transaction: async (fn: any) => fn(tx) } as unknown as PrismaClient;
    await assert.rejects(prepareInstallerAdmin('admin', 'weak'));
    await prepareInstallerAdmin('owner', 'TestPassword123!');
    const staged = await fs.readFile('data/.installer-admin.json', 'utf8');
    assert.ok(!staged.includes('TestPassword123!'));
    assert.equal(await bcrypt.compare('TestPassword123!', JSON.parse(staged).passwordHash), true);
    await applyInstallerAdmin(prisma);
    assert.equal(users.length, 1);
    assert.equal(users[0].role, 'admin');
    await assert.rejects(fs.access('data/.installer-admin.json'));
    await applyInstallerAdmin(prisma);
    await prepareInstallerAdmin('replacement', 'OtherPassword123!');
    await applyInstallerAdmin(prisma);
    assert.equal(users.length, 1);
    assert.equal(users[0].username, 'owner');
  } finally {
    process.chdir(original);
    await fs.rm(directory, { recursive: true, force: true });
  }
});

test('TS3 and TS6 coexist; repeated sync preserves IDs, disabled status and custom names', async () => {
  process.env.ENCRYPTION_KEY = 'test-only-bootstrap-key';
  const { syncInstallerConnection } = await import('./installer.js');
  const { decrypt } = await import('../utils/crypto.js');
  const connections: any[] = [];
  const tx = { tsServerConfig: {
    findFirst: async ({ where }: any) => connections.find((c) => c.host === where.host && c.webqueryPort === where.webqueryPort),
    create: async ({ data }: any) => { connections.push({ ...data, id: connections.length + 1 }); },
    update: async ({ where, data }: any) => Object.assign(connections.find((c) => c.id === where.id), data),
  } };
  const prisma = { $transaction: async (fn: any) => fn(tx) } as unknown as PrismaClient;
  assert.equal(await syncInstallerConnection(prisma, 'TS3', 'teamhub-ts3-1', 'key-three'), true);
  assert.equal(await syncInstallerConnection(prisma, 'TS6', 'teamhub-ts6-1', 'key-six'), true);
  connections[0].name = 'My community';
  connections[0].enabled = false;
  assert.equal(await syncInstallerConnection(prisma, 'TS3', 'teamhub-ts3-1', 'key-three'), false);
  assert.equal(connections.length, 2);
  assert.equal(await syncInstallerConnection(prisma, 'TS3', 'teamhub-ts3-1', 'rotated-key'), true);
  assert.equal(connections[0].name, 'My community');
  assert.equal(connections[0].enabled, false);
  assert.equal(connections[0].id, 1);
  assert.ok(connections[0].apiKey.startsWith('enc:'));
  assert.equal(decrypt(connections[0].apiKey), 'rotated-key');
  await assert.rejects(syncInstallerConnection(prisma, 'TS3', 'invalid/host', 'key'));
});
