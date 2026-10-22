import assert from 'node:assert/strict';
import { test } from 'node:test';
import express from 'express';
import net from 'node:net';
import path from 'node:path';
import { parseQueryResponse, tsEscape } from '@ts6/common';
import { fileRoutes } from './files.routes.js';
import { errorHandler } from '../middleware/error-handler.js';
import { channelFilePath, compatibleChannelFilePath } from '../utils/channel-file-path.js';

async function fixture(role = 'admin') {
  const files = new Map<string, Buffer>(); const folders = new Set(['/']); const commands: string[] = []; const audits: any[] = [];
  const transfers = new Map<string, { kind: string; name: string; size: number }>();
  const ft = net.createServer(socket => {
    let buffer = Buffer.alloc(0); let transfer: { kind: string; name: string; size: number } | undefined;
    socket.on('data', chunk => {
      buffer = Buffer.concat([buffer, chunk]);
      if (!transfer && buffer.length >= 32) {
        transfer = transfers.get(buffer.subarray(0, 32).toString()); buffer = buffer.subarray(32);
        if (!transfer) return socket.destroy();
        if (transfer.kind === 'down') return socket.end(files.get(transfer.name));
      }
      if (transfer?.kind === 'up' && buffer.length >= transfer.size) { files.set(transfer.name, buffer.subarray(0, transfer.size)); socket.end(); }
    });
  });
  await new Promise<void>(resolve => ft.listen(0, '127.0.0.1', resolve));
  const port = (ft.address() as net.AddressInfo).port;
  const bridge = { executeCommand: async (_c: number, _s: number, command: string) => {
    commands.push(command);
    const p = parseQueryResponse(command.slice(command.indexOf(' ') + 1))[0];
    const op = command.split(' ')[0];
    if (op === 'ftcreatedir' && /[^\x00-\x7f]/.test(p.dirname)) throw new Error('TS error 2054: invalid file path');
    if (op === 'ftinitupload' && /[^\x00-\x7f]/.test(p.name)) return 'status=2052 msg=file\\sinput/output\\serror';
    if (op === 'ftcreatedir') {
      if (folders.has(p.dirname) || files.has(p.dirname)) throw new Error('TS error 2053: file already exists');
      folders.add(p.dirname); return '';
    }
    if (op === 'ftgetfilelist') {
      if (!folders.has(p.path)) throw new Error('TS error 2054: invalid file path');
      const rows = [...folders].filter(n => n !== '/' && path.posix.dirname(n) === p.path).map(n => ({ name: path.posix.basename(n), size: '0', type: '0' }));
      rows.push(...[...files].filter(([n]) => path.posix.dirname(n) === p.path).map(([n, b]) => ({ name: path.posix.basename(n), size: String(b.length), type: '1' })));
      return rows.map(row => Object.entries(row).map(([k, v]) => `${k}=${tsEscape(v)}`).join(' ')).join('|');
    }
    if (op === 'ftinitupload' || op === 'ftinitdownload') {
      if (op === 'ftinitupload' && files.has(p.name)) { assert.equal(p.overwrite, '0'); return 'status=2053 msg=file\\salready\\sexists'; }
      const data = files.get(p.name);
      if (op === 'ftinitdownload' && !data) throw new Error('TS error 2051: file not found');
      const key = String(transfers.size).padStart(32, 'k'); const size = op === 'ftinitupload' ? Number(p.size) : data!.length;
      transfers.set(key, { kind: op === 'ftinitupload' ? 'up' : 'down', name: p.name, size });
      return `ftkey=${key} port=${port} size=${size} seekpos=0`;
    }
    if (op === 'ftdeletefile') { files.delete(p.name); folders.delete(p.name); return ''; }
    return '';
  }, getSshReconnectPauseSeconds: () => 0 };
  const db: any = {
    tsServerConfig: { findUnique: async () => ({ host: '127.0.0.1', isDemo: false }) },
    adminAuditEvent: { create: async ({data}: any) => { audits.push(data); return {id:'qa'}; }, updateMany: async () => ({count:1}) },
    $transaction: async (fn: any) => fn(db),
  };
  const app = express(); app.use(express.json()); app.locals.prisma = db; app.locals.botEngine = {getEventBridge: () => bridge};
  app.use((req, _res, next) => { req.user = {id: 1, username:'test', role} as any; next(); });
  app.use('/servers/:configId/vs/:sid/files', fileRoutes); app.use(errorHandler);
  const http = app.listen(0, '127.0.0.1'); await new Promise<void>(resolve => http.once('listening', resolve));
  const base = `http://127.0.0.1:${(http.address() as net.AddressInfo).port}/servers/1/vs/1/files/1`;
  return { base, files, folders, commands, audits, close: async () => { await new Promise<void>(resolve => http.close(() => resolve())); await new Promise<void>(resolve => ft.close(() => resolve())); } };
}
function form(fileName: string, data: Uint8Array = Buffer.from('fixture\0dosya\n'), parent = '/') {
  const body = new FormData(); body.append('name', fileName); body.append('path', parent); body.append('file', new Blob([data as BlobPart]), fileName); return body;
}

