# Query Builder E2E Tests (Playwright)

These tests require the server running: `cargo leptos serve` or `make gpu-up`

## Test Scenarios

### 1. Query Builder Loads Correctly
```
Navigate to: http://localhost:3000/search
Verify:
- Advanced Query Builder panel is visible
- "+ Date Range", "+ Text Field", "+ Array Field" buttons exist
- Search input field is present
- Sort dropdown is present
```

### 2. Add Date Range Filter
```
Click: "+ Date Range" button
Verify:
- Date filter row appears with From/To date inputs
- Quick select buttons: "1 Day", "Last Week", "Last Month", "Last Year"
- Remove (X) button is present

Action: Click "Last Week" preset
Verify:
- From date is set to 7 days ago
- To date is set to today

Action: Click remove button
Verify:
- Date filter row is removed
```

### 3. Add Text Field Filter
```
Click: "+ Text Field" button
Verify:
- Text filter row appears
- Field dropdown with options: title, content, summary, author
- Search text input field
- Remove button present

Action: Select "title" from dropdown, type "machine learning"
Verify:
- Field value saved
- Search triggers with query containing title:"machine learning"
```

### 4. Add Array Field Filter (Keywords)
```
Click: "+ Array Field" button
Verify:
- Array filter row appears
- Field dropdown: keywords, locations, persons, organizations, products, concepts
- Search & Select input field

Action: Select "keywords", type "Python"
Verify:
- Suggestion dropdown appears with mock suggestions
- "Python" chip/tag added when selected

Action: Add "AI" keyword
Verify:
- Multiple chips displayed: "Python", "AI"

Action: Click X on "Python" chip
Verify:
- Only "AI" chip remains
```

### 5. Combined Filters Search
```
Add:
1. Date filter: Last Month
2. Text filter: author = "John Doe"
3. Array filter: organizations = ["OpenAI"]

Click search or type query
Verify:
- Search request includes all filter parameters
- Results reflect combined filters
```

### 6. Sort Order Changes
```
Select sort dropdown:
- "Relevance" (default)
- "Newest First"
- "Oldest First"
- "Title A-Z"
- "Title Z-A"

Verify:
- Search re-triggers with new sort order
- Results order changes appropriately
```

### 7. Empty Filter Handling
```
Add text filter, leave value empty
Verify:
- Empty filter is ignored in search request
- No error thrown

Add array filter, don't select any values
Verify:
- Empty array filter ignored
```

### 8. Multiple Filters of Same Type
```
Add multiple text filters:
- title = "Rust"
- author = "Jane"

Verify:
- Both filters appear as separate rows
- Both contribute to search query
```

## Running Tests

### Manual with Playwright MCP
Start server first:
```bash
cargo leptos serve
# or
make gpu-up
```

Then use Claude Code Playwright tools to:
1. `browser_navigate` to http://localhost:3000/search
2. `browser_snapshot` to see current state
3. `browser_click` on buttons
4. `browser_type` in input fields
5. `browser_take_screenshot` to capture results

### Automated (Future)
Could be converted to a proper Playwright test suite:
```bash
npx playwright test tests/playwright/*.spec.ts
```
