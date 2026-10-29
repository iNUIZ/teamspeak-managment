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

const base = {
  serverConfigId: 1, serverConfig: { id: 1, name: 'Operations Voice', host: 'ops.invalid' }, serverPassword: null,
  defaultChannel: 'Lobby', commandChannelIds: [], virtualServerId: 1, channelPassword: null, voicePort: 9987,
  volume: 40, autoStart: true, nowPlaying: null, createdAt: '2026-10-29T00:00:00.000Z',
};

async function mockBots(page: Page, calls: { method: string; url: string; body: unknown }[]) {
  const automation = { enabled: true, source: { kind: 'radio', id: 7 }, requireListeners: false, quietPeriodSeconds: 300, retryBaseSeconds: 30, retryMaxSeconds: 1800, stayInChannel: true };
  await page.route('**/api/music-bots', (r) => r.fulfill({ json: [
    { ...base, id: 4, name: 'TeamHub Muzik', nickname: 'TeamHub Muzik', externalHost: '25krs', status: 'playing' },
    { ...base, id: 3, name: 'test', nickname: 'test', status: 'connected' },
  ] }));
  await page.route(/\/api\/music-bots\/4(\/.*)?$/, async (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname.replace('/api/music-bots/4', '');
    if (req.method() !== 'GET') calls.push({ method: req.method(), url: path, body: req.postDataJSON() });
    if (path === '/state') return route.fulfill({ json: {
      status: 'playing', nowPlaying: { id: 'a', title: 'Napıyosun Mesela', artist: 'BLOK3' }, position: 72, duration: 188, volume: 40,
      queue: [{ id: 'a', title: 'Napıyosun Mesela', artist: 'BLOK3' }, { id: 'b', title: 'Gidiyorum', artist: 'Sezen Aksu' }],
      currentIndex: 0, shuffle: false, repeat: 'off', isStreaming: false,
    } });
    if (path === '/activity') return route.fulfill({ json: [
      { at: '2026-10-29T20:37:38.000Z', via: 'teamspeak', who: 'BeBeK', action: '!ara sonucu #1' },
      { at: '2026-10-29T20:34:21.000Z', via: 'teamspeak', who: 'BeBeK', action: '!vol 1' },
      { at: '2026-10-29T20:35:09.000Z', via: 'panel', who: 'admin', action: '!pause' },
    ] });
    if (path === '/search') return route.fulfill({ json: { cookies: true, results: [
      { title: 'Kuzu Kuzu', artist: 'Tarkan', duration: 201, url: 'https://open.spotify.com/track/4PTG3Z6ehGkBFwjybzWkR8', source: 'Spotify' },
    ] } });
    if (path === '' && req.method() === 'GET') return route.fulfill({ json: { ...base, id: 4, name: 'TeamHub Muzik', status: 'playing', automationSettings: automation } });
    if (path === '/automation') return route.fulfill({ json: req.postDataJSON() });
    return route.fulfill({ json: { success: true } });
  });
  await page.route('**/api/servers/1/radio-stations', (r) => r.fulfill({ json: [{ id: 7, name: 'Capital UK', serverConfigId: 1 }] }));
}

test('Bot control shows who asked for the song and drives the bot in plain controls', async ({ page, request }) => {
  const calls: { method: string; url: string; body: unknown }[] = [];
  await mockBots(page, calls);
  await signIn(page, request);
  await page.goto('/');
  await expect(page).toHaveURL('/control');
  await expect(page.getByRole('link', { name: 'Advanced bot settings' })).toHaveAttribute('href', '/bot-hub/4');

  await expect(page.getByRole('listitem').filter({ hasText: 'TeamHub Muzik' })).toContainText('25krs');
  const now = page.getByRole('region', { name: 'Now playing' });
  await expect(now).toContainText('BLOK3 - Napıyosun Mesela');
  await expect(now).toContainText('Asked by: BeBeK');
  await expect(page.getByRole('region', { name: 'Up next' })).toContainText('Sezen Aksu - Gidiyorum');
  const log = page.getByRole('region', { name: 'Who did what' });
  await expect(log).toContainText('BeBeK');
  await expect(log).toContainText('!vol 1');

  await now.getByRole('button', { name: 'Pause' }).click();
  await expect.poll(() => calls.map((c) => c.url)).toContain('/pause');

  await expect(now.getByRole('radio', { name: 'Play radio' })).toHaveAttribute('aria-checked', 'true');
  await now.getByRole('radio', { name: 'Stay quiet' }).click();
  await expect.poll(() => calls.find((c) => c.url === '/automation')?.body).toMatchObject({ enabled: false });

  await page.getByLabel('Song name or link').fill('tarkan kuzu kuzu');
  await page.getByRole('button', { name: 'Search', exact: true }).click();
  const results = page.getByRole('list', { name: 'Search results' });
  await expect(results).toContainText('Tarkan - Kuzu Kuzu');
  await results.getByRole('button', { name: 'Add to queue' }).click();
  await expect.poll(() => calls.find((c) => c.url === '/play-url')?.body).toEqual({ url: 'https://open.spotify.com/track/4PTG3Z6ehGkBFwjybzWkR8', enqueue: true });
});

test('a pasted link plays directly without searching', async ({ page, request }) => {
  const calls: { method: string; url: string; body: unknown }[] = [];
  await mockBots(page, calls);
  await signIn(page, request);
  await page.goto('/control');
  await page.getByLabel('Song name or link').fill('https://www.youtube.com/watch?v=dQw4w9WgXcQ');
  await page.getByRole('region', { name: 'Search' }).getByRole('button', { name: 'Play', exact: true }).click();
  await expect.poll(() => calls.find((c) => c.url === '/play-url')?.body).toEqual({ url: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ', enqueue: false });
  expect(calls.some((c) => c.url === '/search')).toBe(false);
});
