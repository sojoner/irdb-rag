# Universal Search UI Implementation Guide

## Project Context

This is a Rust-based RAG (Retrieval Augmented Generation) system using:
- **ParadeDB** - PostgreSQL with pg_search (BM25) + pgvector for hybrid search
- **Leptos 0.8** - Rust web framework with SSR + WASM hydration
- **FastEmbed** - Local ONNX embeddings
- **OpenAI/Anthropic/OpenRouter** - LLM completion APIs

### Previous Session Issues

The previous coding session encountered Leptos 0.8 SSR/hydrate feature flag complexity. Key changes made:
1. Moved server-only dependencies to `optional = true` with `ssr` feature
2. Added `#[cfg(feature = "ssr")]` and `#[cfg_attr(feature = "ssr", derive(FromRow))]` guards
3. Attempted to fix WASM filename mismatches
4. Modified `getrandom` to use `wasm_js` feature for WASM compatibility

The build may have issues with:
- WASM binary naming (`rag-chat.wasm` vs `rag-chat_bg.wasm`)
- Feature flag inconsistencies between SSR and hydrate modes
- Potential runtime errors in the browser due to missing feature guards

---

## Recommended Approach: Simplified Architecture

**Key Insight from Greg Haggard's Leptos talk**: Start simple, iterate incrementally. The talk emphasized that Leptos supports three modes:
1. **CSR (Client-Side Rendering)** - Simple, no server complexity
2. **SSR (Server-Side Rendering)** - SEO benefits, initial HTML
3. **SSR + Hydration** - Full interactivity with SEO

**Recommendation**: Start with a **minimal SSR-only search component** that works via traditional form submission, then add interactivity incrementally.

---

## LOCAL example for look up and reference.

```bash
tree2 /Users/hagentonnies/Workspace/irdb/pg_search_tests 
/Users/hagentonnies/Workspace/irdb/pg_search_tests
├── .cargo
│   └── config.toml
├── data
│   └── products.json
├── public
├── sql_examples
│   ├── 00_setup_extensions.sql
│   ├── 01_fuzzy_search.sql
│   ├── 02_exact_term_search.sql
│   ├── 03_boolean_search.sql
│   ├── 04_phrase_search.sql
│   ├── 05_complete_setup.sql
│   ├── 06_numeric_range_search.sql
│   ├── 07_snippet_highlighting.sql
│   ├── 08_products_schema.sql
│   ├── 09_products_data.sql
│   ├── 10_bm25_search_tests.sql
│   ├── 11_vector_search_tests.sql
│   ├── 12_hybrid_search_tests.sql
│   ├── 13_facet_aggregation_tests.sql
│   ├── README.md
│   └── test_utils.sql
├── src
│   ├── bin
│   ├── fixtures
│   ├── web_app
│   └── lib.rs
├── target
│   ├── debug
│   ├── flycheck0
│   ├── front
│   ├── llvm-cov
│   ├── llvm-cov-target
│   ├── site
│   ├── tmp
│   ├── .DS_Store
│   ├── .future-incompat-report.json
│   └── .rustc_info.json
├── tests
│   ├── common
│   ├── advanced_search_tests.rs
│   ├── app_logic_tests.rs
│   ├── backend_search_tests.rs
│   ├── bm25_detailed_tests.rs
│   ├── common_components_tests.rs
│   ├── component_render_tests.rs
│   ├── dbtuning_test.rs
│   ├── init_db_test.rs
│   ├── integration_tests.rs
│   ├── products_bm25_test.rs
│   ├── products_facets_test.rs
│   ├── products_hybrid_test.rs
│   ├── products_vector_test.rs
│   ├── queries_comprehensive_test.rs
│   ├── search_components_tests.rs
│   ├── search_page_tests.rs
│   ├── server_fn_tests.rs
│   └── web_app_search_tests.rs
├── .gitignore
├── Cargo.lock
├── Cargo.toml
├── Leptos.toml
├── package-lock.json
├── package.json
├── postcss.config.js
├── README.md
└── tailwind.config.js

19 directories, 49 files
```

---

## Phase 1: Basic Search Bar + Results

### Goal
A search bar that:
1. Accepts user input
2. Makes an API call to `/api/search`
3. Renders results as a list

