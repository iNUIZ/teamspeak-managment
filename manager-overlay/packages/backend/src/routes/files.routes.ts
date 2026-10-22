import { Router, Request, Response } from 'express';
import multer from 'multer';
import path from 'node:path';
import { randomInt } from 'node:crypto';
import { channelFilePath, compatibleChannelFilePath } from '../utils/channel-file-path.js';
import { downloadTransfer, uploadTransfer } from '../utils/ts-file-transfer.js';
import { actorFromRequest, runRemoteAudited } from '../audit/index.js';
import { requireRole } from '../middleware/rbac.js';
import {
  AppError,
  TSApiError,
  TeamSpeakPermissionError,
  TeamSpeakSshDisconnectedError,
  isTeamSpeakPermissionError,
} from '../middleware/error-handler.js';
import { parseQueryResponse, tsEscape } from '@ts6/common';
import type { BotEngine } from '../bot-engine/engine.js';
import { isSshFatalConnectFailureMessage } from '../bot-engine/ssh-query-client.js';
import {
  FILE_SUMMARY_DEADLINE_MS,
  FILE_SUMMARY_MAX_CHANNELS,
  FILE_SUMMARY_MAX_COMMANDS_PER_REQUEST,
  FILE_SUMMARY_MAX_DEPTH,
  FILE_SUMMARY_MAX_ENTRIES_PER_CHANNEL,
  FILE_SUMMARY_MAX_ENTRIES_PER_REQUEST,
  FILE_SUMMARY_MIN_COMMAND_GAP_MS,
  abortSignalFromRequest,
  fileSummaryScanCoordinator,
  selectChannelsForSummaryScan,
} from './file-summary-scan.js';

export const fileRoutes: Router = Router({ mergeParams: true });
export const MAX_CHANNEL_FILE_BYTES = 25 * 1024 * 1024;
fileRoutes.param('cid', (_req, _res, next, value) => {
  if (!/^[1-9]\d*$/.test(value) || !Number.isSafeInteger(Number(value))) return next(new AppError(400, 'Invalid channel file path'));
  next();
});

// Bound multipart memory and TCP sockets before parsing the request body.
const activeTransfers = new Set<string>();
function transferSlot(req: Request, res: Response, next: Function) {
  const key = `${getConfigId(req)}:${getSid(req)}`;
  if (activeTransfers.has(key) || activeTransfers.size >= 4) return next(new AppError(429, 'Another file transfer is in progress'));
  activeTransfers.add(key);
  const release = () => activeTransfers.delete(key);
  res.once('finish', release); res.once('close', release);
  next();
}
const fileUpload = multer({ storage: multer.memoryStorage(), preservePath: true,
  limits: { fileSize: MAX_CHANNEL_FILE_BYTES, files: 1, fields: 3, fieldSize: 2048 } }).single('file');

async function transferTarget(req: Request, start: Record<string, string> | undefined) {
  const server = await req.app.locals.prisma.tsServerConfig.findUnique({ where: { id: getConfigId(req) }, select: { host: true, isDemo: true } });
  if (server?.isDemo) throw new AppError(400, 'File transfers are not available on the demo connection');
  if (start?.status && start.status !== '0') throw new TSApiError(Number(start.status), start.msg || 'file transfer refused');
  const port = Number(start?.port);
  if (!server?.host || !start?.ftkey || /[\r\n\x00]/.test(start.ftkey) || start.ftkey.length > 1024 || !Number.isInteger(port) || port < 1 || port > 65535) throw new AppError(502, 'TeamSpeak did not start the file transfer');
  // Do not trust a returned IP/host: use only the administrator's configured server.
  return { host: server.host, port, ftkey: start.ftkey };
}

async function withCompatibleName<T>(name: string, operation: (name: string) => Promise<T>): Promise<{ name: string; value: T }> {
  try { return { name, value: await operation(name) }; }
  catch (error) {
    const fallback = compatibleChannelFilePath(name);
    // TS6 reports rejected Unicode as 2054 for mkdir, but 2052 for upload.
    // Retry only when the leaf actually changes; ordinary I/O failures are not hidden.
    if (!(error instanceof TSApiError) || ![2054, 2052].includes(error.code) || !fallback) throw error;
    return { name: fallback, value: await operation(fallback) };
  }
}

