/**
 * TeamSpeak file transfer: a ServerQuery `ftinitupload` / `ftinitdownload`
 * returns a key and port, and the bytes then travel over a separate TCP
 * connection to that port (30033 by default).
 */
import net from 'node:net';

const TRANSFER_TIMEOUT_MS = 15_000;

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

/** Unsigned CRC32; TeamSpeak names an icon file `icon_<crc32 of its bytes>`. */
export function crc32(data: Buffer): number {
  let crc = 0xffffffff;
  for (const byte of data) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

export function uploadTransfer(host: string, port: number, ftkey: string, data: Buffer, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return Promise.reject(new Error('File transfer cancelled'));
  const expected = Buffer.byteLength(ftkey) + data.length;
  return new Promise((resolve, reject) => {
    const socket = net.connect(port, host, () => { socket.write(ftkey); socket.end(data); });
    const abort = () => socket.destroy(new Error('File transfer cancelled'));
    signal?.addEventListener('abort', abort, { once: true });
    const timer = setTimeout(() => socket.destroy(new Error('TeamSpeak file transfer timed out')), 120_000);
    socket.setTimeout(TRANSFER_TIMEOUT_MS, () => socket.destroy(new Error('TeamSpeak file transfer stalled')));
    socket.on('error', reject);
    socket.on('close', (hadError) => {
      clearTimeout(timer); signal?.removeEventListener('abort', abort);
      if (hadError) return;
      if (socket.bytesWritten < expected) reject(new Error('TeamSpeak file transfer closed before all bytes were written'));
      else resolve();
    });
  });
}

export function downloadTransfer(host: string, port: number, ftkey: string, size: number, signal?: AbortSignal): Promise<Buffer> {
  if (!Number.isSafeInteger(size) || size < 0) return Promise.reject(new Error('Invalid file transfer size'));
  if (signal?.aborted) return Promise.reject(new Error('File transfer cancelled'));
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let received = 0;
    let done = false;
    const finish = (error: Error | null) => {
      if (done) return;
      done = true;
      clearTimeout(timer); signal?.removeEventListener('abort', abort);
      socket.destroy();
      if (error) reject(error); else resolve(Buffer.concat(chunks, size));
    };
    const socket = net.connect(port, host, () => { socket.write(ftkey); });
    const abort = () => finish(new Error('File transfer cancelled'));
    const timer = setTimeout(() => finish(new Error('TeamSpeak file transfer timed out')), 120_000);
    signal?.addEventListener('abort', abort, { once: true });
    socket.setTimeout(TRANSFER_TIMEOUT_MS, () => finish(new Error('TeamSpeak file transfer stalled')));
    socket.on('data', (chunk: Buffer) => {
      if (received + chunk.length > size) return finish(new Error('TeamSpeak sent more bytes than expected'));
      chunks.push(chunk);
      received += chunk.length;
      if (received >= size) finish(null);
    });
    socket.on('error', (error) => finish(error));
    socket.on('close', () => finish(received >= size ? null : new Error('TeamSpeak file transfer closed before all bytes arrived')));
  });
}
