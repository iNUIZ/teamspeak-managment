import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Ts3Client } from './client.js';

function feed(client: Ts3Client, cmdStr: string) {
  (client as any).processCommand(Buffer.from(cmdStr, 'utf-8'));
}

test('tracks the channel of every client in view, not only the own channel', () => {
  const client = new Ts3Client();
  (client as any).clientId = 9;
  (client as any).currentChannelId = 10;
  (client as any).sendCommand = () => {};
  feed(client, 'notifycliententerview clid=3 ctid=20 client_type=0');
  assert.equal(client.getClientChannelId(3), 20);
  feed(client, 'notifyclientmoved clid=3 cfid=20 ctid=30');
  assert.equal(client.getClientChannelId(3), 30);
  feed(client, 'clientlist clid=4 cid=40 client_type=0|clid=5 cid=10 client_type=0');
  assert.equal(client.getClientChannelId(4), 40);
  feed(client, 'notifyclientleftview clid=3');
  assert.equal(client.getClientChannelId(3), 0);
});

test('lists channels with parents and follows renames and deletes', () => {
  const client = new Ts3Client();
  feed(client, 'channellist cid=1 cpid=0 channel_name=Lobby channel_order=0|cid=2 cpid=1 channel_name=Müzik channel_order=1');
  assert.deepEqual(client.listChannels(), [
    { cid: 1, name: 'Lobby', parentId: 0, order: 0 },
    { cid: 2, name: 'Müzik', parentId: 1, order: 1 },
  ]);
  feed(client, 'notifychanneledited cid=2 channel_name=Oyun');
  feed(client, 'notifychanneldeleted cid=1');
  assert.deepEqual(client.listChannels().map((c) => c.name), ['Oyun']);
});
