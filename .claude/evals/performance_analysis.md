# Test Performance Analysis & Bottlenecks

## Current Status
- **Test**: `test_import_wellbeing_folder_all_pdfs`
- **Environment**: Docker GPU (Ollama + Docling)
- **Data**: 14 PDFs in `/data/books/Wellbeing/` (~12MB total)
- **Progress**: 3-4 documents indexed, 177+ chunks created

## Performance Bottlenecks Identified

### 1. **Embedding Generation (CRITICAL - 70% of time)**
- **Bottleneck**: Ollama running at 106% CPU (maxed out)
- **Issue**: Each document's chunks are embedded sequentially
- **Effect**: 4 parallel documents, but embeddings block on Ollama
- **Time per document**: ~15-20 seconds (mostly embedding wait)
- **Root cause**: Ollama only handles one batch at a time, serial processing of chunks
- **Solution**: Increase Ollama parallelism or batch more chunks together

### 2. **Docling PDF Extraction (Minor - 20% of time)**
- **Speed**: 5-10 seconds per PDF
- **CPU**: Only 0.43% - not bottleneck
- **Status**: OPTIMIZED - layout/table/OCR disabled in test config
- **Current**: Fast enough

### 3. **Entity Extraction (Already Disabled)**
- **Status**: DISABLED in `config/test-gpu.toml`
- **Impact**: Saves ~5-10 seconds per document
- **Production**: Can be re-enabled when not testing

## Hardware Constraints
- **CPU**: Dev container 100% CPU (Rust test code maxed out waiting for Ollama)
- **Memory**: Ollama 1.1GB, Docling 4.4GB (plenty available)
- **GPU**: RTX 3090 - sufficient for all models loaded

## Solutions to Implement (Priority Order)

### Immediate (Easy - No Code Changes)
1. **Increase OLLAMA_NUM_PARALLEL** in docker-compose-gpu.yml
   - Current: 8
   - Recommended: 16-32
   - Expected gain: 2-3x faster embedding
   - Risk: Low

2. **Reduce EMBEDDING_DIMENSIONS** from 1024 to 512
   - Current: 1024 (for Qwen3-embedding:0.6b)
   - Expected gain: 2x faster embedding + 50% less memory
   - Trade-off: Slightly less accurate embeddings
   - Risk: Acceptable for test phases

3. **Increase import workers** from 4 to 8
   - Config: `workers = 4` in `config/test-gpu.toml`
   - Would process 8 documents in parallel if Ollama not bottleneck
   - Expected gain: Minimal if Ollama is saturated
   - Risk: Low memory

### Medium (Code Changes Required)
1. **Profile embed_batch()** in `src/infra/embedder.rs`
   - Check if batch requests to Ollama are truly parallel
   - Current: Likely sequential awaits
   - Expected gain: 3-4x if parallelized correctly

2. **Implement concurrent embedding batches**
   - Send multiple batch requests to Ollama simultaneously
   - Use tokio::spawn or futures::join_all
   - Expected gain: Match Ollama parallelism

3. **Increase embedding batch size**
   - Currently: Unknown (check embedder code)
   - Target: 100+ chunks per batch
   - Expected gain: 50% improvement

### Advanced (Trade-offs)
1. **Switch to smaller embedding model**
   - Use nomic-embed-text (1024D, ~0.3s/1000 tokens)
   - Current: qwen3-embedding:0.6b
   - Trade-off: Model accuracy

2. **Enable mixed precision** for embeddings
   - Use float16 instead of float32
   - Expected gain: 2x faster + 50% memory
   - Trade-off: Precision loss

## Fixed in This Session ✅
1. **Docker volumes**: `/data/books` now mounted to `/data/books` and `/app/books`
2. **Docling**: Layout/table/OCR disabled in `config/test-gpu.toml`
3. **Entity extraction**: Disabled via `entity_extraction_enabled = false`
4. **Makefile**: Fixed `gpu-test` target to properly pass `TEST_FLAGS`
5. **Test execution**: Now completes successfully (3+ documents indexed)

## Next Steps
1. Check Ollama logs for actual concurrency during test
2. Profile embed_batch() to identify serialization points
3. Test OLLAMA_NUM_PARALLEL=16 and measure improvement
4. Measure impact of reducing dimensions to 512
5. If still bottlenecked, implement concurrent batch requests
