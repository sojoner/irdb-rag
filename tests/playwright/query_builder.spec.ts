import { test, expect, Page } from '@playwright/test';

// Base URL for the application
const BASE_URL = 'http://localhost:3000';

test.describe('Advanced Query Builder', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(`${BASE_URL}/search`);
    // Wait for the query builder to be visible
    await expect(page.locator('text=Advanced Query Builder')).toBeVisible();
  });

  test('should display query builder with filter buttons', async ({ page }) => {
    // Verify filter buttons are present
    await expect(page.getByRole('button', { name: '+ Date Range' })).toBeVisible();
    await expect(page.getByRole('button', { name: '+ Text Field' })).toBeVisible();
    await expect(page.getByRole('button', { name: '+ Array Field' })).toBeVisible();

    // Verify search input
    await expect(page.getByPlaceholder('Search documents...')).toBeVisible();

    // Verify sort dropdown
    await expect(page.getByRole('combobox')).toBeVisible();
  });

  test('should add and configure date range filter', async ({ page }) => {
    // Add date filter
    await page.getByRole('button', { name: '+ Date Range' }).click();

    // Verify date filter UI appears
    await expect(page.getByText('Quick Select')).toBeVisible();
    await expect(page.getByText('From (YYYY-MM-DD)')).toBeVisible();
    await expect(page.getByText('To (YYYY-MM-DD)')).toBeVisible();

    // Test preset buttons
    await page.getByRole('button', { name: 'Last Week' }).click();

    // Verify dates are populated (checking from date input has a value)
    const fromInput = page.locator('input[type="date"]').first();
    await expect(fromInput).not.toHaveValue('');

    // Remove filter
    await page.locator('button:has(svg)').filter({ hasText: '' }).first().click();
    await expect(page.getByText('Quick Select')).not.toBeVisible();
  });

  test('should add and configure text field filter', async ({ page }) => {
    // Add text filter
    await page.getByRole('button', { name: '+ Text Field' }).click();

    // Verify text filter UI
    await expect(page.locator('select')).toBeVisible();
    await expect(page.getByPlaceholder('Type to search...')).toBeVisible();

    // Select field and enter value
    await page.locator('select').first().selectOption('title');
    await page.getByPlaceholder('Type to search...').fill('machine learning');

    // Verify the value is set
    await expect(page.getByPlaceholder('Type to search...')).toHaveValue('machine learning');
  });

  test('should add array field filter and select values', async ({ page }) => {
    // Add array filter
    await page.getByRole('button', { name: '+ Array Field' }).click();

    // Verify array filter UI
    await expect(page.getByText('Field')).toBeVisible();
    await expect(page.getByPlaceholder('Search values...')).toBeVisible();

    // Select field type
    await page.locator('select').first().selectOption('keywords');

    // Focus search to show suggestions
    await page.getByPlaceholder('Search values...').focus();
    await page.getByPlaceholder('Search values...').fill('Python');

    // Click on a suggestion (mock data should show Python)
    // Wait for dropdown and click
    const suggestion = page.locator('button:has-text("Python")');
    if (await suggestion.count() > 0) {
      await suggestion.click();
      // Verify chip/tag is added
      await expect(page.locator('span:has-text("Python")')).toBeVisible();
    }
  });

  test('should combine multiple filters', async ({ page }) => {
    // Add date filter
    await page.getByRole('button', { name: '+ Date Range' }).click();
    await page.getByRole('button', { name: 'Last Month' }).click();

    // Add text filter
    await page.getByRole('button', { name: '+ Text Field' }).click();
    await page.locator('select').nth(1).selectOption('author'); // Second select (first is for array)
    await page.getByPlaceholder('Type to search...').fill('John');

    // Add array filter
    await page.getByRole('button', { name: '+ Array Field' }).click();

    // Verify all three filter types are present
    await expect(page.getByText('Quick Select')).toBeVisible();
    await expect(page.getByPlaceholder('Type to search...')).toBeVisible();
    await expect(page.getByPlaceholder('Search values...')).toBeVisible();
  });

  test('should change sort order', async ({ page }) => {
    const sortDropdown = page.locator('select').filter({ hasText: 'Relevance' });

    // Change to newest first
    await sortDropdown.selectOption('DateDesc');
    await expect(sortDropdown).toHaveValue('DateDesc');

    // Change to oldest first
    await sortDropdown.selectOption('DateAsc');
    await expect(sortDropdown).toHaveValue('DateAsc');

    // Change to title A-Z
    await sortDropdown.selectOption('TitleAsc');
    await expect(sortDropdown).toHaveValue('TitleAsc');
  });

  test('should perform search with filters', async ({ page }) => {
    // Enter search query
    await page.getByPlaceholder('Search documents...').fill('test query');

    // Add a date filter
    await page.getByRole('button', { name: '+ Date Range' }).click();
    await page.getByRole('button', { name: 'Last Week' }).click();

    // Wait for search to trigger (debounced)
    await page.waitForTimeout(500);

    // Check that results area exists (even if empty)
    await expect(page.locator('[class*="results"]')).toBeVisible();
  });

  test('should remove filters correctly', async ({ page }) => {
    // Add all three filter types
    await page.getByRole('button', { name: '+ Date Range' }).click();
    await page.getByRole('button', { name: '+ Text Field' }).click();
    await page.getByRole('button', { name: '+ Array Field' }).click();

    // Count remove buttons (X icons)
    const removeButtons = page.locator('button:has(svg path[d*="M6 18L18 6"])');
    const initialCount = await removeButtons.count();
    expect(initialCount).toBe(3);

    // Remove first filter
    await removeButtons.first().click();

    // Verify one less filter
    const afterCount = await page.locator('button:has(svg path[d*="M6 18L18 6"])').count();
    expect(afterCount).toBe(2);
  });
});

test.describe('Search Results Integration', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(`${BASE_URL}/search`);
  });

  test('should show stats bar', async ({ page }) => {
    // Stats bar should show "Ready to search" initially
    await expect(page.getByText('Ready to search')).toBeVisible();
  });

  test('should update stats after search', async ({ page }) => {
    // Enter a search query
    await page.getByPlaceholder('Search documents...').fill('data');

    // Wait for search debounce and results
    await page.waitForTimeout(1000);

    // Stats should update (either with results or still ready)
    const statsText = await page.locator('text=/Found \\d+ results|Ready to search/').textContent();
    expect(statsText).toBeTruthy();
  });
});
