import { test } from 'node:test';
import assert from 'node:assert/strict';
import { checkMediaCommand } from './media-access.js';
import type { CommandContext } from './context.js';
import type { VoiceBot } from '../voice-bot.js';

function setup(policy: unknown) {
  const queries: string[] = [];
  const replies: string[] = [];
  const context = {
    botChannelConfig: new Map([[4, { serverConfigId: 1, virtualServerId: -4, defaultChannel: null, commandChannelIds: [] }]]),
    prisma: {
      musicBot: { findUnique: async () => ({ serverConfigId: 1, virtualServerId: 1, externalHost: '25krs' }) },
      appSetting: { findUnique: async ({ where }: any) => {
        assert.equal(where.key, 'media_command_permissions:1:1');
        return policy ? { key: where.key, value: JSON.stringify(policy) } : null;
      } },
    },
    eventBridge: { executeCommand: async (_c: number, _s: number, cmd: string) => { queries.push(cmd); return ''; } },
    virtualServerIdForBot: () => -4,
  } as unknown as CommandContext;
  const bot = {
    floodHoldActive: false,
    currentConfig: { id: 4, serverConfigId: 1 },
    sendTextMessage: (_clid: number, msg: string) => { replies.push(msg); },
  } as unknown as VoiceBot;
  return { context, bot, queries, replies };
}

test('an external bot uses its connection policy, so the default lets everyone play', async () => {
  const { context, bot, queries } = setup(null);
  assert.equal(await checkMediaCommand(context, bot, 155, 'play', 'tarkan'), true);
  assert.deepEqual(queries, []);
});

test('a restricted policy never looks up a client of the external server on the connection', async () => {
  const restricted = {
    playback: { mode: 'server_groups', serverGroupIds: [6] },
    queue: { mode: 'server_groups', serverGroupIds: [6] },
    video: { mode: 'server_groups', serverGroupIds: [6] },
  };
  const { context, bot, queries, replies } = setup(restricted);
  assert.equal(await checkMediaCommand(context, bot, 155, 'play', 'tarkan'), false);
  assert.deepEqual(queries, []);
  assert.deepEqual(replies, ['You are not allowed to use this command.']);
});
