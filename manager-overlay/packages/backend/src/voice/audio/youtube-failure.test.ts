import assert from 'node:assert/strict';
import { test } from 'node:test';
import { summarizeYtDlpStderr } from './youtube.js';
import { mapOperationalError } from '../../middleware/error-handler.js';
test('code-zero empty formats preserves provider verification instead of benign stdin notice', () => {
  const stderr = 'Reading options from STDIN - EOF (Ctrl+D) to end:\nWARNING: [youtube] Sign in to confirm you’re not a bot.\nERROR: Requested format is not available';
  const summary = summarizeYtDlpStderr(stderr);
  assert.match(summary, /Sign in to confirm/); assert.doesNotMatch(summary, /STDIN/);
  assert.equal(mapOperationalError(new Error(`yt-dlp info failed (code 0): ${summary}`))?.reason, 'media_auth_required');
});
test('other failures remain useful, bounded and generic when stderr is empty', () => {
  assert.equal(summarizeYtDlpStderr('WARNING: noise\nERROR: unavailable'), 'ERROR: unavailable');
  assert.equal(summarizeYtDlpStderr('Reading options from STDIN'), 'unknown yt-dlp error');
  assert.equal(summarizeYtDlpStderr('x'.repeat(500)).length, 280);
});
