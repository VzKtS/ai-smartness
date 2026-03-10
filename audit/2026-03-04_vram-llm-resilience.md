# Audit: VRAM Management & LLM Resilience
**Date:** 2026-03-04
**Version:** v5.6.5-alpha
**Trigger:** All daemon LLM calls failing with `NullReturn` — Vulkan can't allocate KV cache context
**GPU:** NVIDIA GTX 1650 (4096 MiB VRAM)

---

## Incident

After daemon restart, the local LLM (Phi-4-mini, 2.5 GB) loads successfully but **every `new_context()` call fails with `NullReturn`**. The model occupies 2371 MiB, and other GPU consumers (Xorg, gnome-shell, VSCode, Discord) take ~462 MiB, leaving ~1263 MiB. Despite this appearing sufficient for a 4096-token KV cache (~384 MiB), Vulkan consistently refuses allocation.

**Impact:** Complete memory blackout — all extractions dropped, no new threads enriched, mind threads created without metadata.

---

## Finding 1: No Pre-flight VRAM Check

**Severity: CRITICAL**
**File:** `src/processing/local_llm.rs:111-137`

The daemon blindly attempts GPU allocation without checking available VRAM first:
- Model load: hardcoded 99 GPU layers, fallback to 28 on failure
- Context creation: hardcoded 4096 tokens, no fallback size
- No call to `nvidia-smi` or Vulkan API before allocation

**Note:** `src/daemon/watchdog.rs:58-82` already has `collect_gpu_vram()` that parses `nvidia-smi` output — but it's only used for metrics, not for pre-flight checks.

**Consequence:** If VRAM is tight (fragmentation, other consumers), allocation fails reactively instead of adapting proactively.

---

## Finding 2: Context Size Not Configurable

**Severity: HIGH**
**File:** `src/processing/local_llm.rs:26`

```rust
const DEFAULT_CTX_SIZE: u32 = 4096;  // hardcoded, no config override
```

`GuardianConfig` in `src/config.rs:813-838` has `local_model_size` but NO `ctx_size` field. The user cannot tune context size for their GPU.

KV cache VRAM varies by model:
- Phi-4-mini (GQA): ~384 MiB for 4096 tokens
- Qwen 3B: ~256 MiB for 4096 tokens
- Qwen 7B: ~512 MiB for 4096 tokens

A 2048-token context would halve KV cache VRAM and still work for most extraction tasks (typical prompt+response < 1500 tokens).

---

## Finding 3: GPU Recovery Loop (Not Infinite, But Wasteful)

**Severity: MEDIUM**
**File:** `src/processing/local_llm.rs:165-195, 241-251`

The recovery flow on NullReturn:
1. `generate()` → context creation fails (retry once after 1s) → `Err(Provider)`
2. `is_gpu_error()` detects NullReturn → drops persistent context (`*ctx_guard = None`)
3. Next `generate()` call → tries to create context again → fails again
4. Repeat forever

This is **not an infinite tight loop** (each iteration only happens when a new capture arrives), but it:
- Spams logs with the same error indefinitely
- Wastes 2s per attempt (1s sleep + 1s retry)
- Never recovers because VRAM situation doesn't change
- No circuit breaker — keeps trying forever

---

## Finding 4: Captures Silently Dropped on LLM Failure

**Severity: CRITICAL**
**Files:** `src/processing/extractor.rs`, `src/daemon/processor.rs`

When LLM fails:
- **Extraction:** Returns `Ok(None)` → capture dropped permanently
- **Tool extraction:** Propagates error → capture dropped
- **Mind enrichment:** Returns `Ok(())` → thread exists but without metadata
- **No retry queue, no dead letter, no persistence for later retry**

A persistent LLM failure causes **total memory blackout** — the agent keeps running but forms no new memories.

---

## Finding 5: Backpressure Auto-Clears Too Soon

**Severity: MEDIUM**
**File:** `src/daemon/capture_queue.rs`

Backpressure flag in BeatState auto-clears after 10 minutes. If LLM is stuck for hours (our case), the flag clears, new captures flood in, and get dropped immediately. The hook has no signal that the daemon is degraded.

---

## Finding 6: Coherence Has Fallback, Extraction Does Not

