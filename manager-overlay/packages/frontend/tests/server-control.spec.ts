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

test('Server control lists people by channel and moves, pokes and kicks them', async ({ page, request }) => {
  const calls: Array<{ path: string; body: unknown }> = [];
  await page.route(/\/api\/servers\/\d+\/vs\/\d+\/channels$/, (r) => r.fulfill({ json: [
    { cid: '1', pid: '0', channel_name: 'Lobby' },
    { cid: '2', pid: '0', channel_name: 'Müzik Odası' },
    { cid: '3', pid: '2', channel_name: 'Sessiz köşe' },
  ] }));
  await page.route(/\/api\/servers\/\d+\/vs\/\d+\/clients$/, (r) => r.fulfill({ json: [
    { clid: '149', cid: '2', client_nickname: 'BeBeK', client_type: '0', client_away: '0' },
    { clid: '154', cid: '2', client_nickname: 'kabus', client_type: '0', client_away: '1' },
    { clid: '7', cid: '1', client_nickname: 'serveradmin', client_type: '1' },
  ] }));
  await page.route(/\/api\/servers\/\d+\/vs\/\d+\/clients\/\d+\/(move|poke|kick)$/, async (r) => {
    calls.push({ path: new URL(r.request().url()).pathname.split('/clients/')[1], body: r.request().postDataJSON() });
    return r.fulfill({ json: { success: true } });
  });
  await signIn(page, request);
  await page.goto('/server-control');

  const tree = page.getByRole('region', { name: 'Channels and people' });
  await expect(tree).toContainText('Müzik Odası');
  await expect(tree).toContainText('Sessiz köşe');
  await expect(tree.getByRole('button', { name: /BeBeK/ })).toBeVisible();
  await expect(tree).not.toContainText('serveradmin');
  await expect(page.getByText('2 online')).toBeVisible();

  await tree.getByRole('button', { name: /BeBeK/ }).click();
  const dialog = page.getByRole('dialog', { name: 'BeBeK' });
  await dialog.getByRole('combobox', { name: 'Channel' }).click();
  await page.getByRole('option', { name: 'Lobby' }).click();
  await dialog.getByRole('button', { name: 'Move', exact: true }).last().click();
  await expect.poll(() => calls[0]).toEqual({ path: '149/move', body: { cid: 1 } });

  await tree.getByRole('button', { name: /kabus/ }).click();
  const second = page.getByRole('dialog', { name: 'kabus' });
  await second.getByRole('radio', { name: 'Poke' }).click();
  await expect(second.getByRole('button', { name: 'Poke' }).last()).toBeDisabled();
  await second.getByLabel('Text').fill('müziği kıs');
  await second.getByRole('button', { name: 'Poke' }).last().click();
  await expect.poll(() => calls[1]).toEqual({ path: '154/poke', body: { msg: 'müziği kıs' } });

  await tree.getByRole('button', { name: /kabus/ }).click();
  await page.getByRole('dialog', { name: 'kabus' }).getByRole('radio', { name: 'Kick' }).click();
  await page.getByRole('dialog', { name: 'kabus' }).getByRole('button', { name: 'Kick' }).last().click();
  await expect.poll(() => calls[2]).toEqual({ path: '154/kick', body: { reasonid: 5 } });
});
