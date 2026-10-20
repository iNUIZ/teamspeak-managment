import { expect, test, type Page, type APIRequestContext } from '@playwright/test';

async function login(page: Page, request: APIRequestContext) {
  await request.post('/__test/reset');
  await request.post('/__test/docs?on');
  await request.post('/__test/auth?on');
  await page.goto('/login');
  await page.getByLabel('Username', { exact: true }).fill('admin');
  await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In', exact: true }).click();
  await expect(page).toHaveURL('/dashboard');
  await page.getByRole('button', { name: 'Türkçe', exact: true }).click();
}
const initialTts = { enabled: true, language: 'tr', maxCharacters: 250, cooldownSeconds: 10, volume: 1 };
const commands = [{ name: 'play', canonical: 'play', usage: '!play <url>' }, { name: 'ara', canonical: 'ara', usage: '!ara <song>' }, { name: 'vol', canonical: 'vol', usage: '!vol [0-100]' }];

test('bot extensions isolate switches and TTS; settings no longer has global TTS', async ({ page, request }) => {
  const disabled: Record<string, string[]> = { '7': [], '8': [] };
  const tts: Record<string, typeof initialTts> = { '7': { ...initialTts }, '8': { ...initialTts } };
  await page.route('**/api/music-bots/*/extensions', async route => {
    const id = route.request().url().split('/').at(-2)!;
    if (route.request().method() === 'PUT') disabled[id] = route.request().postDataJSON().disabledCommands;
    await route.fulfill({ json: { serverConfigId: 1, disabledCommands: disabled[id], builtin: commands, custom: [] } });
  });
  await page.route('**/api/music-bots/*/tts', async route => {
    const id = route.request().url().split('/').at(-2)!;
    if (route.request().method() === 'PUT') tts[id] = route.request().postDataJSON();
    await route.fulfill({ json: { settings: tts[id], installed: true } });
  });
  await login(page, request);
  await page.goto('/settings');
  await expect(page.getByRole('tab', { name: 'TTS', exact: true })).toHaveCount(0);
  await page.goto('/bot-hub');
  await page.getByRole('button', { name: 'Aurora Radio ayarları', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Eklentiler ve komutlar', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('heading', { name: 'Aurora Radio · Eklentiler ve komutlar' })).toBeVisible();
  const search = dialog.getByRole('textbox', { name: 'Komutlarda ara' });
  await search.fill('ara');
  await expect(dialog.getByRole('switch')).toHaveCount(1);
  await dialog.getByRole('switch', { name: '!ara', exact: true }).click();
  await expect(dialog.getByRole('switch', { name: '!ara', exact: true })).not.toBeChecked();
  expect(disabled['7']).toEqual(['ara']);
  expect(disabled['8']).toEqual([]);
  await dialog.getByRole('switch', { name: '!ara', exact: true }).click();
  await expect(dialog.getByRole('switch', { name: '!ara', exact: true })).toBeChecked();
  await dialog.getByRole('tab', { name: 'Sesli okuma (TTS)' }).click();
  await dialog.getByLabel('Varsayılan ses', { exact: true }).selectOption('en');
  await dialog.getByLabel('Karakter sınırı (20–500)', { exact: true }).fill('1');
  await expect(dialog.getByRole('button', { name: 'Kaydet', exact: true })).toBeDisabled();
  await dialog.getByLabel('Karakter sınırı (20–500)', { exact: true }).fill('200');
  await dialog.getByRole('button', { name: 'Kaydet', exact: true }).click();
  await expect(page.getByText('TTS ayarları kaydedildi', { exact: true })).toBeVisible();
  expect(tts['7'].language).toBe('en');
  expect(tts['8'].language).toBe('tr');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Lounge Bot ayarları', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Eklentiler ve komutlar', exact: true }).click();
  await dialog.getByRole('tab', { name: 'Sesli okuma (TTS)' }).click();
  await expect(dialog.getByLabel('Varsayılan ses', { exact: true })).toHaveValue('tr');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'English', exact: true }).click();
  await page.getByRole('button', { name: 'Settings for Lounge Bot', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Extensions and commands', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Lounge Bot · Extensions and commands' })).toBeVisible();
  await dialog.getByRole('tab', { name: 'Text to speech (TTS)' }).click();
  await expect(dialog.getByLabel('Default voice', { exact: true })).toHaveValue('tr');
});

test('player widget title, fields and failure retry use Turkish', async ({ page, request }) => {
  let requests = 0;
  await page.route('**/api/music-bots/7/player-widget-token', route => {
    requests++;
    return requests === 1 ? route.fulfill({ status: 503, json: {} }) : route.fulfill({ json: { token: 'fixture-token', bbcodeUrl: 'https://example.test/widget', jsonUrl: 'https://example.test/json' } });
  });
  await login(page, request);
  await page.goto('/bot-hub');
  await page.getByRole('button', { name: 'Aurora Radio ayarları', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Bileşen bağlantısı', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('heading', { name: 'Oynatıcı bileşeni: Aurora Radio' })).toBeVisible();
  await expect(dialog.getByRole('alert')).toContainText('Bileşen bağlantısı yüklenemedi.');
  await dialog.getByRole('button', { name: 'Tekrar dene', exact: true }).click();
  await expect(dialog.getByLabel('Bileşen erişim anahtarı', { exact: true })).toHaveValue('fixture-token');
  await expect(dialog.getByLabel('BBCode bağlantısı (kanal açıklaması için)', { exact: true })).toBeVisible();
});

test('failed extension fetch retries; failed switch save never pretends success', async ({ page, request }) => {
  let reads = 0;
  await page.route('**/api/music-bots/7/extensions', route => {
    if (route.request().method() === 'PUT') return route.fulfill({ status: 500, json: {} });
    reads++;
    return reads === 1 ? route.fulfill({ status: 404, json: {} }) : route.fulfill({ json: { serverConfigId: 1, disabledCommands: [], builtin: commands, custom: [] } });
  });
  await login(page, request);
  await page.goto('/bot-hub');
  await page.getByRole('button', { name: 'Aurora Radio ayarları', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Eklentiler ve komutlar', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('alert')).toContainText('Eklentiler yüklenemedi.');
  await dialog.getByRole('button', { name: 'Tekrar dene', exact: true }).click();
  await dialog.getByRole('switch', { name: '!ara', exact: true }).click();
  await expect(page.getByText('Komut ayarı kaydedilemedi', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('switch', { name: '!ara', exact: true })).toBeChecked();
});
