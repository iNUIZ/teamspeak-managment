import fs from 'node:fs/promises';
import { PrismaClient } from '../../generated/prisma/index.js';
import { prepareInstallerAdmin, syncInstallerConnection } from './installer.js';
import { serverAccessSummary } from './server-access.js';

const prisma = new PrismaClient();
async function main() {
  const action = process.argv[2];
  if (action === 'admin-usernames') {
    const users = await prisma.user.findMany({ where: { role: 'admin', enabled: true }, select: { username: true } });
    console.log(users.map(u => u.username).join(', '));
    return;
  }
  if (action === 'admin-state') {
    // Missing schema means this is a new database; other database failures are errors.
    let existing = false;
    try { existing = await prisma.user.count() > 0; }
    catch (error: any) { if (error.code !== 'P2021') throw error; }
    const pending = await fs.access('data/.installer-admin.json').then(() => true, () => false);
    console.log(existing ? 'existing' : pending ? 'prepared' : 'new');
    return;
  }
  let input = '';
  process.stdin.setEncoding('utf8');
  for await (const chunk of process.stdin) {
    input += chunk.toString('utf8');
    if (Buffer.byteLength(input) > 16384) throw new Error('Installer input too large');
  }
  const fields = input.split('\0');
  if (action === 'prepare-admin') {
    await prepareInstallerAdmin(fields[0], fields[1]);
    console.log('prepared');
  } else if (action === 'sync-connection') {
    const changed = await syncInstallerConnection(prisma, fields[0], fields[1], fields[2], fields[3] || undefined);
    console.log(changed ? 'changed' : 'unchanged');
  } else if (action === 'server-access') {
    for (const line of await serverAccessSummary(fields[0], fields[1], fields[2] || undefined)) console.log(line);
  } else throw new Error('Unknown installer action');
}
main().catch(() => {
  // Never expose Prisma errors containing input secrets.
  console.error('Installer configuration failed; existing data has been preserved.');
  process.exitCode = 1;
}).finally(() => prisma.$disconnect());
