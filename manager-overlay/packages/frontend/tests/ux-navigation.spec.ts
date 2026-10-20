import { expect, test } from '@playwright/test';

test('TR menu searches both languages, clears with Escape and preserves branding', async ({ page, request }) => {
  await request.post('/__test/reset');
  await request.post('/__test/dashboard?scenario=normal');
  await request.post('/__test/auth?on');
  await page.goto('/login');
  await page.getByLabel('Username', { exact: true }).fill('admin');
  await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In', exact: true }).click();
  await expect(page).toHaveURL('/dashboard');
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click();
  const search = page.getByRole('textbox', { name: 'Menüde ara' });
  await search.fill('teamspeak');
  await expect(page.getByRole('link', { name: 'TeamSpeak sunucuları', exact: true })).toBeVisible();
  await search.fill('permissions');
  await expect(page.getByRole('link', { name: 'Yetkiler', exact: true })).toBeVisible();
  await search.fill('does-not-exist');
  await expect(page.getByText('Eşleşen sayfa bulunamadı', { exact: true })).toBeVisible();
  await search.press('Escape');
  await expect(search).toHaveValue('');
  await search.fill('kanal');
  await page.getByRole('button', { name: 'Aramayı temizle', exact: true }).click();
  await page.getByRole('link', { name: 'TeamSpeak sunucuları', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'TeamSpeak sunucuları', exact: true })).toBeVisible();
  await expect(page.getByText('Sanal sunucular', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'English', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'TeamSpeak servers', exact: true })).toBeVisible();
});
