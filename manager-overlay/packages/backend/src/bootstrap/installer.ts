import fs from 'node:fs/promises';
import path from 'node:path';
import bcrypt from 'bcryptjs';
import type { PrismaClient } from '../../generated/prisma/index.js';
import { encrypt, decrypt } from '../utils/crypto.js';
import { validatePassword } from '../utils/validate-password.js';

const adminFile = path.resolve('data', '.installer-admin.json');

/** Local installer only: credentials arrive on stdin, never in process arguments. */
export async function prepareInstallerAdmin(username: string, password: string): Promise<void> {
  if (!/^[a-zA-Z0-9_.-]{3,64}$/.test(username)) throw new Error('Invalid administrator username');
  const error = validatePassword(password);
  if (error || password.length > 72 || Buffer.byteLength(password, 'utf8') > 72) {
    throw new Error(error || 'Administrator password exceeds 72 bytes');
  }
  const passwordHash = await bcrypt.hash(password, 12);
  await fs.mkdir(path.dirname(adminFile), { recursive: true });
  await fs.writeFile(adminFile, JSON.stringify({ username, passwordHash }), { mode: 0o600, flag: 'wx' });
}

/** Runs before listen(): a new installation never exposes an unclaimed setup page. */
export async function applyInstallerAdmin(prisma: PrismaClient): Promise<void> {
  let source: string;
  try { source = await fs.readFile(adminFile, 'utf8'); }
  catch (error: any) { if (error.code === 'ENOENT') return; throw error; }
  const admin = JSON.parse(source);
  if (!/^[a-zA-Z0-9_.-]{3,64}$/.test(admin.username) || !/^\$2[aby]\$12\$[./A-Za-z0-9]{53}$/.test(admin.passwordHash)) {
    throw new Error('Invalid installer administrator record');
  }
  await prisma.$transaction(async (tx) => {
    if (await tx.user.count() === 0) {
      await tx.user.create({ data: {
        username: admin.username, displayName: admin.username, passwordHash: admin.passwordHash, role: 'admin',
      } });
    }
  });
  await fs.unlink(adminFile);
}

export async function syncInstallerConnection(prisma: PrismaClient, name: string, host: string, apiKey: string): Promise<boolean> {
  if (!name || name.length > 150 || !/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,252}$/.test(host) || !apiKey || apiKey.length > 4096) {
    throw new Error('Invalid installer connection');
  }
  return prisma.$transaction(async (tx) => {
    const existing = await tx.tsServerConfig.findFirst({ where: { host, webqueryPort: 10080, isDemo: false } });
    if (existing) {
      if (decrypt(existing.apiKey) === apiKey) return false;
      await tx.tsServerConfig.update({ where: { id: existing.id }, data: { apiKey: encrypt(apiKey) } });
    } else {
      await tx.tsServerConfig.create({ data: { name, host, webqueryPort: 10080, apiKey: encrypt(apiKey), useHttps: false } });
    }
    return true;
  });
}