**Severity: INFO**
**File:** `src/processing/coherence.rs`

Coherence gate falls back to embedding similarity when LLM fails. This is the ONLY LLM call with a fallback. Extraction has no heuristic fallback — it's LLM-or-nothing.

---

## Finding 7: Daemon Init Race

**Severity: LOW**
**File:** `src/daemon/mod.rs:138-154`

IPC server starts immediately; LLM init runs in background thread. Workers block on `OnceLock` until init completes. If model loads but context never succeeds, the daemon runs forever in a degraded state with no notification.

---

## Root Cause Analysis

The immediate cause is **Vulkan VRAM fragmentation after daemon restart**:

| Timeline | State |
|---|---|
| March 3 21:49 | Daemon starts early, model + context allocated. VRAM: model 2371 + KV ~384 + Xorg ~200 ≈ 2955 MiB. Works fine. |
| March 3 → March 4 | Discord opens (+102 MiB), VSCode windows open (+91 MiB). VRAM fills up. |
| March 4 04:45 | Daemon restarts. Model loads (2371 MiB reused). Context creation: Vulkan can't find contiguous block for KV cache. NullReturn. |

The underlying cause is that the daemon has **no VRAM awareness** — it assumes 4 GB is always available and never checks reality.

---

## Recommendations

### R1: Adaptive Context Size (Priority: CRITICAL)

Add a VRAM-aware context size selection:

```
fn select_ctx_size(model_vram_mb: u64) -> u32 {
    let (used, total) = collect_gpu_vram();     // Already exists in watchdog
    let free = total - used + model_vram_mb;    // Model VRAM will be shared
    let kv_per_token_kb = 96;                   // Approximate

    for ctx in [4096, 2048, 1024, 512] {
        let kv_mb = (ctx * kv_per_token_kb) / 1024;
        if free > model_vram_mb + kv_mb + 200 { // 200 MB safety margin
            return ctx;
        }
    }
    512 // minimum viable
}
```

### R2: Make ctx_size Configurable

Add to `GuardianConfig`:
```toml
[local_llm]
ctx_size = 4096       # 0 = auto (VRAM-adaptive)
max_tokens = 768
gpu_layers = 99       # 0 = auto
```

### R3: Circuit Breaker

After N consecutive LLM failures (e.g., 5), mark LLM unavailable for a cooldown period (e.g., 5 min). Log a single WARN instead of spamming. Expose status via IPC/beat.

```
State machine:
  Available → (5 consecutive failures) → Cooldown(5min) → Available → ...
```

### R4: Dead Letter Queue for Failed Captures

Instead of dropping captures on LLM failure, persist them to `.failed` files:
- On LLM recovery, replay `.failed` files
- Configurable max age (e.g., 24h) before permanent discard
- Prevents memory blackout during LLM outages

### R5: Extend Backpressure Semantics

- Distinguish "LLM unavailable" from "LLM busy"
- Hook should stop sending captures when LLM is down (not just during transient backpressure)
- Add `llm_status` field to BeatState: `available | cooldown | unavailable`

### R6: Startup VRAM Pre-flight

Before model load, check available VRAM and adjust gpu_layers accordingly:
```
available = total_vram - used_by_others
if available < model_size + min_kv_cache:
    reduce gpu_layers or warn and abort
```

---

## Priority Matrix

| # | Fix | Effort | Impact | Priority |
|---|-----|--------|--------|----------|
| R1 | Adaptive ctx_size | Medium | Fixes current incident | P0 |
| R2 | Configurable ctx_size | Low | User control | P0 |
| R3 | Circuit breaker | Medium | Stops log spam, enables recovery | P1 |
| R4 | Dead letter queue | High | Prevents memory blackout | P1 |
| R5 | Backpressure semantics | Medium | Better hook behavior | P2 |
| R6 | Startup VRAM pre-flight | Medium | Proactive failure avoidance | P2 |

---

## Immediate Mitigation

To unblock the current daemon:
1. Close Discord or any non-essential GPU consumer
2. Restart daemon (model reloads, context may succeed with freed VRAM)
3. Or: reduce `DEFAULT_CTX_SIZE` to 2048 temporarily and rebuild

Long-term: implement R1+R2 (adaptive + configurable ctx_size).
