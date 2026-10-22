import path from 'node:path';
import { createHash } from 'node:crypto';
import { AppError } from '../middleware/error-handler.js';

export function channelFilePath(value: unknown, allowRoot = false): string {
  if (typeof value !== 'string' || !value.startsWith('/') || Buffer.byteLength(value) > 1024 || /[\\\x00-\x1f\x7f]/.test(value)) {
    throw new AppError(400, 'Invalid channel file path');
  }
  if (allowRoot && value === '/') return value;
  if (value.split('/').slice(1).some(p => !p || p === '.' || p === '..')) throw new AppError(400, 'Invalid channel file path');
  return value;
}

/** Only retried after an upstream invalid-path refusal; never overwrite a collision. */
export function compatibleChannelFilePath(value: string): string | null {
  const leaf = path.posix.basename(value);
  let safe = leaf.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').replace(/ı/g, 'i').replace(/İ/g, 'I').replace(/[^\x20-\x7e]/g, '_');
  if (!/[a-z0-9]/i.test(safe)) safe = `dosya-${createHash('sha256').update(leaf).digest('hex').slice(0, 12)}`;
  return safe === leaf ? null : path.posix.join(path.posix.dirname(value), safe);
}
