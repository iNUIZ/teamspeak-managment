import { test, expect, type Page, type APIRequestContext } from '@playwright/test';
async function login(page: Page, request: APIRequestContext) {
  await request.post('/__test/reset'); await request.post('/__test/dashboard?scenario=normal'); await request.post('/__test/auth?on');
  await page.goto('/login'); await page.getByLabel('Username', { exact: true }).fill('admin'); await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In', exact: true }).click(); await expect(page).toHaveURL('/dashboard');
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click();
}
test('confirmed regex deletion removes row without refresh; refused deletion keeps it', async ({ page, request }) => {
  let fail = false;
  await page.route('**/api/servers/*/vs/*/bans**', async route => {
    if (route.request().method() === 'DELETE') return route.fulfill({ status: fail ? 403 : 200, json: fail ? { error: 'Permission denied' } : {} });
    // Deliberately stale list: a successful delete must not reinsert the old row.
    return route.fulfill({ json: [{ banid: '11', name: '^bad.*', duration: 0 }, { banid: '12', name: '^keep.*', duration: 60 }] });
  });
  await login(page, request); await page.goto('/bans');
  const row = page.getByRole('row').filter({ hasText: '^bad.*' }); await expect(row).toBeVisible(); await row.getByRole('button').last().click(); await expect(row).toHaveCount(0);
  await expect(page.getByText('1 sonuç', { exact: true })).toBeVisible(); fail = true;
  const retained = page.getByRole('row').filter({ hasText: '^keep.*' }); await retained.getByRole('button').last().click(); await expect(retained).toBeVisible();
  await expect(page.getByText('Yetki yetersiz', { exact: true })).toBeVisible();
});
test('named recipient sends live chat; offline mail has distinct fields and truthful acceptance', async ({ page, request }) => {
  const sent: any[] = [];
  await page.route('**/api/servers/*/vs/*/clients', route => route.fulfill({ json: [{ clid: 11, client_type: 0, client_nickname: 'Review Alice', client_unique_identifier: 'abcdefghijklmnopqrstuvwx=' }] }));
  await page.route('**/api/servers/*/vs/*/messages**', route => {
    if (route.request().method() === 'POST') { sent.push(route.request().postDataJSON()); return route.fulfill({ status: 201, json: { accepted: true } }); }
    return route.fulfill({ json: [] });
  });
  await login(page, request); await page.goto('/messages'); await page.getByRole('button', { name: 'Mesaj oluştur', exact: true }).click();
  await page.getByLabel('Alıcı', { exact: true }).selectOption('11'); await page.getByLabel('Mesaj', { exact: true }).fill('Fixture message'); await page.getByRole('button', { name: 'Gönder', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0); expect(sent[0]).toEqual({ mode: 'chat', clid: 11, message: 'Fixture message', expectedUid: 'abcdefghijklmnopqrstuvwx=' });
  await expect(page.getByText('TeamSpeak mesajı kabul etti. Bu, mesajın okunduğu anlamına gelmez.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Mesaj oluştur', exact: true }).click(); await page.getByLabel('Gönderim türü').selectOption('offline');
  await expect(page.getByLabel('Konu', { exact: true })).toBeVisible(); await expect(page.getByRole('button', { name: 'Gönder', exact: true })).toBeDisabled();
  await page.getByLabel('Alıcının TeamSpeak kimliği (UID)', { exact: true }).fill('abcdefghijklmnopqrstuvwx='); await page.getByLabel('Konu', { exact: true }).fill('Fixture'); await page.getByLabel('Mesaj', { exact: true }).fill('Offline fixture'); await page.getByRole('button', { name: 'Gönder', exact: true }).click();
  await expect.poll(() => sent.length).toBe(2); expect(sent[1].mode).toBe('offline');
});
test('reports localize labels, preserve legacy query and show named audit filters', async ({ page, request }) => {
  const auditCalls: string[] = [];
  await page.route('**/api/audit**', route => { auditCalls.push(route.request().url()); return route.fulfill({ json: { items: [], nextCursor: null, retention: { maxAgeDays: 30, maxRows: 100000 } } }); });
  await page.route('**/api/users', route => route.fulfill({ json: [{ id: 7, username: 'review-admin', displayName: 'Review Admin' }] }));
  await login(page, request); await page.goto('/audit?old=kept'); await expect(page).toHaveURL(/reports\?old=kept&view=audit/);
  await expect(page.getByRole('heading', { name: 'Raporlar ve tanılama', exact: true })).toBeVisible(); await page.getByLabel('Manager yöneticisi').selectOption('7');
  await expect.poll(() => auditCalls.some(url => url.includes('actorUserId=7'))).toBe(true);
  await expect(page.getByRole('option', { name: 'Kullanıcıyı yasakla', exact: true })).toBeAttached();
  await page.getByRole('button', { name: 'İngilizce', exact: true }).click(); await expect(page.getByRole('heading', { name: 'Reports and diagnostics', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click(); await page.reload(); await expect(page.getByLabel('Manager yöneticisi')).toBeVisible();
});
test('disabled journal hides stale reconnect errors and toggle makes capture explicit', async ({ page, request }) => {
  let enabled = false;
  const status = () => ({ serverConfigId: 1, virtualServerId: 1, enabled, status: enabled ? 'capturing' : 'disabled', droppedEvents: 0, lastError: 'Reconnecting capture (attempt 1)...' });
  await page.route('**/api/activity-journal/**', async route => {
    if (route.request().method() === 'PUT') { enabled = route.request().postDataJSON().enabled; return route.fulfill({ json: { ok: true, status: status() } }); }
    return route.fulfill({ json: route.request().url().includes('/status') ? { statuses: [status()], retention: { days: 7, perConnectionSid: 10000, global: 100000 } } : { items: [], nextCursor: null } });
  });
  await login(page, request); await page.goto('/reports?view=activity'); await expect(page.getByText('Kayıt kapalı. Giriş ve çıkışları kaydetmek için etkinleştirin.')).toBeVisible();
  await expect(page.getByText('Reconnecting capture', { exact: false })).toHaveCount(0); await page.getByRole('switch').click(); await expect(page.getByTestId('journal-capture-status')).toContainText('Kaydediliyor');
  await page.getByRole('switch').click(); await expect(page.getByTestId('journal-capture-status')).toContainText('Kapalı');
});
test('service information moved under servers and new-server group templates under permissions', async ({ page, request }) => {
  await page.route('**/api/servers/*/instance**', route => route.fulfill({ json: [{ version: '6-test', virtualservers_total: 2, serverinstance_filetransfer_port: 30033, serverinstance_template_serveradmin_group: 3 }] }));
  await login(page, request); await page.goto('/instance'); await expect(page).toHaveURL('/servers?tab=service'); await expect(page.getByRole('heading', { name: 'Sunucu hizmeti bilgileri', exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'Yeni sunucu grup şablonlarını Yetkiler bölümünde yönet' }).click(); await expect(page.getByRole('heading', { name: 'Yeni sunucu grup şablonları', exact: true })).toBeVisible();
  await expect(page.getByText('mevcut kullanıcıların yetkileri değildir', { exact: false })).toBeVisible();
});
test('provider refusal is Turkish, actionable, and never shown as playing', async ({ page, request }) => {
  await page.route('**/api/music-bots', route => route.fulfill({ json: [{ id: 4, name: 'Review bot', serverConfigId: 1, status: 'idle' }] }));
  await page.route('**/api/servers/*/vs/*/clients', route => route.fulfill({ json: [{ clid: 11, client_type: 0, client_nickname: 'Review Alice' }] }));
  await page.route('**/api/music-bots/4/play-url', route => route.fulfill({ status: 422, json: { error: 'The media provider requires verification', reason: 'media_auth_required', retryable: false, details: 'Refresh your YouTube cookies in Settings → YouTube and check outbound connectivity. Cookies do not guarantee access.' } }));
  await login(page, request); await page.goto('/clients'); await page.getByRole('button', { name: 'Review Alice işlemleri', exact: true }).click(); await page.getByRole('menuitem', { name: 'YouTube / Spotify / Apple Music çal', exact: true }).click();
  await page.getByRole('dialog').getByRole('textbox').fill('https://www.youtube.com/watch?v=abcdefghijk'); await page.getByRole('button', { name: 'Oynat', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('Medya sağlayıcısı doğrulama istiyor');
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('erişimi garanti etmez'); await expect(page.getByRole('button', { name: 'Oynat', exact: true })).toBeEnabled();
});
test('media diagnostics show localized readiness and original technical detail only on demand', async ({ page, request }) => {
  await page.route('**/api/settings/media-diagnostics', route => route.fulfill({ json: { checkedAt: new Date().toISOString(), overall: 'partial', checks: [{ id: 'ffmpeg', label: 'FFmpeg', status: 'ok', message: 'Original technical fixture', version: 'ffmpeg fixture' }], sidecar: { status: 'ok', mode: 'url', message: 'Technical sidecar message' }, encoders: { status: 'warning', message: 'No GPU', items: [] }, warnings: [] } }));
  await login(page, request); await page.goto('/diagnostics'); await expect(page).toHaveURL('/reports?view=media');
  await expect(page.getByText('Araç kontrolü başarılı', { exact: true })).toBeVisible(); await expect(page.getByText('Original technical fixture', { exact: true })).not.toBeVisible();
  await page.getByText('Teknik çıktı (özgün dilinde)', { exact: true }).first().click(); await expect(page.getByText('Original technical fixture', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'İngilizce', exact: true }).click(); await expect(page.getByText('Tool check passed', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click(); await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole('button', { name: 'Yenile', exact: true })).toBeVisible(); expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
test('key-use history is separate from active keys, opt-in and scoped', async ({ page, request }) => {
  let enabled = false; const seen: any[] = [];
  await page.route('**/api/servers/*/vs/*/tokens/history', route => {
    if (route.request().method() === 'PUT') { enabled = route.request().postDataJSON().enabled; seen.push(route.request().url()); }
    return route.fulfill({ json: { enabled, status: enabled ? 'capturing' : 'disabled', items: enabled ? [{ id: 'event1', observedAt: '2026-10-21T10:00:00Z', fingerprint: '0123456789abcdef', clientId: 11, nickname: 'Receipt Alice', uniqueId: 'receipt-uid', databaseId: 22 }] : [] } });
  });
  await page.route('**/api/servers/*/vs/*/tokens', route => route.fulfill({ json: [] }));
  await login(page, request); await page.goto('/tokens');
  await page.getByRole('button', { name: 'Kullanım kaydını başlat', exact: true }).click();
  await expect(page.getByText('Receipt Alice', { exact: true })).toBeVisible(); await expect(page.getByText('receipt-uid', { exact: true })).toBeVisible();
  expect(seen[0]).toContain('/servers/1/vs/1/tokens/history');
  await page.getByRole('button', { name: 'Kullanım kaydını durdur', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Kullanım kaydını başlat', exact: true })).toBeVisible();
});
