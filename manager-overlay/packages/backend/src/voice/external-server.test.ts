import { test } from 'node:test';
import assert from 'node:assert/strict';
import { externalCommandScope, normalizeExternalHost, parseExternalHost, resolveExternalServer } from './external-server.js';

test('accepts nicknames, hosts, IPv4 and IPv6 with optional ports', () => {
  assert.deepEqual(parseExternalHost(' 25KRS '), { host: '25krs', port: null });
  assert.deepEqual(parseExternalHost('ts.example.com:9988'), { host: 'ts.example.com', port: 9988 });
  assert.deepEqual(parseExternalHost('93.177.102.9:9987'), { host: '93.177.102.9', port: 9987 });
  assert.deepEqual(parseExternalHost('[2001:db8::1]:9987'), { host: '2001:db8::1', port: 9987 });
  assert.deepEqual(parseExternalHost('2001:db8::1'), { host: '2001:db8::1', port: null });
  assert.equal(parseExternalHost(''), null);
  assert.equal(parseExternalHost(null), null);
  assert.equal(normalizeExternalHost(' TS.Example.com:9988 '), 'ts.example.com:9988');
  assert.equal(normalizeExternalHost('[2001:db8::1]:9987'), '[2001:db8::1]:9987');
});

test('rejects malformed addresses and ports', () => {
  for (const bad of ['host:0', 'host:65536', 'host:abc', 'bad host', 'a..b', '-x.com', 'x/y', '[nope]:1', 42]) {
    assert.throws(() => parseExternalHost(bad), /Invalid external server/, String(bad));
  }
});

test('a nickname resolves through the myTeamSpeak lookup', async () => {
  let asked = '';
  const target = await resolveExternalServer('25krs', async (url) => { asked = url; return '93.177.102.9:9987\n'; });
  assert.equal(asked, 'https://named.myteamspeak.com/lookup?name=25krs');
  assert.deepEqual(target, { host: '93.177.102.9', port: 9987 });
});

test('an explicit host or port never uses the nickname lookup', async () => {
  const lookup = async () => { throw new Error('should not look up'); };
  assert.deepEqual(await resolveExternalServer('ts.example.com', lookup), { host: 'ts.example.com', port: 9987 });
  assert.deepEqual(await resolveExternalServer('teamspeak:10000', lookup), { host: 'teamspeak', port: 10000 });
});

test('an unknown or unreachable nickname connects to the name itself', async () => {
  assert.deepEqual(await resolveExternalServer('teamhub-ts6-1', async () => ''), { host: 'teamhub-ts6-1', port: 9987 });
  assert.deepEqual(await resolveExternalServer('ts6', async () => { throw new Error('offline'); }), { host: 'ts6', port: 9987 });
});

test('external bots get a command scope no ServerQuery event uses', () => {
  assert.equal(externalCommandScope(3), -3);
  assert.ok(externalCommandScope(1) < 1);
});