### Architecture Options

#### Option A: Server Function Approach (Recommended)
Use Leptos server functions to keep everything in Rust:

```rust
// src/web_app/components/search.rs
use leptos::prelude::*;

#[server(SearchDocuments, "/api")]
pub async fn search_documents(
    query: String,
    limit: i32,
) -> Result<Vec<SearchResult>, ServerFnError> {
    use crate::infra::db;
    use crate::services::embedder::Embedder;
    
    let pool = db::get_pool().await?;
    let embedder = Embedder::new()?;
    let embedding = embedder.embed(&query).await?;
    
    let results = db::hybrid_search(
        &pool,
        &query,
        &embedding,
        limit,
        0.5, // bm25_weight
        0.5, // vector_weight
        None, // filters
    ).await?;
    
    Ok(results)
}

#[component]
pub fn SearchBar() -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let search_action = ServerAction::<SearchDocuments>::new();
    
    // Get results from the action
    let results = search_action.value();
    
    view! {
        <div class="search-container">
            <form on:submit=move |ev| {
                ev.prevent_default();
                search_action.dispatch(SearchDocuments {
                    query: query.get(),
                    limit: 20,
                });
            }>
                <input 
                    type="text"
                    placeholder="Search documents..."
                    class="w-full px-4 py-2 border rounded-lg"
                    prop:value=query
                    on:input=move |ev| set_query.set(event_target_value(&ev))
                />
                <button type="submit" class="px-4 py-2 bg-blue-500 text-white rounded-lg">
                    "Search"
                </button>
            </form>
            
            <Suspense fallback=move || view! { <p>"Loading..."</p> }>
                {move || results.get().map(|res| match res {
                    Ok(items) => view! {
                        <SearchResults results=items />
                    }.into_any(),
                    Err(e) => view! {
                        <p class="text-red-500">{e.to_string()}</p>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
fn SearchResults(results: Vec<SearchResult>) -> impl IntoView {
    view! {
        <ul class="mt-4 space-y-2">
            {results.into_iter().map(|result| view! {
                <li class="p-4 bg-white rounded-lg shadow">
                    <h3 class="font-bold">{result.title}</h3>
                    <p class="text-gray-600 text-sm">{result.snippet}</p>
                    <span class="text-xs text-blue-500">
                        "Score: " {format!("{:.2}", result.combined_score)}
                    </span>
                </li>
            }).collect_view()}
        </ul>
    }
}
```

#### Option B: Pure REST API Approach
If server functions are problematic, use `reqwest` from the client:

```rust
#[component]
pub fn SearchBar() -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let (results, set_results) = signal(Vec::<SearchResult>::new());
    let (loading, set_loading) = signal(false);
    
    let do_search = move |_| {
        let q = query.get();
        spawn_local(async move {
            set_loading.set(true);
            
            // Use gloo-net or reqwest for client-side fetch
            let resp = gloo_net::http::Request::post("/api/search")
                .json(&SearchRequest { query: q, limit: 20, ..Default::default() })
                .unwrap()
                .send()
                .await;
            
            match resp {
                Ok(r) => {
                    if let Ok(data) = r.json::<Vec<SearchResult>>().await {
                        set_results.set(data);
                    }
                }
                Err(e) => log::error!("Search failed: {:?}", e),
            }
            
            set_loading.set(false);
        });
    };
    
    // ... rest of view
}
```

### Debouncing for Typeahead

Add debouncing to avoid excessive API calls:

```rust
use leptos::prelude::*;
use std::time::Duration;

#[component]
pub fn DebouncedSearch() -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let (debounced_query, set_debounced_query) = signal(String::new());
    
    // Effect that debounces the query
    Effect::new(move |_| {
        let q = query.get();
        set_timeout(
            move || set_debounced_query.set(q.clone()),
            Duration::from_millis(300),
        );
    });
    
    // Resource that reacts to debounced query
    let search_results = Resource::new(
        move || debounced_query.get(),
        |q| async move {
            if q.is_empty() {
                return Ok(vec![]);
            }
            search_documents(q, 20).await
        }
    );
    
    // ... view implementation
}
```

### Files to Create/Modify

