import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { afterEach, test } from 'node:test';
import { getCookieArgs, getYtCookieFile, setYtCookieFile, setYouTubeAuthMode } from './youtube.js';
import { poTokenArgs, poTokenBaseUrl, poTokenStatus } from './youtube-pot.js';

afterEach(() => { setYtCookieFile(null); setYouTubeAuthMode('po-token'); });

test('PO Token is the default even if a cookie file was saved earlier', () => {
  setYtCookieFile('private-cookie-file.txt');
  assert.equal(getYtCookieFile(), null);
  const args = getCookieArgs();
  assert.ok(args.includes('youtube:player_client=mweb'));
  assert.ok(args.includes('--no-cookies'));
  assert.ok(args.includes('--no-cookies-from-browser'));
  assert.ok(!args.includes('--cookies'));
  assert.ok(!args.includes('--remote-components'), 'no unpinned runtime component downloads');
});

test('account cookies require explicit opt-in and switching back keeps them unused', () => {
  setYtCookieFile('private-cookie-file.txt');
  setYouTubeAuthMode('cookies');
  assert.equal(getYtCookieFile(), 'private-cookie-file.txt');
  assert.ok(getCookieArgs().includes('--cookies'));
  assert.ok(!getCookieArgs().includes('--no-cookies'));
  setYouTubeAuthMode('po-token');
  assert.equal(getYtCookieFile(), null);
  assert.ok(!getCookieArgs().includes('private-cookie-file.txt'));
});

test('provider accepts only a loopback origin, without credentials, redirects or URL paths', () => {
  assert.equal(poTokenBaseUrl({}), 'http://127.0.0.1:4416');
  for (const url of ['https://example.com', 'http://localhost:4416', 'http://127.0.0.1:4416/x',
    'http://user:pass@127.0.0.1:4416', 'http://127.0.0.1:4416?x=y', 'http://127.0.0.1:4416/#x']) {
    assert.throws(() => poTokenBaseUrl({ YOUTUBE_POT_URL: url }));
  }
});

test('only explicitly configured trusted plugin directory is enabled', () => {
  assert.deepEqual(poTokenArgs({}), []);
  assert.throws(() => poTokenArgs({ YOUTUBE_POT_PLUGIN_DIR: 'relative-path' }));
  assert.deepEqual(poTokenArgs({ YOUTUBE_POT_PLUGIN_DIR: tmpdir() }), [
    '--plugin-dirs', tmpdir(), '--extractor-args', 'youtubepot-bgutilhttp:base_url=http://127.0.0.1:4416',
  ]);
});

test('readiness checks version without exposing upstream data or following redirects', async () => {
  let body: unknown = { version: '2.0.2', privateToken: 'must-not-leak' };
  const server = createServer((_req, res) => { res.setHeader('content-type', 'application/json'); res.end(JSON.stringify(body)); });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const addr = server.address();
  assert.ok(addr && typeof addr !== 'string');
  const env = { YOUTUBE_POT_PLUGIN_DIR: tmpdir(), YOUTUBE_POT_URL: `http://127.0.0.1:${addr.port}` };
  try {
    assert.deepEqual(await poTokenStatus(env), { configured: true, ready: true, version: '2.0.2' });
    body = { version: 'wrong', privateToken: 'must-not-leak' };
    assert.deepEqual(await poTokenStatus(env), { configured: true, ready: false, version: null });
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
  assert.deepEqual(await poTokenStatus({}), { configured: false, ready: false, version: null });
});
