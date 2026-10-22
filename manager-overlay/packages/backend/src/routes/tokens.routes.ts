import { Router, Request, Response } from 'express';
import { requireRole } from '../middleware/rbac.js';
import type { ConnectionPool } from '../ts-client/connection-pool.js';
import { actorFromRequest, runRemoteAudited } from '../audit/index.js';
import { AppError } from '../middleware/error-handler.js';
import { keyFingerprint } from '../audit/privilege-key-history.js';

export const tokenRoutes: Router = Router({ mergeParams: true });

const getClient = (req: Request) => {
  const pool: ConnectionPool = req.app.locals.connectionPool;
  return pool.getClient(parseInt(String(req.params.configId)));
};
const getSid = (req: Request) => parseInt(String(req.params.sid));
const getConnectionId = (req: Request) => parseInt(String(req.params.configId), 10);

tokenRoutes.get('/history', requireRole('admin'), async (req, res, next) => {
  try { res.json(await req.app.locals.privilegeKeyHistory.list(getConnectionId(req), getSid(req))); } catch (error) { next(error); }
});
tokenRoutes.put('/history', requireRole('admin'), async (req, res, next) => {
  try {
    if (typeof req.body?.enabled !== 'boolean') throw new AppError(400, 'Invalid capture setting');
    res.json(await req.app.locals.privilegeKeyHistory.setEnabled(getConnectionId(req), getSid(req), req.body.enabled));
  } catch (error) { next(error); }
});

tokenRoutes.get('/', requireRole('admin'), async (req: Request, res: Response, next) => {
  try { const rows = await getClient(req).execute(getSid(req), 'privilegekeylist'); res.json(Array.isArray(rows) ? rows.map(row => ({ ...row, token_fingerprint: typeof row.token === 'string' ? keyFingerprint(row.token) : null })) : rows); } catch (err) { next(err); }
});

tokenRoutes.post('/', requireRole('admin'), async (req: Request, res: Response, next) => {
  try {
    const prisma = req.app.locals.prisma;
    const sid = getSid(req);
    const { tokentype, tokenid1, tokenid2 = 0, tokendescription = '' } = req.body;
    if (![0, 1].includes(tokentype) || !Number.isSafeInteger(tokenid1) || tokenid1 < 1
      || !Number.isSafeInteger(tokenid2) || tokenid2 < 0 || (tokentype === 1 && tokenid2 < 1)
      || typeof tokendescription !== 'string' || tokendescription.length > 255) throw new AppError(400, 'Invalid privilege key parameters');
    // Never audit body or resolve token string from TeamSpeak response
    const result = await runRemoteAudited(
      prisma,
      {
        actor: actorFromRequest(req.user),
        action: 'privilege_key.create',
        connectionId: getConnectionId(req),
        virtualServerId: sid,
        target: { type: 'privilege_key' },
      },
      () => getClient(req).execute(sid, 'privilegekeyadd', { tokentype, tokenid1, tokenid2, tokendescription }),
    );
    res.status(201).json(result);
  } catch (err) { next(err); }
});

tokenRoutes.delete('/:token', requireRole('admin'), async (req: Request, res: Response, next) => {
  try {
    const prisma = req.app.locals.prisma;
    const sid = getSid(req);
    const token = String(req.params.token);
    // Token is a secret in the URL path — never store it as targetId
    const result = await runRemoteAudited(
      prisma,
      {
        actor: actorFromRequest(req.user),
        action: 'privilege_key.delete',
        connectionId: getConnectionId(req),
        virtualServerId: sid,
        target: { type: 'privilege_key' },
      },
      () => getClient(req).execute(sid, 'privilegekeydelete', { token }),
    );
    res.json(result);
  } catch (err) { next(err); }
});