1. **`src/web_app/components/search_bar.rs`** - New search component
2. **`src/web_app/components/search_results.rs`** - Results display component
3. **`src/web_app/components/mod.rs`** - Export new components
4. **`src/web_app/pages/search.rs`** - Update to use new components

---

## Phase 2: LLM Moderation + Document Details

### LLM Result Summary

Add a server function that calls the LLM to generate a moderation/summary:

```rust
#[server(SummarizeResults, "/api")]
pub async fn summarize_results(
    query: String,
    results: Vec<SearchResult>,
) -> Result<String, ServerFnError> {
    use crate::services::llm::LLMClient;
    
    let client = LLMClient::from_env();
    
    let context = results.iter()
        .take(5)
        .map(|r| format!("Title: {}\nSnippet: {}", r.title, r.snippet))
        .collect::<Vec<_>>()
        .join("\n---\n");
    
    let prompt = format!(
        "Based on the following search results for query '{}', provide a brief summary:\n\n{}",
        query, context
    );
    
    let summary = client.complete(&prompt).await?;
    Ok(summary)
}
```

### Document Preview Modal

```rust
#[component]
pub fn DocumentPreview(
    #[prop(into)] document_id: Signal<Option<Uuid>>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let document = Resource::new(
        move || document_id.get(),
        |id| async move {
            match id {
                Some(id) => get_document(id).await,
                None => Ok(None),
            }
        }
    );
    
    view! {
        <Show when=move || document_id.get().is_some()>
            <div class="fixed inset-0 bg-black/50 flex items-center justify-center">
                <div class="bg-white rounded-lg w-3/5 h-4/5 p-6 overflow-auto">
                    <button 
                        class="absolute top-4 right-4"
                        on:click=move |_| on_close.run(())
                    >
                        "×"
                    </button>
                    
                    <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                        {move || document.get().map(|res| match res {
                            Ok(Some(doc)) => view! {
                                <DocumentDetails doc=doc />
                            }.into_any(),
                            _ => view! { <p>"Document not found"</p> }.into_any(),
                        })}
                    </Suspense>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn DocumentDetails(doc: Document) -> impl IntoView {
    view! {
        <article>
            <h1 class="text-2xl font-bold mb-4">{doc.title}</h1>
            
            <div class="grid grid-cols-2 gap-4 mb-6 text-sm">
                <div><strong>"Type:"</strong> " " {doc.source_type}</div>
                <div><strong>"Words:"</strong> " " {doc.word_count.unwrap_or(0)}</div>
                <div><strong>"Created:"</strong> " " {doc.created_at.to_string()}</div>
                <div><strong>"Author:"</strong> " " {doc.author.unwrap_or_default()}</div>
            </div>
            
            <Show when=move || doc.summary.is_some()>
                <div class="mb-4 p-4 bg-gray-100 rounded">
                    <h3 class="font-semibold">"Summary"</h3>
                    <p>{doc.summary.clone()}</p>
                </div>
            </Show>
            
            // Keywords as tags
            <div class="flex flex-wrap gap-2 mb-4">
                {doc.keywords.unwrap_or_default().iter().map(|kw| view! {
                    <span class="px-2 py-1 bg-blue-100 text-blue-800 rounded text-xs">
                        {kw}
                    </span>
                }).collect_view()}
            </div>
            
            // Locations as tags
            <div class="flex flex-wrap gap-2 mb-4">
                {doc.locations.unwrap_or_default().iter().map(|loc| view! {
                    <span class="px-2 py-1 bg-green-100 text-green-800 rounded text-xs">
                        "📍 " {loc}
                    </span>
                }).collect_view()}
            </div>
            
            // Full content
            <div class="prose max-w-none">
                {doc.content}
            </div>
        </article>
    }
}
```

---

## Phase 3: Aggregation Stats + Graph Visualization

### Entity Aggregation Display

Use the existing `/api/aggregation-stats` endpoint:

