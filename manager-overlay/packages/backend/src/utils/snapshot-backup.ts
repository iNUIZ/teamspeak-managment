import { randomBytes, scrypt, createCipheriv } from 'node:crypto';
import { promisify } from 'node:util';
const derive = promisify(scrypt);
/** At-rest encryption; the password is never written to disk or process argv. */
export async function encryptSnapshot(payload: unknown, password: string) {
  if (typeof password !== 'string' || password.length < 16 || password.length > 128) throw new Error('Backup password must be 16–128 characters');
  const salt = randomBytes(16); const nonce = randomBytes(12);
  const key = await derive(password, salt, 32) as Buffer;
  try {
    const cipher = createCipheriv('aes-256-gcm', key, nonce);
    cipher.setAAD(Buffer.from('teamspeak-manager-snapshot-v1'));
    const data = Buffer.concat([cipher.update(JSON.stringify(payload), 'utf8'), cipher.final()]);
    return { format: 'teamspeak-manager-snapshot-v1', algorithm: 'aes-256-gcm', kdf: 'scrypt',
      salt: salt.toString('base64'), nonce: nonce.toString('base64'), tag: cipher.getAuthTag().toString('base64'), data: data.toString('base64') };
  } finally { key.fill(0); }
}
