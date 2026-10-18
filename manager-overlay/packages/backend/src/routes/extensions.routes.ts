import { Router, type Request, type Response } from 'express';
import { z } from 'zod';
import { requireRole } from '../middleware/rbac.js';
import { AppError } from '../middleware/error-handler.js';
import { isReservedChatCommandName, normalizeChatCommandName } from '../voice/chat-commands.js';

export const extensionRoutes: Router = Router({ mergeParams: true });
extensionRoutes.use(requireRole('admin'));

const commandSchema = z.object({
  name: z.string().min(1).max(32),
  description: z.string().max(120).default(''),
  response: z.string().min(1).max(900),
});

const manifestSchema = z.object({
  id: z.string().regex(/^[a-z0-9][a-z0-9-]{1,63}$/),
  name: z.string().min(2).max(80),
  version: z.string().regex(/^\d+\.\d+\.\d+(?:-[a-z0-9.-]+)?$/i),
  description: z.string().max(240).default(''),
  author: z.string().max(80).default('Unknown'),
  commands: z.array(commandSchema).min(1).max(25),
}).strict();

type StoredPack = z.infer<typeof manifestSchema> & { enabled: boolean; commandIds: number[]; installedAt: string };
const keyFor = (configId: number, id: string) => `extension_pack:${configId}:${id}`;

function configId(req: Request): number {
  const value = Number(req.params.configId);
  if (!Number.isSafeInteger(value) || value <= 0) throw new AppError(400, 'A valid server connection is required');
  return value;
}

async function readPacks(prisma: any, id: number): Promise<StoredPack[]> {
  const rows = await prisma.appSetting.findMany({ where: { key: { startsWith: `extension_pack:${id}:` } } });
  return rows.flatMap((row: { value: string }) => {
    try { return [JSON.parse(row.value) as StoredPack]; } catch { return []; }
  }).sort((a: StoredPack, b: StoredPack) => a.name.localeCompare(b.name));
}

extensionRoutes.get('/', async (req: Request, res: Response, next) => {
  try {
    const id = configId(req);
    res.json({
      bundled: [
        { id: 'search', name: 'Smart Search', version: '1.0.0', description: 'Mixed quality-ranked !ara results from YouTube Music, YouTube and Spotify matches.', enabled: true, bundled: true },
        { id: 'custom-commands', name: 'Custom Commands', version: '1.0.0', description: 'Safe server-scoped ! commands managed from the panel.', enabled: true, bundled: true },
      ],
      installed: await readPacks(req.app.locals.prisma, id),
    });
  } catch (err) { next(err); }
});

extensionRoutes.post('/import', async (req: Request, res: Response, next) => {
  try {
    const id = configId(req);
    const parsed = manifestSchema.safeParse(req.body);
    if (!parsed.success) throw new AppError(400, parsed.error.issues[0]?.message || 'Invalid extension manifest');
    const manifest = parsed.data;
    const normalized = manifest.commands.map((command) => ({ ...command, name: normalizeChatCommandName(command.name) }));
    if (normalized.some((command) => !command.name || isReservedChatCommandName(command.name))) {
      throw new AppError(400, 'Extension command names must be valid and cannot override bundled commands');
    }
    if (new Set(normalized.map((command) => command.name)).size !== normalized.length) {
      throw new AppError(400, 'Extension command names must be unique');
    }
    const prisma = req.app.locals.prisma;
    if (await prisma.appSetting.findUnique({ where: { key: keyFor(id, manifest.id) } })) {
      throw new AppError(409, 'This extension is already installed');
    }
    const clashes = await prisma.chatCommand.findMany({
      where: { serverConfigId: id, name: { in: normalized.map((command) => command.name) } },
      select: { name: true },
    });
    if (clashes.length) throw new AppError(409, `Command already exists: !${clashes[0].name}`);

    const stored = await prisma.$transaction(async (tx: any) => {
      const commandIds: number[] = [];
      for (const command of normalized) {
        const created = await tx.chatCommand.create({ data: {
          serverConfigId: id,
          name: command.name,
          description: command.description || `${manifest.name} extension`,
          response: command.response,
          enabled: true,
        } });
        commandIds.push(created.id);
      }
      const pack: StoredPack = { ...manifest, commands: normalized, enabled: true, commandIds, installedAt: new Date().toISOString() };
      await tx.appSetting.create({ data: { key: keyFor(id, manifest.id), value: JSON.stringify(pack) } });
      return pack;
    });
    res.status(201).json(stored);
  } catch (err) { next(err); }
});

extensionRoutes.put('/:packId', async (req: Request, res: Response, next) => {
  try {
    const id = configId(req);
    const row = await req.app.locals.prisma.appSetting.findUnique({ where: { key: keyFor(id, String(req.params.packId)) } });
    if (!row) throw new AppError(404, 'Extension not found');
    const pack = JSON.parse(row.value) as StoredPack;
    const enabled = Boolean(req.body.enabled);
    await req.app.locals.prisma.$transaction([
      req.app.locals.prisma.chatCommand.updateMany({ where: { id: { in: pack.commandIds }, serverConfigId: id }, data: { enabled } }),
      req.app.locals.prisma.appSetting.update({ where: { key: row.key }, data: { value: JSON.stringify({ ...pack, enabled }) } }),
    ]);
    res.json({ ...pack, enabled });
  } catch (err) { next(err); }
});

extensionRoutes.delete('/:packId', async (req: Request, res: Response, next) => {
  try {
    const id = configId(req);
    const key = keyFor(id, String(req.params.packId));
    const row = await req.app.locals.prisma.appSetting.findUnique({ where: { key } });
    if (!row) throw new AppError(404, 'Extension not found');
    const pack = JSON.parse(row.value) as StoredPack;
    await req.app.locals.prisma.$transaction([
      req.app.locals.prisma.chatCommand.deleteMany({ where: { id: { in: pack.commandIds }, serverConfigId: id } }),
      req.app.locals.prisma.appSetting.delete({ where: { key } }),
    ]);
    res.json({ success: true });
  } catch (err) { next(err); }
});
