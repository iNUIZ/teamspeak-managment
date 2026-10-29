import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

async function signIn(page: Page, request: APIRequestContext) {
  await request.post('/__test/reset');
  await request.post('/__test/auth?on');
  await request.post('/__test/dashboard?scenario=normal');
  await page.goto('/login');
  await page.getByLabel('Username').fill('admin');
  await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In' }).click();
  await expect(page).toHaveURL('/dashboard');
}

test('a new bot can join another TeamSpeak server by address without Query settings', async ({ page, request }) => {
  const created: Record<string, unknown>[] = [];
  await page.route('**/api/music-bots', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    created.push(route.request().postDataJSON());
    return route.fulfill({ status: 201, json: { id: 9 } });
  });
  await signIn(page, request);
  await page.goto('/bot-hub');
  await page.getByRole('button', { name: 'New bot' }).click();
  const dialog = page.getByRole('dialog', { name: 'New bot' });
  await dialog.getByLabel('Bot name and TeamSpeak nickname').fill('Guest Radio');
  await dialog.getByRole('combobox', { name: 'Server' }).click();
  await page.getByRole('option').first().click();
  await expect(dialog.getByText('Voice Port', { exact: true })).toBeVisible();

  await dialog.getByRole('switch', { name: 'Join another TeamSpeak server' }).check();
  await expect(dialog.getByText('Voice Port', { exact: true })).toBeHidden();
  await expect(dialog.getByText('Virtual server ID', { exact: true })).toBeHidden();
  await expect(dialog.getByRole('button', { name: 'Create' })).toBeDisabled();
  await dialog.getByLabel('TeamSpeak address').fill(' 25krs ');
  await dialog.getByRole('button', { name: 'Create' }).click();

  await expect.poll(() => created.length).toBe(1);
  expect(created[0]).toMatchObject({ name: 'Guest Radio', externalHost: '25krs' });
  await expect(dialog).toBeHidden();
});

test('turning the external server off sends an empty address so the bot returns home', async ({ page, request }) => {
  const updates: Record<string, unknown>[] = [];
  const bot = {
    id: 4, name: 'Guest Radio', nickname: 'Guest Radio', serverConfigId: 1,
    serverConfig: { id: 1, name: 'Operations Voice', host: 'ops.invalid' }, externalHost: '25krs', serverPassword: null,
    defaultChannel: 'Lobby', commandChannelIds: [], virtualServerId: 1, channelPassword: null, voicePort: 0,
    volume: 50, autoStart: false, status: 'stopped', nowPlaying: null, createdAt: '2026-10-29T00:00:00.000Z',
  };
  await page.route('**/api/music-bots', (route) => route.fulfill({ json: [bot] }));
  await page.route('**/api/music-bots/media', (route) => route.fulfill({ json: [{
    botId: 4, botName: 'Guest Radio', serverConfigId: 1, serverName: 'Operations Voice', status: 'stopped',
    channelId: null, channelName: null, session: null, music: null, video: null, lastMusicStop: null, lastVideoStop: null,
  }] }));
  await page.route('**/api/music-bots/4', async (route) => {
    if (route.request().method() === 'PUT') {
      updates.push(route.request().postDataJSON());
      return route.fulfill({ json: { success: true } });
    }
    return route.fulfill({ json: bot });
  });
  await signIn(page, request);
  await page.goto('/bot-hub');
  await page.getByRole('button', { name: 'Settings for Guest Radio' }).click();
  await page.getByRole('menuitem', { name: 'Edit bot' }).click();
  const dialog = page.getByRole('dialog', { name: 'Edit bot' });
  await expect(dialog.getByLabel('TeamSpeak address')).toHaveValue('25krs');
  await dialog.getByRole('switch', { name: 'Join another TeamSpeak server' }).uncheck();
  await dialog.getByRole('button', { name: 'Save' }).click();
  await expect.poll(() => updates.length).toBe(1);
  expect(updates[0]).toMatchObject({ externalHost: '' });
});
