import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

async function login(page: Page, request: APIRequestContext) {
  await request.post('/__test/reset');
  await request.post('/__test/auth?on');
  await page.goto('/login');
  await page.getByLabel('Username', { exact: true }).fill('admin');
  await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In', exact: true }).click();
  await expect(page).toHaveURL('/dashboard');
}

test('extensions are ready cards and a simple command form, with no JSON box', async ({ page, request }) => {
  const flows: Array<{ id: number; name: string; enabled: boolean; serverConfigId: number }> = [];
  const commands: Array<{ id: number; name: string; response: string; enabled: boolean }> = [];
  await page.route('**/api/bots', async (route) => {
    if (route.request().method() === 'POST') {
      const body = route.request().postDataJSON();
      const flow = { id: flows.length + 1, name: body.name, enabled: false, serverConfigId: body.serverConfigId };
      flows.push(flow);
      return route.fulfill({ status: 201, json: flow });
    }
    return route.fulfill({ json: flows });
  });
  await page.route('**/api/bots/*/enable', async (route) => {
    const id = Number(route.request().url().split('/').at(-2));
    flows.find((flow) => flow.id === id)!.enabled = true;
    await route.fulfill({ json: { success: true } });
  });
  await page.route('**/api/bots/*/disable', async (route) => {
    const id = Number(route.request().url().split('/').at(-2));
    flows.find((flow) => flow.id === id)!.enabled = false;
    await route.fulfill({ json: { success: true } });
  });
  await page.route('**/api/servers/*/chat-commands', async (route) => {
    if (route.request().method() === 'POST') {
      const body = route.request().postDataJSON();
      const command = { id: commands.length + 1, name: body.name, response: body.response, enabled: true };
      commands.push(command);
      return route.fulfill({ status: 201, json: command });
    }
    return route.fulfill({ json: commands });
  });
  await page.route('**/api/servers/*/extensions', (route) => route.fulfill({ json: { bundled: [], installed: [] } }));

  await page.route('**/api/servers', (route) => route.request().method() === 'GET'
    ? route.fulfill({ json: [{ id: 1, name: 'Primary server', host: 'primary.invalid' }] }) : route.fallback());
  await login(page, request);
  await page.evaluate(() => localStorage.setItem('ts6-server', JSON.stringify({ state: { selectedConfigId: 1, selectedSid: 1 }, version: 0 })));
  await page.goto('/bots?tab=extensions');
  const panel = page.locator('#bot-extensions-panel');
  await expect(panel.getByText('Ready extensions')).toBeVisible();
  await expect(panel.locator('textarea')).toHaveCount(0);

  const welcome = panel.getByRole('group', { name: 'Welcome Message', exact: true });
  await welcome.getByRole('button', { name: 'Turn on' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel(/Welcome Message/).fill('Hoş geldin!');
  await dialog.getByRole('button', { name: 'Turn on' }).click();
  await expect(welcome.getByRole('switch', { name: 'Welcome Message' })).toBeChecked();
  expect(flows).toEqual([expect.objectContaining({ name: 'Welcome Message', enabled: true })]);
  await welcome.getByRole('switch', { name: 'Welcome Message' }).click();
  await expect(welcome.getByRole('switch', { name: 'Welcome Message' })).not.toBeChecked();

  await panel.getByLabel('Command').fill('!site');
  await panel.getByLabel('Bot reply').fill('https://example.com');
  await panel.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(panel.getByText('!site')).toBeVisible();
  expect(commands[0]).toMatchObject({ name: 'site', response: 'https://example.com' });
});
