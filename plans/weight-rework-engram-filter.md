# Plan: Weight Rework + Engram Filter + Reminder via Engram

## Context

Current weight system is a simple access counter (start 0.5, +0.1 per access, cap 1.0).
A file opened 5 times weighs more than a critical architectural decision captured once.
The reminder picks top 3 threads by weight — shows file changelogs instead of reasoning threads.

User decisions:
- Weight = (importance + confidence) / 10, then +0.1 per access, cap 1.0
- Reminder uses engram (not list_all by weight)
- Engram excludes File/Command threads from injection
- Decay suspends at weight <= 0 (not age-based)
- Engram can propose Suspended/Archived threads if no better Active ones

---

## Phase 1: Add `confidence` to Thread struct

### 1.1 Migration: add `confidence` column
**File:** `src/storage/migrations.rs`
```sql
ALTER TABLE threads ADD COLUMN confidence REAL DEFAULT 0.5;
```

### 1.2 Update Thread struct
**File:** `src/thread.rs`
```rust
pub confidence: f64,  // LLM extraction confidence (0.0-1.0)
```
Default: 0.5 (for existing threads + manually created ones)

### 1.3 Update storage (from_row, insert, update)
**File:** `src/storage/threads.rs`
- Add `confidence` to SELECT columns
- Add to INSERT/UPDATE statements
- Handle in `from_row()`

### 1.4 Wire confidence in processor
**File:** `src/daemon/processor.rs`
- After extraction, set `thread.confidence = extraction.confidence` in process_capture stage 5

### 1.5 Wire confidence in enrichment
**File:** `src/daemon/ipc_server.rs`
- In `enrich_thread` handler: `thread.confidence = extraction.confidence`
- In `handle_mind_coherence_chain`: same

---

## Phase 2: Dynamic weight initialization

### 2.1 New weight formula
**File:** `src/intelligence/thread_manager.rs` — `process_input()`

Replace:
```rust
weight: 0.5,
```
With:
```rust
weight: (extraction.importance + extraction.confidence) / 10.0,
```

Examples:
- importance=0.8, confidence=0.9 → weight=0.17 → after 5 accesses: 0.67
- importance=0.3, confidence=0.4 → weight=0.07 → after 5 accesses: 0.57
- importance=1.0, confidence=1.0 → weight=0.20 → after 8 accesses: 1.0

### 2.2 Update enrichment weight
**File:** `src/daemon/ipc_server.rs`

After enrichment updates importance, recalculate weight:
```rust
thread.weight = ((thread.importance + extraction.confidence) / 10.0)
    .max(thread.weight);  // never decrease weight via enrichment
```

### 2.3 Update handle_thread_create
**File:** `src/mcp/tools/threads.rs`

Initial weight for agent-created threads:
```rust
weight: (importance + 0.5) / 10.0,  // confidence=0.5 default until enriched
```

---

## Phase 3: Engram excludes File/Command

### 3.1 Filter in engram retriever candidates
**File:** `src/intelligence/engram_retriever.rs`

In `search()` and `get_relevant_context()`, filter candidates:
```rust
candidates.retain(|t| !matches!(t.origin_type,
    OriginType::FileRead | OriginType::FileWrite | OriginType::Command));
```

Note: File/Command threads remain in DB, accessible via `ai_recall` with explicit query.
Continuity expansion still pulls them in when a Prompt/Response thread references them.

---

## Phase 4: Reminder via Engram

### 4.1 Replace list_all with engram search
**File:** `src/hook/reminder.rs`

Replace `append_threads_pins_focus()` thread selection:
```rust
// OLD: list_all sorted by weight DESC, take 3
// NEW: engram search with session context
let engram = EngramRetriever::new(conn, EngramConfig::default())?;
let threads = engram.search(conn, &session_context, 3)?;
```

Session context = last prompt or beat state summary.

### 4.2 Add ONNX init to reminder path
**File:** `src/hook/reminder.rs`

Ensure `ensure_ort_dylib_path()` is called before engram (ONNX embeddings).
~200ms overhead — acceptable per user decision.

---

## Phase 5: Decay suspends at weight <= 0

### 5.1 Change decay logic
**File:** `src/daemon/periodic_tasks.rs` (prune cycle)

Current: suspends threads inactive > X days.
New: decay subtracts from weight each cycle. Suspend when weight <= 0.

```rust
// Decay rate: configurable, default 0.01 per prune cycle (~5 min)
thread.weight -= decay_config.weight_decay_rate;
if thread.weight <= 0.0 {
    ThreadStorage::update_status(conn, &thread.id, ThreadStatus::Suspended);
}
```

### 5.2 Add `weight_decay_rate` to config
**File:** `src/config.rs`
```rust
pub weight_decay_rate: f64,  // default: 0.01
```

### 5.3 Remove age-based suspension
**File:** `src/daemon/periodic_tasks.rs`

Remove or disable the age-based `max_thread_age_days` suspension.
Weight is now the sole authority on thread lifecycle.

---

## Phase 6: Engram proposes Suspended/Archived threads

### 6.1 Expand candidate pool
**File:** `src/intelligence/engram_retriever.rs`

Current: candidates = Active threads only.
New: if Active candidates < limit, also search Suspended threads.

```rust
let mut candidates = load_active_candidates(...);
if candidates.len() < limit {
    let suspended = load_suspended_candidates(...);
    candidates.extend(suspended);
}
```

### 6.2 Review validators
**File:** `src/intelligence/engram_retriever.rs`

Check each validator handles Suspended threads correctly:
- StatusValidator: must not penalize Suspended (or remove this validator)
- FreshnessValidator: Suspended threads are stale by nature — adjust scoring
- Others: verify no hard Active-only gates

---

## Files touched (summary)

| File | Phases |
|------|--------|
| `src/storage/migrations.rs` | 1 |
| `src/thread.rs` | 1 |
| `src/storage/threads.rs` | 1 |
| `src/daemon/processor.rs` | 1 |
| `src/daemon/ipc_server.rs` | 1, 2 |
| `src/intelligence/thread_manager.rs` | 2 |
| `src/mcp/tools/threads.rs` | 2 |
| `src/intelligence/engram_retriever.rs` | 3, 6 |
| `src/hook/reminder.rs` | 4 |
| `src/daemon/periodic_tasks.rs` | 5 |
| `src/config.rs` | 5 |

## Execution order

Phase 1 → 2 → 3 → 4 → 5 → 6
Each phase is independently committable. Build + test after each.