```rust
#[component]
pub fn AggregationSidebar() -> impl IntoView {
    let stats = Resource::new(|| (), |_| get_aggregation_stats());
    
    view! {
        <aside class="w-64 p-4 bg-gray-50">
            <Suspense fallback=|| view! { <p>"Loading stats..."</p> }>
                {move || stats.get().map(|res| match res {
                    Ok(s) => view! {
                        <div class="space-y-4">
                            <AggregationSection title="Categories" items=s.categories />
                            <AggregationSection title="Organizations" items=s.organizations />
                            <AggregationSection title="Persons" items=s.persons />
                            <AggregationSection title="Locations" items=s.locations />
                            <AggregationSection title="Keywords" items=s.keywords />
                        </div>
                    }.into_any(),
                    Err(_) => view! { <p>"Failed to load"</p> }.into_any(),
                })}
            </Suspense>
        </aside>
    }
}

#[component]
fn AggregationSection(
    title: &'static str,
    items: Vec<(String, i64)>,
) -> impl IntoView {
    view! {
        <div>
            <h3 class="font-semibold text-sm text-gray-700 mb-2">{title}</h3>
            <ul class="space-y-1">
                {items.into_iter().take(10).map(|(name, count)| view! {
                    <li class="flex justify-between text-sm">
                        <span class="truncate">{name}</span>
                        <span class="text-gray-500">{count}</span>
                    </li>
                }).collect_view()}
            </ul>
        </div>
    }
}
```

### Graph Visualization with D3.js

For the cluster visualization, embed D3.js in a Leptos component:

```rust
// src/web_app/components/similarity_graph.rs
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/static/js/similarity_graph.js")]
extern "C" {
    fn renderSimilarityGraph(containerId: &str, data: &str);
}

#[component]
pub fn SimilarityGraph(results: Vec<SearchResult>) -> impl IntoView {
    let graph_id = "similarity-graph";
    
    Effect::new(move |_| {
        // Compute similarity matrix
        let nodes: Vec<_> = results.iter().enumerate().map(|(i, r)| {
            serde_json::json!({
                "id": i,
                "title": r.title,
                "score": r.combined_score,
            })
        }).collect();
        
        // Create links based on cosine similarity
        // (In practice, you'd compute this server-side)
        let links: Vec<_> = vec![]; // Populated from similarity computation
        
        let graph_data = serde_json::json!({
            "nodes": nodes,
            "links": links,
        });
        
        renderSimilarityGraph(graph_id, &graph_data.to_string());
    });
    
    view! {
        <div id=graph_id class="w-full h-96 border rounded-lg"></div>
    }
}
```

**D3.js JavaScript file** (`static/js/similarity_graph.js`):

