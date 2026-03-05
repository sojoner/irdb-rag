# Wikipedia Import Evaluation (Performance & Depth)

## Performance Benchmarks
- [ ] **Throughput:** Sustained >20,000 pages/sec.
- [ ] **Job Integration:** UI correctly shows progress percentage as pages are processed.
- [ ] **No Embedding Overhead:** Verify LLM/GPU usage is zero during Wikipedia import.

## Quality Metrics
- [ ] **Infobox Coverage:** >90% of pages with infoboxes have structured JSON data.
- [ ] **BM25 Search:** `Relativity` query returns Albert Einstein as top result.

## UI Verification
- [ ] "Wikipedia" source type appears in "New Import Job" modal.
- [ ] Providing a valid path starts a background job visible in the list.
- [ ] Progress bar updates smoothly without page refreshes.