const getConfigId = (req: Request) => parseInt(String(req.params.configId));
const getSid = (req: Request) => parseInt(String(req.params.sid));

async function listChannelPath(req: Request, cid: number, path: string): Promise<Record<string, string>[]> {
  try {
    return await sshExecute(req, 'ftgetfilelist', { cid: String(cid), cpw: '', path });
  } catch (err) {
    if (err instanceof TSApiError && err.code === 1281) return [];
    throw err;
  }
}

/**
 * Execute a ServerQuery command via the shared SSH connection (EventBridge).
 * Reuses the same SSH session used for bot events — no extra server slots.
 */
function demoFileCommand(command: string, params: Record<string, string>): Record<string, string>[] {
  if (command === 'ftgetfilelist') {
    const path = params.path || '/';
    if (path === '/') {
      return [
        { name: 'README.txt', size: '2048', datetime: '1700000000', type: '1' },
        { name: 'Shared', size: '0', datetime: '1700000100', type: '0' },
      ];
    }
    if (path === '/Shared') {
      return [
        { name: 'sample-notes.txt', size: '4096', datetime: '1700000200', type: '1' },
        { name: 'example-image.png', size: '32768', datetime: '1700000300', type: '1' },
      ];
    }
    return [];
  }

  if (command === 'ftcreatedir' || command === 'ftdeletefile') {
    return [{ success: '1', demo: '1' }];
  }

  return [];
}

export async function sshExecute(
  req: Request,
  command: string,
  params: Record<string, string>,
): Promise<Record<string, string>[]> {
  const configId = getConfigId(req);
  const prisma = req.app.locals.prisma;
  const server = await prisma.tsServerConfig.findUnique({
    where: { id: configId },
    select: { isDemo: true },
  });

  if (server?.isDemo) {
    return demoFileCommand(command, params);
  }

  const engine: BotEngine = req.app.locals.botEngine;
  if (!engine) throw new AppError(503, 'Bot engine not available');

  const bridge = engine.getEventBridge();
  const sid = getSid(req);

  // Build raw ServerQuery command string
  const paramStr = Object.entries(params)
    .map(([k, v]) => `${k}=${tsEscape(v)}`)
    .join(' ');
  const fullCommand = paramStr ? `${command} ${paramStr}` : command;

  let rawResponse: string;
  try {
    rawResponse = await bridge.executeCommand(configId, sid, fullCommand);
  } catch (err: any) {
    // Convert "TS error {code}: {msg}" to TSApiError
    const match = err.message?.match(/^TS error (\d+): (.+)$/);
    if (match) {
      throw new TSApiError(parseInt(match[1]), match[2]);
    }
    throw err;
  }

  if (!rawResponse.trim()) return [];
  return parseQueryResponse(rawResponse);
}

/** Exported for unit tests — maps SSH/flood transport failures for Files routes. */
export function mapFileSshTransportError(
  err: Error,
  purpose: 'browse' | 'changes',
  retryAfterSeconds = 15,
): AppError | null {
  // Query flood on ft* — reconnectable; same 503 path as SSH disconnect.
  if (err instanceof TSApiError && err.code === 524) {
    return new TeamSpeakSshDisconnectedError(purpose, retryAfterSeconds);
  }

  const msg = err.message || '';
  // Permanent missing credentials (must not match reconnectable "SSH not connected" text).
  if (
    /SSH credentials not configured/i.test(msg)
    || (msg.includes('SSH credentials') && !msg.includes('SSH not connected'))
  ) {
    return new AppError(
      400,
      purpose === 'browse'
        ? 'SSH credentials not configured for this server. File browsing requires SSH access because WebQuery HTTP does not support ft* commands.'
        : 'SSH credentials not configured for this server. File changes require SSH access because WebQuery HTTP does not support ft* commands.',
    );
  }
  // Fatal auth / host-key after EventBridge removed the client — not reconnectable.
  if (isSshFatalConnectFailureMessage(msg)) {
    const isHostKey = /host key/i.test(msg);
    return new AppError(
      401,
      isHostKey
        ? 'SSH host key verification failed. Update or clear the pinned host key in server settings, then retry.'
        : 'SSH authentication failed. Check the SSH username and password in server settings.',
      msg,
    );
  }
  if (msg.includes('SSH not connected')) {
    // 503 (not 502): Session is down/reconnecting — often Query flood cooldown after redeploy.
    return new TeamSpeakSshDisconnectedError(purpose, retryAfterSeconds);
  }
  return null;
}