```javascript
export function renderSimilarityGraph(containerId, dataJson) {
    const data = JSON.parse(dataJson);
    const container = document.getElementById(containerId);
    
    // Clear previous
    container.innerHTML = '';
    
    const width = container.clientWidth;
    const height = container.clientHeight;
    
    const svg = d3.select(`#${containerId}`)
        .append('svg')
        .attr('width', width)
        .attr('height', height);
    
    const simulation = d3.forceSimulation(data.nodes)
        .force('link', d3.forceLink(data.links).id(d => d.id).distance(100))
        .force('charge', d3.forceManyBody().strength(-200))
        .force('center', d3.forceCenter(width / 2, height / 2));
    
    const link = svg.append('g')
        .selectAll('line')
        .data(data.links)
        .enter().append('line')
        .attr('stroke', '#999')
        .attr('stroke-opacity', d => d.similarity);
    
    const node = svg.append('g')
        .selectAll('circle')
        .data(data.nodes)
        .enter().append('circle')
        .attr('r', d => 5 + d.score * 10)
        .attr('fill', '#3b82f6')
        .call(d3.drag()
            .on('start', dragstarted)
            .on('drag', dragged)
            .on('end', dragended));
    
    node.append('title')
        .text(d => d.title);
    
    simulation.on('tick', () => {
        link
            .attr('x1', d => d.source.x)
            .attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x)
            .attr('y2', d => d.target.y);
        
        node
            .attr('cx', d => d.x)
            .attr('cy', d => d.y);
    });
    
    function dragstarted(event) {
        if (!event.active) simulation.alphaTarget(0.3).restart();
        event.subject.fx = event.subject.x;
        event.subject.fy = event.subject.y;
    }
    
    function dragged(event) {
        event.subject.fx = event.x;
        event.subject.fy = event.y;
    }
    
    function dragended(event) {
        if (!event.active) simulation.alphaTarget(0);
        event.subject.fx = null;
        event.subject.fy = null;
    }
}
```

### Server-Side Similarity Computation

Add an endpoint to compute pairwise similarities:

```rust
// src/api/handlers.rs
pub async fn compute_similarities(
    State(state): State<AppState>,
    Json(doc_ids): Json<Vec<Uuid>>,
) -> Result<Json<SimilarityMatrix>, AppError> {
    let embeddings = db::get_embeddings_for_docs(&state.pool, &doc_ids).await?;
    
    let mut similarities = Vec::new();
    for i in 0..embeddings.len() {
        for j in (i+1)..embeddings.len() {
            let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
            if sim > 0.5 { // Only include significant similarities
                similarities.push(SimilarityEdge {
                    source: i,
                    target: j,
                    similarity: sim,
                });
            }
        }
    }
    
    Ok(Json(SimilarityMatrix { edges: similarities }))
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    (dot / (norm_a * norm_b)) as f64
}
```

---

## Implementation Order

### Step 1: Fix Build Issues
1. Verify Cargo.toml feature flags are correct
2. Test SSR build: `cargo build --features ssr`
3. Test WASM build: `cargo build --target wasm32-unknown-unknown --features hydrate`
4. Use `cargo-leptos` for unified builds: `cargo leptos build`

### Step 2: Create Minimal Search Component
1. Create `src/web_app/components/search_bar.rs`
2. Add server function for search
3. Render basic results list
4. Test form submission works

### Step 3: Add Debouncing + Loading States
1. Implement debounced input signal
2. Add loading spinner
3. Handle empty states gracefully

### Step 4: Document Preview Modal
1. Create modal component
2. Implement document fetch
3. Style the preview panel

### Step 5: LLM Summary
1. Add server function for summarization
2. Display summary above results
3. Add streaming support if needed

### Step 6: Aggregation Sidebar
1. Fetch aggregation stats
2. Display entity counts
3. Make items clickable as filters

### Step 7: Similarity Graph
1. Add D3.js to static assets
2. Create graph component
3. Compute similarities server-side
4. Wire up interactive graph

---

## Key Dependencies

```toml
# Cargo.toml additions for client-side networking
[dependencies]
gloo-net = { version = "0.6", optional = true }
gloo-timers = { version = "0.3", optional = true }

[features]
hydrate = [
    # ... existing
    "gloo-net",
    "gloo-timers",
]
```

```json
// package.json - add D3 for visualization
{
  "dependencies": {
    "d3": "^7.0.0"
  }
}
```

---

## Testing Approach

1. **Unit Tests**: Test search logic in isolation
2. **Integration Tests**: Test API endpoints with database
3. **E2E Tests**: Use `cargo-leptos` watch mode to manually verify

```bash
# Development workflow
cargo leptos watch  # Auto-rebuilds on changes

# Or manual builds
cargo build --features ssr  # Server binary
trunk build --features hydrate  # WASM bundle
```

---

## Troubleshooting Common Issues

### "Cannot find module" in WASM
- Check `#[cfg(feature = "ssr")]` guards on server-only code
- Ensure `reqwest` uses `native-tls-vendored` for server, not for WASM

### Hydration Mismatch
- Ensure server and client render identical HTML
- Use `Suspense` boundaries around async data

### Server Function Not Found
- Check route registration in `src/api/routes.rs`
- Verify `ServerFn` derive macro syntax

### D3 Not Rendering
- Ensure script is loaded before component mounts
- Check `Effect` timing for DOM availability

---

## References

- [Leptos Book - Server Functions](https://book.leptos.dev/server/25_server_functions.html)
- [Leptos Book - Resources](https://book.leptos.dev/async/10_resources.html)
- [Leptos Book - Actions](https://book.leptos.dev/async/13_actions.html)
- [D3 Force-Directed Graphs](https://d3js.org/d3-force)
- [Greg Haggard's Rust Nation Talk](transcript provided in context)

---

## Summary

This guide provides a phased approach to building a universal search UI:

1. **Phase 1**: Basic search with server functions + result rendering
2. **Phase 2**: LLM moderation + document detail modal
3. **Phase 3**: Aggregation stats + D3.js similarity graph

Start simple, verify each phase works, then iterate. The key is to avoid fighting Leptos's SSR/hydrate complexity by starting with server functions and progressive enhancement.
---