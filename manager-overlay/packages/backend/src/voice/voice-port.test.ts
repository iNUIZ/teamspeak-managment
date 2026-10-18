import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveBotVoicePort } from './voice-port.js';

test('auto port uses the selected virtual server for TS3 and TS6', async () => {
  assert.equal(await resolveBotVoicePort(0, async () => ({ virtualserver_port: '9987' })), 9987);
  assert.equal(await resolveBotVoicePort(0, async () => [{ virtualserver_port: '9988' }]), 9988);
  assert.equal(await resolveBotVoicePort(null, async () => ({ virtualserver_port: '12345' })), 12345);
});
test('an explicit NAT port is preserved and never performs discovery', async () => {
  assert.equal(await resolveBotVoicePort(40000, async () => { throw Error('should not query'); }), 40000);
});
test('missing or invalid port never silently uses an incorrect 9987 default', async () => {
  for (const port of [undefined, 0, -1, 'bad', 65536]) {
    await assert.rejects(resolveBotVoicePort(0, async () => ({ virtualserver_port: port })));
  }
});