/** Prefer flood/reconnect pause from EventBridge when the SSH client is mid-cooldown. */
function sshDisconnectRetryAfterSeconds(req: Request): number {
  try {
    const engine: BotEngine | undefined = req.app.locals.botEngine;
    const remaining = engine?.getEventBridge()?.getSshReconnectPauseSeconds(
      getConfigId(req),
      getSid(req),
    );
    if (typeof remaining === 'number' && Number.isFinite(remaining) && remaining > 0) {
      return Math.min(300, Math.max(1, Math.ceil(remaining)));
    }
  } catch {
    // Fall through to default.
  }
  return 15;
}

export function mapFileMutationError(err: unknown, actionHint: string, req?: Request): unknown {
  if (err instanceof TSApiError && err.code === 2051) return new AppError(404, 'File not found');
  if (err instanceof TSApiError && err.code === 2054) return new AppError(400, 'The server rejected this file path', 'Use a valid folder and a supported file name.', { reason: 'file_path_invalid', retryable: false });
  if (err instanceof TSApiError && (err.code === 2053 || /already exists|file exists/i.test(err.message))) return new AppError(409, 'A file or folder with this name already exists', undefined, { reason: 'file_exists', retryable: false });
  if (err instanceof TSApiError && isTeamSpeakPermissionError(err)) {
    return new TeamSpeakPermissionError(err.code, err.message, actionHint);
  }
  if (err instanceof TeamSpeakPermissionError) {
    return new TeamSpeakPermissionError(err.tsCode, 'insufficient client permissions', actionHint);
  }
  if (err instanceof Error) {
    const retryAfter = req ? sshDisconnectRetryAfterSeconds(req) : 15;
    const transport = mapFileSshTransportError(err, 'changes', retryAfter);
    if (transport) return transport;
    if (err instanceof TSApiError) return new AppError(502, 'TeamSpeak file operation failed');
    if (!(err instanceof AppError) && /file transfer/i.test(err.message)) return new AppError(502, 'TeamSpeak file operation failed');
    return err;
  }
  return err;
}

function getBridgeGeneration(req: Request): number {
  const engine: BotEngine | undefined = req.app.locals.botEngine;
  if (!engine) return 0;
  try {
    return engine.getEventBridge().getClientCacheGeneration(getConfigId(req), getSid(req));
  } catch {
    return 0;
  }
}

