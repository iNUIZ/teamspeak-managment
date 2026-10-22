import assert from 'node:assert/strict';
import fs from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'node:test';
import { getCookieArgs } from './youtube.js';
import { getYtCookieFile, setYtCookieFile, requireYouTubeCookies } from './youtube-cookies.js';
import { spawnYtDlp, YT_DLP_EGRESS_ARGS } from './yt-dlp-process.js';
import { mapOperationalError } from '../../middleware/error-handler.js';

afterEach(() => setYtCookieFile(null));

test('no cookie blocks YouTube before starting any downloader or egress job', async () => {
  for (const target of ['ytsearch3:test', 'https://youtu.be/abcdefghijk', 'https://music.youtube.com/watch?v=abcdefghijk']) {
    await assert.rejects(spawnYtDlp(['--', target], { target, timeoutMs: 100 }), /YouTube cookies required/);
  }
  const mapped = mapOperationalError(new Error('YouTube cookies required. Upload first.'));
  assert.equal(mapped?.reason, 'media_auth_required');
  assert.equal(mapped?.statusCode, 422);
});

test('upload is immediately active, missing/empty files are not usable and removal never falls back', () => {
  const dir = fs.mkdtempSync(path.join(tmpdir(), 'teamhub-cookie-test-'));
  const file = path.join(dir, 'cookies.txt');
  try {
    fs.writeFileSync(file, '# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\ttest\tfixture\n');
    setYtCookieFile(file);
    assert.equal(getYtCookieFile(), file);
    const args = getCookieArgs();
    assert.ok(args.includes('youtube:player_client=default'));
    assert.doesNotThrow(() => requireYouTubeCookies('ytsearch1:test', args));
    assert.throws(() => requireYouTubeCookies('ytsearch1:test', ['--cookies', 'another-file']), /cookies required/);
    fs.writeFileSync(file, ''); assert.equal(getYtCookieFile(), null);
    fs.unlinkSync(file); assert.equal(getYtCookieFile(), null);
    setYtCookieFile(dir); assert.equal(getYtCookieFile(), null);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('non-YouTube media does not require cookies and plugins remain disabled', () => {
  assert.doesNotThrow(() => requireYouTubeCookies('https://radio.example/live.mp3', []));
  assert.doesNotThrow(() => requireYouTubeCookies('https://youtube.com.example/media', []));
  assert.ok(YT_DLP_EGRESS_ARGS.includes('--no-plugin-dirs'));
});
