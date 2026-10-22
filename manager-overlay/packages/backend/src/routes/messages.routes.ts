import { Router, Request, Response } from 'express';
import type { ConnectionPool } from '../ts-client/connection-pool.js';
import { requireRole } from '../middleware/rbac.js';
import { AppError } from '../middleware/error-handler.js';
import { actorFromRequest, runRemoteAudited } from '../audit/index.js';

export const messageRoutes: Router = Router({ mergeParams: true });

const getClient = (req: Request) => {
  const pool: ConnectionPool = req.app.locals.connectionPool;
  return pool.getClient(parseInt(String(req.params.configId)));
};
const getSid = (req: Request) => parseInt(String(req.params.sid));

messageRoutes.get('/', async (req: Request, res: Response, next) => {
  try { res.json(await getClient(req).execute(getSid(req), 'messagelist')); } catch (err) { next(err); }
});
messageRoutes.get('/sender', requireRole('admin'), async (req, res, next) => {
  try {
    const result = await getClient(req).execute(getSid(req), 'whoami');
    const sender = Array.isArray(result) ? result[0] : result;
    res.json({ nickname: sender?.client_nickname || null, loginName: sender?.client_login_name || null });
  } catch (error) { next(error); }
});

messageRoutes.get('/:msgid', async (req: Request, res: Response, next) => {
  try { res.json(await getClient(req).execute(getSid(req), 'messageget', { msgid: String(req.params.msgid) })); } catch (err) { next(err); }
});

// M1: Write operations require admin role
messageRoutes.post('/', requireRole('admin'), async (req: Request, res: Response, next) => {
  try {
    const { mode = 'offline', cluid, clid, subject, message, expectedUid } = req.body || {};
    if (mode !== 'offline' && mode !== 'chat') throw new AppError(400, 'Invalid message delivery mode');
    if (typeof message !== 'string' || !message.trim() || message.length > 8192) throw new AppError(400, 'Message must contain between 1 and 8192 characters');
    if (mode === 'chat' && (!Number.isSafeInteger(clid) || clid < 1)) throw new AppError(400, 'Select an online recipient');
    if (expectedUid !== undefined && (typeof expectedUid !== 'string' || !/^[A-Za-z0-9+/=]{16,128}$/.test(expectedUid))) throw new AppError(400, 'Invalid recipient UID');
    if (mode === 'offline' && (typeof cluid !== 'string' || !/^[A-Za-z0-9+/=]{16,128}$/.test(cluid) || typeof subject !== 'string' || !subject.trim() || subject.length > 255)) {
      throw new AppError(400, 'A valid recipient UID and subject are required');
    }
    const client = getClient(req); const sid = getSid(req);
    await runRemoteAudited(req.app.locals.prisma, {
      actor: actorFromRequest(req.user), action: 'client.message',
      connectionId: Number(req.params.configId), virtualServerId: sid,
      target: { type: 'client', ...(mode === 'chat' ? { id: clid } : {}) },
    }, async () => {
      if (mode === 'chat') {
        const rows = await client.execute(sid, 'clientinfo', { clid });
        const recipient = Array.isArray(rows) ? rows[0] : rows;
        if (expectedUid && recipient?.client_unique_identifier !== expectedUid) throw new AppError(409, 'Recipient changed. Refresh the recipient list before sending.');
        return client.execute(sid, 'sendtextmessage', { targetmode: 1, target: clid, msg: message });
      }
      return client.execute(sid, 'messageadd', { cluid, subject, message });
    });
    res.status(201).json({ accepted: true, mode });
  } catch (err) { next(err); }
});

messageRoutes.delete('/:msgid', requireRole('admin'), async (req: Request, res: Response, next) => {
  try { res.json(await getClient(req).execute(getSid(req), 'messagedel', { msgid: String(req.params.msgid) })); } catch (err) { next(err); }
});