// Recursively summarize file trees for the channel selector (bounded).
fileRoutes.get('/summary', async (req: Request, res: Response, next) => {
  try {
    const rawCids = String(req.query.cids || '');
    const requested = [...new Set(rawCids.split(',').filter(Boolean).map(Number))];
    if (requested.length < 1 || requested.some((cid) => !Number.isInteger(cid) || cid <= 0)) {
      throw new AppError(400, 'Provide at least one valid channel ID');
    }
    // Accept oversized lists: bound the scan set and label the remainder Not scanned.
    // (Previously rejected >256 with 400 — the Files UI could not explain that.)
    const { scanIds, omittedIds } = selectChannelsForSummaryScan(requested, FILE_SUMMARY_MAX_CHANNELS);

    const configId = getConfigId(req);
    const sid = getSid(req);
    const connectionGeneration = getBridgeGeneration(req);
    const cacheGeneration = fileSummaryScanCoordinator.getCacheGeneration(configId, sid);
    const signal = abortSignalFromRequest(req, res);
    const now = Date.now();

    const response = await fileSummaryScanCoordinator.runRequest({
      configId,
      sid,
      cids: scanIds,
      omittedCids: omittedIds,
      connectionGeneration,
      cacheGeneration,
      deadlineAt: now + FILE_SUMMARY_DEADLINE_MS,
      maxCommands: FILE_SUMMARY_MAX_COMMANDS_PER_REQUEST,
      maxEntries: FILE_SUMMARY_MAX_ENTRIES_PER_REQUEST,
      maxDepth: FILE_SUMMARY_MAX_DEPTH,
      maxEntriesPerChannel: FILE_SUMMARY_MAX_ENTRIES_PER_CHANNEL,
      minCommandGapMs: FILE_SUMMARY_MIN_COMMAND_GAP_MS,
      signal,
      listPath: (cid, path) => listChannelPath(req, cid, path),
      getConnectionGeneration: () => getBridgeGeneration(req),
      getCacheGeneration: () => fileSummaryScanCoordinator.getCacheGeneration(configId, sid),
    });

    res.json(response);
  } catch (err) {
    if (err instanceof Error) {
      const mapped = mapFileSshTransportError(err, 'browse', sshDisconnectRetryAfterSeconds(req));
      if (mapped) return next(mapped);
    }
    next(err);
  }
});

// List files in a channel directory
// Uses shared SSH connection because ft* commands are not supported via WebQuery HTTP
fileRoutes.get('/:cid', async (req: Request, res: Response, next) => {
  try {
    const result = await sshExecute(req, 'ftgetfilelist', {
      cid: String(req.params.cid),
      cpw: String(req.query.cpw || ''),
      path: channelFilePath(req.query.path || '/', true),
    });
    res.json(result);
  } catch (err: any) {
    // TS3 error 1281 = database_empty_result → empty directory
    if (err instanceof TSApiError && err.code === 1281) {
      return res.json([]);
    }
    if (err instanceof Error) {
      const mapped = mapFileSshTransportError(err, 'browse', sshDisconnectRetryAfterSeconds(req));
      if (mapped) return next(mapped);
    }
    next(mapFileMutationError(err, 'listing files', req));
  }
});

// Create directory
fileRoutes.post('/:cid/mkdir', requireRole('admin'), async (req: Request, res: Response, next) => {
  try {
    const requested = channelFilePath(req.body.dirname);
    const result = await runRemoteAudited(req.app.locals.prisma, {
      actor: actorFromRequest(req.user), action: 'channel_file.mkdir', connectionId: getConfigId(req), virtualServerId: getSid(req), target: { type: 'channel', id: String(req.params.cid) },
    }, () => withCompatibleName(requested, async (dirname) => {
      await sshExecute(req, 'ftcreatedir', {
      cid: String(req.params.cid),
      cpw: '',
      dirname,
      });
      const rows = await listChannelPath(req, Number(req.params.cid), path.posix.dirname(dirname));
      if (!rows.some(row => row.name === path.posix.basename(dirname) && row.type === '0')) throw new AppError(502, 'The new folder could not be verified');
      return true;
    }));
    fileSummaryScanCoordinator.invalidateChannel(getConfigId(req), getSid(req), Number(req.params.cid));
    res.status(201).json({ success: true, name: result.name, renamed: result.name !== requested });
  } catch (err) {
    next(mapFileMutationError(err, 'creating directories', req));
  }
});

// Delete file
fileRoutes.delete('/:cid/file', requireRole('admin'), async (req: Request, res: Response, next) => {
  try {
    const name = channelFilePath(req.body.name);
    const result = await runRemoteAudited(req.app.locals.prisma, {
      actor: actorFromRequest(req.user), action: 'channel_file.delete', connectionId: getConfigId(req), virtualServerId: getSid(req), target: { type: 'channel', id: String(req.params.cid) },
    }, () => sshExecute(req, 'ftdeletefile', {
      cid: String(req.params.cid),
      cpw: '',
      name,
    }));
    fileSummaryScanCoordinator.invalidateChannel(getConfigId(req), getSid(req), Number(req.params.cid));
    res.json(result);
  } catch (err) {
    next(mapFileMutationError(err, 'deleting files', req));
  }
});

