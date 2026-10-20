import { test, expect } from '@playwright/test';

for (const language of ['tr', 'en']) {
  test(`YouTube PO Token opt-in lifecycle (${language})`, async ({ page, request }) => {
    await request.post('/__test/reset'); await request.post('/__test/auth?on');
    await page.addInitScript((language) => localStorage.setItem('teamhub-language', JSON.stringify({ state: { language }, version: 0 })), language);
    const tr = language === 'tr';
    const status = { active: false, exists: false, size: 0, mode: 'po-token', provider: { configured: true, ready: true, version: '2.0.2' } };
    let fail = false;
    await page.route('**/api/settings/yt-cookies**', async (route) => {
      const req = route.request();
      if (req.method() === 'PUT') {
        if (fail) return route.fulfill({ status: 500, json: { error: 'Test update failed' } });
        status.mode = req.postDataJSON().mode; status.active = status.mode === 'cookies';
      } else if (req.method() === 'POST') { status.exists = true; status.size = 100; }
      else if (req.method() === 'DELETE') { status.mode = 'po-token'; status.active = false; status.exists = false; }
      return route.fulfill({ json: status });
    });
    await page.goto('/login');
    await page.getByLabel(tr ? 'Kullanıcı adı' : 'Username', { exact: true }).fill('admin');
    await page.getByLabel(tr ? 'Parola' : 'Password', { exact: true }).fill('test-password');
    await page.getByRole('button', { name: tr ? 'Giriş yap' : 'Sign In', exact: true }).click();
    await expect(page).toHaveURL('/dashboard');
    await page.goto('/settings?tab=youtube');
    const method = page.getByLabel(tr ? 'Oynatma yöntemi' : 'Playback method', { exact: true });
    await expect(method).toHaveValue('po-token');
    await expect(method.locator('option[value="cookies"]')).toHaveAttribute('disabled', '');
    await expect(page.getByText(tr ? 'PO Token hizmeti hazır' : 'PO Token provider ready', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: tr ? 'Çerezleri yapıştır' : 'Paste cookies', exact: true }).click();
    await page.locator('textarea').fill('# Netscape HTTP Cookie File\n# TEST FIXTURE ONLY');
    await page.getByRole('button', { name: tr ? 'Kaydet' : 'Save', exact: true }).click();
    await expect(page.getByText(tr ? 'Çerez dosyası kayıtlı — kullanımda değil' : 'Cookie file stored — not in use', { exact: true })).toBeVisible();
    await expect(method).toHaveValue('po-token');
    await method.selectOption('cookies'); await expect(method).toHaveValue('cookies');
    await method.selectOption('po-token'); await expect(method).toHaveValue('po-token');
    fail = true; await method.selectOption('cookies');
    await expect(page.getByText(tr ? 'YouTube oynatma yöntemi değiştirilemedi' : 'Failed to change YouTube playback mode', { exact: true })).toBeVisible();
    await expect(method).toHaveValue('po-token');
    fail = false; status.provider.ready = false;
    await page.getByRole('button', { name: tr ? 'Yenile' : 'Refresh', exact: true }).click();
    await expect(page.getByText(tr ? 'PO Token hizmetine ulaşılamıyor — yerel hizmeti kontrol edin' : 'PO Token provider unavailable — check the local service', { exact: true })).toBeVisible();
    status.provider.configured = false;
    await page.getByRole('button', { name: tr ? 'Yenile' : 'Refresh', exact: true }).click();
    await expect(page.getByText(tr ? 'Bu çalışma ortamında PO Token hizmeti kurulu değil' : 'PO Token provider is not installed in this runtime', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: tr ? 'Kaldır' : 'Remove', exact: true }).click();
    await expect(method.locator('option[value="cookies"]')).toHaveAttribute('disabled', '');
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(method).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  });
}
