import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test('authored static interface copy has Turkish catalog coverage and translation boundaries',()=>{
  const output=execFileSync(process.execPath,['--import','tsx','scripts/i18n-inventory.ts','--check'],{
    cwd:fileURLToPath(new URL('../../',import.meta.url)),encoding:'utf8',maxBuffer:1024*1024,
  });
  assert.match(output,/Turkish catalog coverage passed/);
});