fileRoutes.post('/:cid/upload', requireRole('admin'), transferSlot, (req, res, next) => {
  fileUpload(req, res, (error) => next(error ? new AppError(400, error instanceof multer.MulterError && error.code === 'LIMIT_FILE_SIZE' ? 'File must be at most 25 MB' : 'Upload exactly one file') : undefined));
}, async (req: Request, res: Response, next) => {
  try {
    if (!req.file) throw new AppError(400, 'Upload exactly one file');
    const parent = channelFilePath(req.body.path || '/', true);
    const leaf = req.body.name || req.file.originalname;
    if (typeof leaf !== 'string' || /[/\\]/.test(leaf) || !leaf) throw new AppError(400, 'Invalid channel file path');
    const requested = channelFilePath(`${parent === '/' ? '' : parent}/${leaf}`);
    const controller = new AbortController(); res.once('close', () => controller.abort());
    const result = await runRemoteAudited(req.app.locals.prisma, {
      actor: actorFromRequest(req.user), action: 'channel_file.upload', connectionId: getConfigId(req), virtualServerId: getSid(req), target: { type: 'channel', id: String(req.params.cid) },
    }, async () => {
      const started = await withCompatibleName(requested, async (name) => {
        const [start] = await sshExecute(req, 'ftinitupload', { clientftfid: String(randomInt(1, 65535)), cid: String(req.params.cid), cpw: '', name, size: String(req.file!.size), overwrite: '0', resume: '0' });
        return { start, target: await transferTarget(req, start) };
      });
      await uploadTransfer(started.value.target.host, started.value.target.port, started.value.target.ftkey, req.file!.buffer, controller.signal);
      const rows = await listChannelPath(req, Number(req.params.cid), parent);
      if (!rows.some(row => row.name === path.posix.basename(started.name) && Number(row.size) === req.file!.size && row.type === '1')) throw new AppError(502, 'The uploaded file could not be verified');
      return started.name;
    });
    fileSummaryScanCoordinator.invalidateChannel(getConfigId(req), getSid(req), Number(req.params.cid));
    res.status(201).json({ success: true, name: result, size: req.file.size, renamed: result !== requested });
  } catch (error) { next(mapFileMutationError(error, 'uploading files', req)); }
});

fileRoutes.get('/:cid/download', requireRole('admin'), transferSlot, async (req: Request, res: Response, next) => {
  try {
    const name = channelFilePath(req.query.name);
    const controller = new AbortController(); res.once('close', () => controller.abort());
    const data = await runRemoteAudited(req.app.locals.prisma, {
      actor: actorFromRequest(req.user), action: 'channel_file.download', connectionId: getConfigId(req), virtualServerId: getSid(req), target: { type: 'channel', id: String(req.params.cid) },
    }, async () => {
      const [start] = await sshExecute(req, 'ftinitdownload', { clientftfid: String(randomInt(1, 65535)), cid: String(req.params.cid), cpw: '', name, seekpos: '0' });
      const target = await transferTarget(req, start);
      const size = Number(start.size);
      if (!Number.isSafeInteger(size) || size < 0 || size > MAX_CHANNEL_FILE_BYTES) throw new AppError(400, 'File must be at most 25 MB');
      return downloadTransfer(target.host, target.port, target.ftkey, size, controller.signal);
    });
    res.attachment(path.posix.basename(name));
    res.setHeader('X-Content-Type-Options', 'nosniff'); res.setHeader('Cache-Control', 'no-store');
    res.setHeader('Content-Security-Policy', "default-src 'none'; sandbox");
    res.type('application/octet-stream').send(data);
  } catch (error) { next(mapFileMutationError(error, 'downloading files', req)); }
});
