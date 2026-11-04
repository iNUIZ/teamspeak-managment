import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

const mainItems = ['Music bots', 'Server', 'Channels', 'Library', 'Status', 'Settings'] as const;

async function signInAsAdmin(page: Page, request: APIRequestContext) {
  await request.post('/__test/auth?on');
  await page.goto('/login');
  await page.getByLabel('Username').fill('admin');
  await page.getByLabel('Password', { exact: true }).fill('test-password');
  await page.getByRole('button', { name: 'Sign In' }).click();
  await expect(page).toHaveURL('/dashboard');
}

test.beforeEach(async ({ request }) => {
  await request.post('/__test/reset');
});

test('the menu is one short list; configuration pages live under Settings', async ({ page, request }) => {
  await signInAsAdmin(page, request);
  const nav = page.getByRole('navigation', { name: 'Primary navigation' });
  for (const label of mainItems.slice(0, 5)) await expect(nav.getByRole('link', { name: label, exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
  for (const hidden of ['Permissions', 'IPTV', 'Bot Flows', 'Server Groups']) {
    await expect(nav.getByRole('link', { name: hidden, exact: true })).toHaveCount(0);
  }
  await expect(page.getByRole('button', { name: /section$/ })).toHaveCount(0);
});

test('Settings opens an overview of every management page, each with a way back', async ({ page, request }) => {
  await signInAsAdmin(page, request);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const people = page.getByRole('region', { name: 'People and access' });
  await expect(people.getByRole('link', { name: /Permissions/ })).toBeVisible();
  await people.getByRole('link', { name: /Permissions/ }).click();
  await expect(page).toHaveURL('/permissions');
  await expect(page.getByRole('complementary').getByRole('link', { name: 'Settings', exact: true })).toHaveAttribute('aria-current', 'page');
  const crumb = page.getByRole('navigation', { name: 'Breadcrumb' });
  await expect(crumb).toContainText('Permissions');
  await crumb.getByRole('link', { name: 'Settings' }).click();
  await expect(page).toHaveURL('/settings');
});

test('menu search also finds the pages under Settings', async ({ page, request }) => {
  await signInAsAdmin(page, request);
  const search = page.getByRole('textbox', { name: 'Search menu' });
  await search.fill('perm');
  const nav = page.getByRole('navigation', { name: 'Primary navigation' });
  await expect(nav.getByRole('link', { name: 'Permissions', exact: true })).toBeVisible();
  await search.fill('iptv');
  await expect(nav.getByRole('link', { name: 'Library', exact: true })).toBeVisible();
});

test('old IPTV and YouTube links open their Library tabs', async ({ page, request }) => {
  await signInAsAdmin(page, request);
  await page.goto('/iptv');
  await expect(page).toHaveURL(/\/media-bots\?tab=tv/);
  await expect(page.getByRole('tab', { name: 'TV (IPTV)' })).toHaveAttribute('aria-selected', 'true');
  await page.goto('/settings?tab=youtube');
  await expect(page).toHaveURL(/\/media-bots\?tab=youtube/);
  await expect(page.getByRole('tab', { name: 'YouTube access' })).toHaveAttribute('aria-selected', 'true');
});

for (const [width, height] of [[1366, 768], [1024, 700]] as const) {
  test(`the whole menu fits without scrolling at ${width}x${height}`, async ({ page, request }) => {
    await page.setViewportSize({ width, height });
    await signInAsAdmin(page, request);
    await expect(page.locator('[data-nav-scroll-edge]')).toHaveCount(0);
  });
}

test('icon-only desktop navigation keeps every destination accessible', async ({ page, request }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await signInAsAdmin(page, request);
  await page.getByRole('button', { name: 'Collapse sidebar' }).click();
  for (const label of mainItems) await expect(page.getByRole('link', { name: label, exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'Library', exact: true }).hover();
  await expect(page.getByRole('tooltip')).toContainText('Library');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('mobile navigation lists the same short menu', async ({ page, request }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await signInAsAdmin(page, request);
  await page.getByRole('button', { name: 'Open navigation menu' }).click();
  const nav = page.getByRole('navigation', { name: 'Mobile navigation' });
  for (const label of mainItems.slice(0, 5)) await expect(nav.getByRole('link', { name: label, exact: true })).toBeVisible();
  await nav.getByRole('link', { name: 'Channels', exact: true }).click();
  await expect(page).toHaveURL('/channels');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
