import { test } from 'node:test';
import assert from 'node:assert/strict';
import { getNeededServerPairs, musicBotClidsOnServer, refreshBotChannels } from './channel-ownership.js';
import type { CommandContext } from './context.js';

function contextWith(bots: Record<number, Record<string, unknown>>) {
  const synced: string[] = [];
  const context = {
    prisma: { musicBot: { findUnique: async ({ where }: any) => bots[where.id] ?? null } },
    botChannelConfig: new Map(),
    channelToBots: new Map(),
    unregisterBotChannels: (id: number) => { context.botChannelConfig.delete(id); },
    syncCommandListenersForPair: async (configId: number, sid: number) => { synced.push(`${configId}:${sid}`); },
    syncMusicSessionOwnership: async () => {},
    voiceBotManager: { getBot: (id: number) => ({ status: 'playing', ts3ClientId: 40 + id }) },
  } as unknown as CommandContext;
  return { context, synced };
}

test('an external bot never shares the connection scope used by ServerQuery events', async () => {
  const { context, synced } = contextWith({
    1: { id: 1, serverConfigId: 1, virtualServerId: 1, defaultChannel: null, commandChannelIds: null, externalHost: null },
    3: { id: 3, serverConfigId: 1, virtualServerId: 1, defaultChannel: 'Lobby', commandChannelIds: '["7"]', externalHost: '25krs' },
  });
  await refreshBotChannels(context, 1);
  await refreshBotChannels(context, 3);

  const external = context.botChannelConfig.get(3)!;
  assert.ok(external.virtualServerId < 1);
  assert.deepEqual(external.commandChannelIds, []);
  assert.equal(context.channelToBots.size, 0);
  assert.deepEqual(getNeededServerPairs(context), ['1:1']);
  // The external bot's client id belongs to another server and must not hide a local human.
  assert.deepEqual([...musicBotClidsOnServer(context, 1, 1)], [41]);
  assert.equal(context.botChannelConfig.get(1)!.virtualServerId, 1);
  assert.ok(synced.includes('1:1'));
});
