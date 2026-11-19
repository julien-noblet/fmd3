import { expect, test } from '@playwright/test';
import { attachPageErrorGuard, startTestServer } from '../helpers/fmd-server';

test('BASE_PATH=/reader serves prefixed assets, API routes, and WebSocket', async ({ page }) => {
  const srv = await startTestServer({ basePath: '/reader' });
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await page.goto(`${srv.baseUrl}/reader/`);
    await expect(page.locator('#screen-setup-wizard')).toBeVisible();

    // Verify logo asset loads cleanly under /reader/logo.png
    const logoRes = await page.request.get(`${srv.baseUrl}/reader/logo.png`);
    expect(logoRes.status()).toBe(200);
    expect(logoRes.headers()['content-type']).toContain('image/png');

    await page.fill('#setup-username', 'reader_admin');
    await page.fill('#setup-password', 'ReaderAdmin123!');
    await page.fill('#setup-confirm-password', 'ReaderAdmin123!');
    await page.click('#setup-wizard-form button[type="submit"]');
    await expect(page.locator('#main-app-container')).toBeVisible();
    await expect(page.locator('#ws-badge')).toContainText(/Connecté|Connected|接続済み/i);

    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});

test('demo mode isolates concurrent browser contexts and disables interval selects', async ({
  browser,
}) => {
  const srv = await startTestServer({ demoMode: true });
  const ctx1 = await browser.newContext();
  const ctx2 = await browser.newContext();
  const page1 = await ctx1.newPage();
  const page2 = await ctx2.newPage();
  const guard1 = attachPageErrorGuard(page1);
  const guard2 = attachPageErrorGuard(page2);

  try {
    // Visitor 1 enters demo, signs in with pre-filled demo account, dismisses dialog, adds favorite, checks disabled settings selects
    await page1.goto(srv.baseUrl);
    await expect(page1.locator('#login-username')).toHaveValue(/^demo-/, { timeout: 15_000 });
    await page1.click('#login-form button[type="submit"]');
    await expect(page1.locator('#main-app-container')).toBeVisible({ timeout: 15_000 });
    const dialog1 = page1.locator('#demo-dialog');
    if (await dialog1.isVisible()) {
      await page1.click('#demo-dialog button');
    }

    await page1.click('#tab-btn-settings');
    await expect(page1.locator('#setting-check-interval')).toBeDisabled();
    await expect(page1.locator('#setting-catalog-resync')).toBeDisabled();

    const addStatus = await page1.evaluate(async () => {
      const res = await fetch('/api/favorites', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          module_id: 'fixture-mod',
          url: '/manga/demo-one',
          title: 'Demo Manga',
        }),
      });
      return res.status;
    });
    expect(addStatus).toBe(200);
    await page1.reload();
    await expect(page1.locator('#main-app-container')).toBeVisible({ timeout: 15_000 });
    await page1.click('#tab-btn-favorites');
    await expect(page1.locator('#favorites-list')).toContainText('Demo Manga');

    // Visitor 2 enters demo in a separate browser context -> does NOT see Visitor 1's favorite
    await page2.goto(srv.baseUrl);
    await expect(page2.locator('#login-username')).toHaveValue(/^demo-/, { timeout: 15_000 });
    await page2.click('#login-form button[type="submit"]');
    await expect(page2.locator('#main-app-container')).toBeVisible({ timeout: 15_000 });
    const dialog2 = page2.locator('#demo-dialog');
    if (await dialog2.isVisible()) {
      await page2.click('#demo-dialog button');
    }
    await page2.click('#tab-btn-favorites');
    await expect(page2.locator('#favorites-list')).not.toContainText('Demo Manga');

    guard1();
    guard2();
  } finally {
    await ctx1.close();
    await ctx2.close();
    await srv.stop();
  }
});

test('SFW mode hides adult modules in UI and blocks adult-genre manga fetch', async ({ page }) => {
  const srv = await startTestServer({ sfwMode: true, includeAdultModule: true });
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await page.goto(srv.baseUrl);
    await page.fill('#setup-username', 'sfw_admin');
    await page.fill('#setup-password', 'SfwPassword123!');
    await page.fill('#setup-confirm-password', 'SfwPassword123!');
    await page.click('#setup-wizard-form button[type="submit"]');
    await expect(page.locator('#main-app-container')).toBeVisible();

    await page.click('#tab-btn-search');
    await expect(page.locator('#fetch-module-select option[value="fixture-mod"]')).toBeAttached();
    await expect(
      page.locator('#fetch-module-select option[value="nhentai-fixture"]'),
    ).toHaveCount(0);

    // Adult-genre manga fetch returns 403
    const status = await page.evaluate(async (adultUrl) => {
      const res = await fetch('/api/manga/fetch', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ module_id: 'fixture-mod', url: adultUrl }),
      });
      return res.status;
    }, `${srv.originUrl}/manga/adult-one`);
    expect(status).toBe(403);

    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});

test('PostgreSQL E2E smoke suite (when FMD_TEST_POSTGRES_URL is set)', async ({ page }) => {
  const pgUrl = process.env.FMD_TEST_POSTGRES_URL;
  test.skip(!pgUrl, 'FMD_TEST_POSTGRES_URL not configured');

  const srv = await startTestServer({ databaseUrl: pgUrl, demoMode: true });
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await page.goto(srv.baseUrl);
    await expect(page.locator('#main-app-container')).toBeVisible({ timeout: 15_000 });
    const dialog = page.locator('#demo-dialog');
    if (await dialog.isVisible()) {
      await page.click('#demo-dialog button');
    }
    await page.click('#tab-btn-settings');
    await page.fill('#setting-download-dir', 'pg-e2e-dl');
    await page.click('#btn-save-settings');
    await expect(page.locator('#settings-alert')).toBeVisible();

    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});
