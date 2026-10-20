import type { PrismaClient } from '../../generated/prisma/index.js';

export const botExtensionsKey = (botId: number) => {
  if (!Number.isSafeInteger(botId) || botId <= 0) throw new Error('Invalid bot ID');
  return `bot_extensions:${botId}`;
};
const aliases: Record<string, string> = { come: 'here', pl: 'playlist', next: 'skip', volume: 'vol', nowplaying: 'np', add: 'queue', iptv: 'tv' };
export const canonicalBotCommand = (name: string) => aliases[name] ?? name;
export function parseBotExtensions(value: unknown): { disabledCommands: string[] } {
  const commands = (value as { disabledCommands?: unknown } | null)?.disabledCommands;
  if (!Array.isArray(commands) || commands.length > 256 || commands.some(c => typeof c !== 'string' || !/^[a-z0-9_-]{1,32}$/.test(c))) throw new Error('Invalid bot command settings');
  return { disabledCommands: [...new Set(commands.map(canonicalBotCommand))] };
}
export async function readBotExtensions(prisma: PrismaClient, botId: number) {
  const row = await prisma.appSetting.findUnique({ where: { key: botExtensionsKey(botId) } });
  return row ? parseBotExtensions(JSON.parse(row.value)) : { disabledCommands: [] };
}
