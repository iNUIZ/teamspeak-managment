import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { PrismaClient } from '../../generated/prisma/index.js';

test('real SQLite: initial admin, both server types, key rotation and linked bots survive sync', async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'teamhub-sqlite-test-'));
  const original = process.cwd();
  const backendRoot = fileURLToPath(new URL('../../', import.meta.url));
  const databaseUrl = `file:${path.join(directory, 'test.db').replaceAll('\\', '/')}`;
  const require = createRequire(import.meta.url);
  const prisma = new PrismaClient({ datasources: { db: { url: databaseUrl } } });
  try {
    await prisma.$queryRawUnsafe('SELECT 1');
    execFileSync(process.execPath, [require.resolve('prisma/build/index.js'), 'db', 'push', '--skip-generate'], {
      cwd: backendRoot, env: { ...process.env, DATABASE_URL: databaseUrl }, stdio: 'pipe',
    });
    process.chdir(directory);
    process.env.ENCRYPTION_KEY = 'sqlite-test-only-key';
    const { prepareInstallerAdmin, applyInstallerAdmin, syncInstallerConnection } = await import('./installer.js');
    const { decrypt } = await import('../utils/crypto.js');
    await prepareInstallerAdmin('owner', 'TestPassword123!');
    await applyInstallerAdmin(prisma);
    assert.equal(await prisma.user.count(), 1);
    await syncInstallerConnection(prisma, 'TS3', 'teamhub-ts3-1', 'test-ts3-key');
    await syncInstallerConnection(prisma, 'TS6', 'teamhub-ts6-1', 'test-ts6-key');
    const ts3 = await prisma.tsServerConfig.findFirstOrThrow({ where: { host: 'teamhub-ts3-1' } });
    await prisma.tsServerConfig.update({ where: { id: ts3.id }, data: { name: 'Community', enabled: false } });
    await prisma.botFlow.create({ data: { name: 'Existing bot', serverConfigId: ts3.id, virtualServerId: 1, flowData: '{}' } });
    assert.equal(await syncInstallerConnection(prisma, 'TS3', 'teamhub-ts3-1', 'rotated-ts3-key'), true);
    assert.equal(await syncInstallerConnection(prisma, 'TS3', 'teamhub-ts3-1', 'rotated-ts3-key'), false);
    assert.equal(await prisma.tsServerConfig.count(), 2);
    const restored = await prisma.tsServerConfig.findUniqueOrThrow({ where: { id: ts3.id } });
    assert.equal(restored.name, 'Community');
    assert.equal(restored.enabled, false);
    assert.equal(decrypt(restored.apiKey), 'rotated-ts3-key');
    assert.equal(await prisma.botFlow.count({ where: { serverConfigId: ts3.id } }), 1);
    await prepareInstallerAdmin('replacement', 'OtherPassword123!');
    await applyInstallerAdmin(prisma);
    assert.equal((await prisma.user.findFirstOrThrow()).username, 'owner');
  } finally {
    process.chdir(original);
    await prisma.$disconnect();
    await fs.rm(directory, { recursive: true, force: true });
  }
});
