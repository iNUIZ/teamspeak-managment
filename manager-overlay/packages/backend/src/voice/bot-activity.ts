import type { PrismaClient } from '../../generated/prisma/index.js';

/**
 * "Who did what" for each bot: chat commands from TeamSpeak and control actions from the
 * panel. Kept in app settings so it survives restarts. Only the visible TeamSpeak nickname
 * or panel username is stored — no client UID or IP — and spoken/remote text is redacted.
 */
export interface BotActivityEntry {
  at: string;
  via: 'teamspeak' | 'panel';
  who: string;
  action: string;
}

const MAX_ENTRIES = 50;
const MAX_ACTION_LENGTH = 160;
const REDACTED_COMMANDS = new Set(['say', 'remote']);

export function botActivityKey(botId: number): string {
  return `bot_activity:${botId}`;
}

/** Text shown for a chat command; spoken text and remote links never reach the log. */
export function describeChatCommand(command: string, args: string, searchChoice: number | null): string {
  if (searchChoice !== null) return `!ara sonucu #${searchChoice}`;
  if (REDACTED_COMMANDS.has(command)) return `!${command}`;
  const text = `!${command}${args ? ` ${args}` : ''}`;
  return text.length > MAX_ACTION_LENGTH ? `${text.slice(0, MAX_ACTION_LENGTH - 1)}…` : text;
}

function cleanName(who: string | undefined): string {
  const name = (who ?? '').replace(/[\u0000-\u001f]/g, '').trim();
  return name ? name.slice(0, 40) : 'Bilinmeyen';
}

export class BotActivityLog {
  /** One write chain per bot keeps entries ordered without blocking commands. */
  private chains = new Map<number, Promise<void>>();

  constructor(private prisma: PrismaClient, private clock: () => Date = () => new Date()) {}

  record(botId: number, entry: Omit<BotActivityEntry, 'at'>): Promise<void> {
    const item: BotActivityEntry = {
      at: this.clock().toISOString(),
      via: entry.via,
      who: cleanName(entry.who),
      action: entry.action.slice(0, MAX_ACTION_LENGTH),
    };
    const next = (this.chains.get(botId) ?? Promise.resolve()).then(async () => {
      const key = botActivityKey(botId);
      const entries = [item, ...(await this.list(botId))].slice(0, MAX_ENTRIES);
      const value = JSON.stringify(entries);
      await this.prisma.appSetting.upsert({ where: { key }, create: { key, value }, update: { value } });
    }).catch((error: any) => {
      // A log write must never break the command that triggered it.
      console.warn(`[BotActivity] Could not record activity for bot ${botId}: ${error?.message || error}`);
    });
    this.chains.set(botId, next);
    return next;
  }

  async list(botId: number): Promise<BotActivityEntry[]> {
    const row = await this.prisma.appSetting.findUnique({ where: { key: botActivityKey(botId) } });
    if (!row) return [];
    try {
      const parsed = JSON.parse(row.value);
      return Array.isArray(parsed) ? parsed.filter((e) => e && typeof e.action === 'string').slice(0, MAX_ENTRIES) : [];
    } catch { return []; }
  }
}
