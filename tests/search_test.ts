import { test, expect } from '@playwright/test';

test.describe('Search Functionality', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:3000');
    await page.waitForLoadState('networkidle');
  });

  test('search input should be visible', async ({ page }) => {
    const searchInput = page.locator('input[placeholder*="Search documents"]');
    await expect(searchInput).toBeVisible();
  });

  test('entering search query should trigger search', async ({ page }) => {
    const searchInput = page.locator('input[placeholder*="Search documents"]');

    // Type search query
    await searchInput.fill('test');

    // Check if there's a loading indicator
    const loadingIndicator = page.locator('text=Searching');

    // Wait for either results or timeout
    const resultsArea = page.locator('text=found');

    // Give it time to search
    await page.waitForTimeout(3000);

    // Check network activity
    const requests = await page.context().storageState();
    console.log('Page state:', requests);
  });

  test('search should return results on Enter', async ({ page }) => {
    const searchInput = page.locator('input[placeholder*="Search documents"]');

    await searchInput.fill('document');
    await searchInput.press('Enter');

    // Wait for results
    await page.waitForTimeout(2000);

    // Check if results section shows count
    const resultsHeader = page.locator('text="Discovery & Context"').locator('..').locator('text=/\\d+ found/');
    const isVisible = await resultsHeader.isVisible().catch(() => false);

    console.log('Results visible:', isVisible);

    // Also check the network tab for API calls
    const allText = await page.locator('body').textContent();
    console.log('Page contains results indicator:', allText?.includes('found') ? 'YES' : 'NO');
  });

  test('network debugging - check API calls', async ({ page }) => {
    let apiCallCount = 0;
    let errors: string[] = [];

    page.on('response', response => {
      if (response.url().includes('/api')) {
        apiCallCount++;
        console.log(`API Call: ${response.url()} - Status: ${response.status()}`);
        if (!response.ok()) {
          errors.push(`${response.url()}: ${response.status()}`);
        }
      }
    });

    const searchInput = page.locator('input[placeholder*="Search documents"]');
    await searchInput.fill('test search query');
    await searchInput.press('Enter');

    // Wait for API calls to complete
    await page.waitForTimeout(3000);

    console.log(`Total API calls: ${apiCallCount}`);
    console.log(`Errors: ${errors.join(', ')}`);

    expect(apiCallCount).toBeGreaterThan(0);
  });
});
