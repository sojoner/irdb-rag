# Query Performance Optimization - Deployment Checklist

## Overview
This deployment includes critical performance optimizations that fix a severe query planning issue and add missing database indexes.

**Impact**: ~99.99% improvement in BM25 search planning time (46s → 3ms)

---

## Pre-Deployment

- [ ] Review `PERFORMANCE_OPTIMIZATION_SUMMARY.md` for details
- [ ] Back up current database (if not doing clean deploy)
- [ ] Test in staging environment first if possible

---

## Deployment Steps

### Option A: Clean Deploy (Recommended)
```bash
# 1. Stop current stack
docker-compose down

# 2. Remove old data (optional, for clean slate)
docker volume rm irdb-rag_postgres_data

# 3. Update from latest main branch
git pull origin main

# 4. Start fresh with optimized schema
docker-compose -f docker-compose-gpu.yml up -d

# 5. Wait for database initialization (2-5 minutes)
docker logs -f rag-db | grep "ready to accept"
```

### Option B: Apply to Existing Database
```bash
# 1. No downtime needed, indexes created in background

# 2. Verify docker-compose files have ParadeDB config
grep "paradedb.enable_aggregate" docker-compose-gpu.yml
grep "paradedb.enable_aggregate" docker-compose.yml

# 3. Connect to database and verify settings applied
docker exec rag-db psql -U rag_user -d rag_chat -c "
  SHOW paradedb.enable_aggregate_custom_scan;
  SHOW paradedb.per_tuple_cost;
"

# Expected output: "on" and "100"

# 4. If settings not applied, run:
docker exec rag-db psql -U rag_user -d rag_chat << 'EOF'
ALTER SYSTEM SET paradedb.enable_aggregate_custom_scan = on;
ALTER SYSTEM SET paradedb.enable_custom_scan_without_operator = on;
ALTER SYSTEM SET paradedb.per_tuple_cost = 100;
ALTER SYSTEM SET paradedb.limit_fetch_multiplier = 2;
SELECT pg_reload_conf();
