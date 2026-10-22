import { expect, test, type Page, type APIRequestContext } from '@playwright/test';

async function login(page: Page, request: APIRequestContext) {
  await request.post('/__test/reset');
  await request.post('/__test/dashboard?scenario=normal');
  await request.post('/__test/auth?on');
  await page.goto('/login');
  await page.getByLabel('Username', { exact: true }).fill('admin');
  await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In', exact: true }).click();
  await expect(page).toHaveURL('/dashboard');
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click();
}

test('TR widget form and permissions tabs switch to EN and back with persistent locale', async ({ page, request }) => {
  await login(page, request);
  await page.getByRole('button', { name: 'Bileşenler', exact: true }).click();
  await page.getByRole('button', { name: 'Yeni bileşen', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Bileşen oluştur', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('combobox', { name: 'TeamSpeak sunucusu', exact: true })).toBeVisible();
  await expect(dialog.getByText('Kanal ağacını göster', { exact: true })).toBeVisible();
  await dialog.getByPlaceholder('Sunucu bileşenim').fill('Test value');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'İngilizce', exact: true }).click();
  await page.goto('/permissions');
  await expect(page.getByRole('heading', { name: 'Permissions', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Yetkiler', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sunucu grupları', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Kanal grupları', exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Yetkiler', exact: true })).toBeVisible();
});

test('privilege-key copy works without Clipboard API and copies the complete key', async ({ page, request }) => {
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.addInitScript(() => { Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true }); });
  await page.route('**/api/servers/*/vs/*/tokens', route => route.fulfill({ json: [{ token: 'test-long-privilege-key-1234567890', token_type: 0, token_id1: 6, token_description: 'Test key' }] }));
  await login(page, request);
  await page.goto('/tokens');
  await page.getByRole('button', { name: 'Yetki anahtarını kopyala', exact: true }).click();
  await expect(page.getByText('Kopyalandı', { exact: true })).toBeVisible();
  const search = page.getByRole('searchbox', { name: 'Yetki anahtarlarında ara' });
  await search.focus();
  await page.keyboard.press('Control+V');
  await expect(search).toHaveValue('test-long-privilege-key-1234567890');
});

test('clipboard denial never produces a false copied message', async ({ page, request }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: () => Promise.reject(Error('denied')) }, configurable: true });
    document.execCommand = () => false;
  });
  await page.route('**/api/servers/*/vs/*/tokens', route => route.fulfill({ json: [{ token: 'test-key', token_type: 0, token_id1: 6 }] }));
  await login(page, request);
  await page.goto('/tokens');
  await page.getByRole('button', { name: 'Yetki anahtarını kopyala', exact: true }).click();
  await expect(page.getByText('Kopyalanamadı. Metni seçip elle kopyalayın.', { exact: true })).toBeVisible();
  await expect(page.getByText('Kopyalandı', { exact: true })).toHaveCount(0);
});