test('paths reject traversal/control characters and fallback is deterministic', () => {
  for (const bad of ['../x','/../x','/a/./b','/a//b','/a\\b','/a\0b','/']) assert.throws(() => channelFilePath(bad));
  assert.equal(compatibleChannelFilePath('/parent/Türkçe şarkı.txt'), '/parent/Turkce sarki.txt');
  assert.equal(compatibleChannelFilePath('/parent/file.txt'), null);
});

test('mkdir → nested upload → binary download → delete uses real TCP and never overwrites', async () => {
  const f = await fixture();
  try {
    const mkdir = await fetch(`${f.base}/mkdir`, {method:'POST',headers:{'Content-Type':'application/json'}, body:JSON.stringify({dirname:'/Türkçe klasör'})});
    assert.equal(mkdir.status, 201); assert.deepEqual(await mkdir.json(), {success:true,name:'/Turkce klasor',renamed:true});
    const bytes = Buffer.from([0,1,255,34,10,13,78,0]);
    const upload = await fetch(`${f.base}/upload`, {method:'POST',body:form('şarkı.txt', bytes, '/Turkce klasor')});
    assert.equal(upload.status, 201); assert.equal((await upload.json() as any).name, '/Turkce klasor/sarki.txt');
    const again = await fetch(`${f.base}/upload`, {method:'POST',body:form('sarki.txt', Buffer.from('different'), '/Turkce klasor')});
    assert.equal(again.status, 409); assert.deepEqual(f.files.get('/Turkce klasor/sarki.txt'), bytes);
    const download = await fetch(`${f.base}/download?name=${encodeURIComponent('/Turkce klasor/sarki.txt')}`);
    assert.equal(download.status, 200); assert.equal(download.headers.get('x-content-type-options'), 'nosniff');
    assert.match(download.headers.get('content-disposition')!, /attachment/);
    assert.deepEqual(Buffer.from(await download.arrayBuffer()), bytes);
    const remove = await fetch(`${f.base}/file`, {method:'DELETE',headers:{'Content-Type':'application/json'},body:JSON.stringify({name:'/Turkce klasor/sarki.txt'})});
    assert.equal(remove.status, 200); assert.equal(f.files.size, 0);
    assert.ok(f.audits.some(a => a.action === 'channel_file.upload'));
    assert.ok(!JSON.stringify(f.audits).includes('ftkey'));
  } finally { await f.close(); }
});

test('invalid paths, viewer transfers, oversize and absent files do not report success', async () => {
  const f = await fixture(); const viewer = await fixture('viewer');
  try {
    assert.equal((await fetch(`${f.base}/mkdir`, {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({dirname:'/../outside'})})).status, 400);
    assert.equal((await fetch(`${viewer.base}/upload`, {method:'POST',body:form('x.txt')})).status, 403);
    assert.equal((await fetch(`${viewer.base}/download?name=/x.txt`)).status, 403);
    assert.equal((await fetch(`${f.base}/upload`, {method:'POST',body:form('x.txt', new Uint8Array(25*1024*1024+1))})).status, 400);
    assert.equal((await fetch(`${f.base}/download?name=/missing.txt`)).status, 404);
    assert.equal(f.files.size, 0);
  } finally { await f.close(); await viewer.close(); }
});
