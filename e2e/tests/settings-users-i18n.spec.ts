import { expect, test } from '@playwright/test';
import { attachPageErrorGuard, startTestServer } from '../helpers/fmd-server';

test('settings persistence, naming preview, admin user creation & RBAC UI, i18n, and browser history', async ({
  page,
}) => {
  const srv = await startTestServer();
  const assertNoPageErrors = attachPageErrorGuard(page);

  try {
    await page.goto(srv.baseUrl);
    await page.fill('#setup-username', 'admin');
    await page.fill('#setup-password', 'AdminPassword123!');
    await page.fill('#setup-confirm-password', 'AdminPassword123!');
    await page.click('#setup-wizard-form button[type="submit"]');
    await expect(page.locator('#main-app-container')).toBeVisible();

    // 1. Settings update + live naming preview + reload persistence
    await page.click('#tab-btn-settings');
    await expect(page.locator('#tab-settings')).toBeVisible();
    await page.fill('#setting-download-dir', 'e2e-downloads');
    await page.fill('#setting-naming-template', '{manga}/{number}');
    await expect(page.locator('#naming-preview')).toContainText('e2e-downloads/One Piece/1180.cbz');
    await page.selectOption('#setting-check-interval', '6');
    await page.selectOption('#setting-catalog-resync', '14');
    await page.click('#btn-save-settings');
    await expect(page.locator('#settings-alert')).toBeVisible();

    await page.reload();
    await page.click('#tab-btn-settings');
    await expect(page.locator('#setting-download-dir')).toHaveValue('e2e-downloads');
    await expect(page.locator('#setting-check-interval')).toHaveValue('6');
    await expect(page.locator('#setting-catalog-resync')).toHaveValue('14');

    // 2. Language switching (i18n) + persistence
    await page.selectOption('#nav-lang-select', 'en');
    await expect(page.locator('#tab-btn-favorites')).toContainText(/Favorites|Library/i);
    await page.selectOption('#nav-lang-select', 'fr');
    await expect(page.locator('#tab-btn-favorites')).toContainText(/Favoris|Bibliothèque/i);

    await page.reload();
    await expect(page.locator('#nav-lang-select')).toHaveValue('fr');

    // 3. Browser history navigation (pushState / popstate)
    await page.click('#tab-btn-modules');
    await expect(page.locator('#tab-modules')).toBeVisible();
    await page.click('#tab-btn-downloads');
    await expect(page.locator('#tab-downloads')).toBeVisible();
    await page.goBack();
    await expect(page.locator('#tab-modules')).toBeVisible();

    // 4. Admin creates a standard user -> standard user has admin tabs hidden
    await page.click('#tab-btn-users');
    await expect(page.locator('#tab-users')).toBeVisible();
    await page.click('button[onclick="openCreateUserModal()"]');
    await expect(page.locator('#user-modal')).toBeVisible();
    await page.fill('#user-form-username', 'reader1');
    await page.fill('#user-form-password', 'ReaderPass123!');
    await page.selectOption('#user-form-role', 'user');
    await page.click('#user-form button[type="submit"]');
    await expect(page.locator('#users-table-body')).toContainText('reader1');

    // Logout and login as reader1
    await page.click('button[onclick="logout()"]');
    await page.fill('#login-username', 'reader1');
    await page.fill('#login-password', 'ReaderPass123!');
    await page.click('#login-form button[type="submit"]');
    await expect(page.locator('#main-app-container')).toBeVisible();
    await expect(page.locator('#tab-btn-users')).toBeHidden();
    await expect(page.locator('#nav-role-badge')).toContainText(/user|utilisateur/i);

    assertNoPageErrors();
  } finally {
    await srv.stop();
  }
});
