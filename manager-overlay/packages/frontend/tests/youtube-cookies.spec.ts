import { test, expect } from '@playwright/test';

for (const language of ['tr', 'en']) {
  test(`YouTube cookie-only lifecycle (${language})`, async ({ page, request }) => {
    await request.post('/__test/reset'); await request.post('/__test/auth?on');
    await page.addInitScript((language) => localStorage.setItem('teamhub-language', JSON.stringify({ state: { language }, version: 0 })), language);
    const tr = language === 'tr';
    const status = { active: false, exists: false, removable: false, size: 0, mode: 'cookies' };
    let fail = false; let puts = 0;
    await page.route('**/api/settings/yt-cookies**', async (route) => {
      const req = route.request();
      if (req.method() === 'PUT') { puts++; return route.fulfill({ status: 410, json: { error: 'Removed' } }); }
      if (req.method() === 'POST') {
        if (fail) return route.fulfill({ status: 500, json: { error: 'Test upload failed' } });
        status.exists = true; status.removable = true; status.active = true; status.size = 100;
      } else if (req.method() === 'DELETE') { status.active = false; status.exists = false; status.removable = false; }
      return route.fulfill({ json: status });
    });
    await page.goto('/login');
    await page.getByLabel(tr ? 'Kullanıcı adı' : 'Username', { exact: true }).fill('admin');
    await page.getByLabel(tr ? 'Parola' : 'Password', { exact: true }).fill('test-password');
    await page.getByRole('button', { name: tr ? 'Giriş yap' : 'Sign In', exact: true }).click();
    await expect(page).toHaveURL('/dashboard');
    await page.goto('/settings?tab=youtube');
    await expect(page.getByText(tr ? 'Çerez dosyası eklenmedi' : 'No cookies configured', { exact: true })).toBeVisible();
    await expect(page.getByLabel(tr ? 'Oynatma yöntemi' : 'Playback method', { exact: true })).toHaveCount(0);
    await expect(page.getByText(/PO Token/)).toHaveCount(0);
    await page.getByRole('button', { name: tr ? 'Çerezleri yapıştır' : 'Paste cookies', exact: true }).click();
    const save = page.getByRole('button', { name: tr ? 'Kaydet' : 'Save', exact: true });
    await expect(save).toBeDisabled();
    await page.locator('textarea').fill('# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\ttest\tfixture');
    await save.click();
    await expect(page.getByText(/Cookie file configured|Çerez dosyası mevcut/)).toBeVisible();
    fail = true;
    await page.locator('#cookie-file-input').setInputFiles({ name: 'cookies.txt', mimeType: 'text/plain', buffer: Buffer.from('# TEST FIXTURE ONLY') });
    await expect(page.getByText(tr ? 'Çerez dosyası yüklenemedi' : 'Failed to upload cookie file', { exact: true })).toBeVisible();
    await expect(page.getByText(/Cookie file configured|Çerez dosyası mevcut/)).toBeVisible();
    fail = false;
    await page.getByRole('button', { name: tr ? 'Kaldır' : 'Remove', exact: true }).click();
    await expect(page.getByText(tr ? 'Çerez dosyası eklenmedi' : 'No cookies configured', { exact: true })).toBeVisible();
    expect(puts).toBe(0);
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.getByRole('button', { name: tr ? 'Çerezleri yapıştır' : 'Paste cookies', exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  });
}
