import type { PrismaClient } from '../../generated/prisma/index.js';
import { DEFAULT_BOT_AUTOMATION_SETTINGS, type BotAutomationSettings } from '@ts6/common';

/** The part of VoiceBotManager this needs, so tests can pass a fake. */
export interface DefaultBotCreator {
  createBot(data: {
    name: string;
    serverConfigId: number;
    nickname?: string;
    autoStart?: boolean;
    connectAfterCreate?: boolean;
    avatarMode?: 'default';
  }): Promise<{ id: number }>;
  getBot(id: number): { automation?: { configure(settings: BotAutomationSettings): Promise<void> | void } } | undefined;
}

export const DEFAULT_MUSIC_BOT = 'Müzik Botu';
export const DEFAULT_RADIO_BOT = 'Radyo Botu';

/**
 * A new server comes with two ready bots: a music bot and a radio bot. Both take the
 * same chat commands, so the radio bot also plays requests (`!play`) and goes back to
 * its station when the queue is empty.
 *
 * Runs once per connection, and only when that connection has no bots yet, so an
 * existing install or a bot deleted on purpose is never re-created.
 */
export async function seedDefaultBots(
  prisma: PrismaClient, manager: DefaultBotCreator, serverConfigId: number,
): Promise<number[]> {
  const key = `default-bots-v1:${serverConfigId}`;
  if (await prisma.appSetting.findUnique({ where: { key } })) return [];
  await prisma.appSetting.create({ data: { key, value: 'seeded' } });
  if (await prisma.musicBot.count({ where: { serverConfigId } }) > 0) return [];

  const created: number[] = [];
  const music = await manager.createBot({
    name: DEFAULT_MUSIC_BOT, nickname: DEFAULT_MUSIC_BOT, serverConfigId, autoStart: true, avatarMode: 'default',
  });
  created.push(music.id);
  await saveAutomation(prisma, manager, music.id, { ...DEFAULT_BOT_AUTOMATION_SETTINGS, stayInChannel: true });

  const radio = await manager.createBot({
    name: DEFAULT_RADIO_BOT, nickname: DEFAULT_RADIO_BOT, serverConfigId, autoStart: true, avatarMode: 'default',
  });
  created.push(radio.id);
  const station = await prisma.radioStation.findFirst({ where: { serverConfigId }, orderBy: { id: 'asc' } });
  await saveAutomation(prisma, manager, radio.id, {
    ...DEFAULT_BOT_AUTOMATION_SETTINGS,
    stayInChannel: true,
    ...(station && { enabled: true, source: { kind: 'radio', id: station.id }, quietPeriodSeconds: 30 }),
  });
  return created;
}

async function saveAutomation(
  prisma: PrismaClient, manager: DefaultBotCreator, botId: number, settings: BotAutomationSettings,
): Promise<void> {
  await prisma.musicBot.update({ where: { id: botId }, data: { automationSettings: JSON.stringify(settings) } });
  await manager.getBot(botId)?.automation?.configure(settings);
}
