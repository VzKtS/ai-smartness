# Arborescence Refactor Plan — Standard Rust Multi-Module

**Auditor**: arc (architect)
**Date**: 2026-02-24
**Codebase**: ai-smartness v1.1.0, 107 .rs files, 28,768 LOC
**Scope**: Per cor's directive — reorganize src/ for scalability to 50K+ LOC

---

## 1. Current State Inventory

### 1.1 Root-level files (20 files, 3,580 lines)

These are the most problematic — 20 files at src/ root with mixed concerns:

| File | Lines | Role | Problem |
|------|------:|------|---------|
| `lib.rs` | 62 | Library root | Contains inline `HealthStatus`/`HealthLevel` types |
| `main.rs` | 356 | Binary entry point | Fine — stays |
| `agent.rs` | 336 | Core type: Agent, AgentStatus, etc. | Should be in `core/` |
| `bridge.rs` | 100 | Core type: ThinkBridge | Should be in `core/` |
| `config.rs` | 1,768 | **MEGA CONFIG** — all subsystem configs | Must split into `config/` |
| `config_sync.rs` | 69 | Config propagation utility | Merge into `config/` |
| `constants.rs` | 177 | System constants | Should be in `core/` |
| `error.rs` | 80 | AiError, AiResult | Should be in `core/` |
| `hook_setup.rs` | 312 | Claude Code hook installer | Should be in `hook/` |
| `id_gen.rs` | 58 | ID generation utilities | Should be in `core/` |
| `message.rs` | 103 | Message types | Should be in `core/` |
| `project_registry.rs` | 43 | ProjectRegistryTrait | Should be in `registry/` |
| `provider.rs` | 37 | AiProvider trait (dead code) | Should be in `adapters/` |
| `session.rs` | 156 | SessionState | Should be in `core/` |
| `shared.rs` | 29 | SharedThread types | Should be in `core/` |
| `test_helpers.rs` | 297 | Test builders | Should be in `testing/` |
| `thread.rs` | 225 | Core type: Thread | Should be in `core/` |
| `time_utils.rs` | 29 | Time helpers | Should be in `core/` |
| `tracing_init.rs` | 73 | Logging setup | Fine — infrastructure |
| `user_profile.rs` | 202 | UserProfile type | Should be in `core/` |

### 1.2 Existing subdirectories (13 dirs, 25,188 lines)

| Directory | Files | Lines | Quality | Notes |
|-----------|------:|------:|---------|-------|
| `storage/` | 18 | 4,476 | Good | Well-organized, could split threads.rs |
| `mcp/` | 16 | 4,089 | Good | Well-structured with tools/ subdir |
| `intelligence/` | 12 | 3,186 | Good | Has validators/ subdir |
| `daemon/` | 7 | 2,466 | Good | Clean structure |
| `hook/` | 10 | 2,301 | Good | Has providers/ subdir |
| `gui/` | 2+5 | 1,720 | Needs split | commands.rs is 1,650 lines |
| `registry/` | 5 | 1,444 | Good | Clean structure |
| `cli/` | 11 | 1,441 | Good | One file per subcommand |
| `processing/` | 7 | 1,004 | Mixed | daemon_ipc_client misplaced here |
| `healthguard/` | 4 | 797 | Good | Clean checks system |
| `guardcode/` | 4 | 121 | Small | May merge with hook/ or adapters/ |
| `admin/` | 3 | 107 | **DEAD CODE** | All `todo!()` stubs |
| `network/` | 4 | 36 | **STUB** | Placeholder for P2P |

### 1.3 Critical Issues

1. **20 files at root** — Core types (agent, thread, bridge, message, shared) scattered at src/ root instead of in a `core/` module
2. **config.rs is 1,768 lines** — All subsystem configs in one mega-file
3. **gui/commands.rs is 1,650 lines** — All Tauri commands in one file
4. **hook/inject.rs is 1,160 lines** — 8+ injection layers in one function
5. **No `core/` module** — Foundation types have no home
6. **No `adapters/` module** — Provider abstraction has no home (per meta-layer vision)
7. **Dead modules** — `admin/` (todo stubs), `network/` (placeholder)
8. **Duplicate code** — `hook_setup.rs` at root + `hook/setup.rs` (2-line re-export)
9. **Everything is `pub mod`** in lib.rs — No privacy boundaries
10. **Binary/library boundary blur** — `hook/`, `cli/`, `daemon/`, `gui/`, `mcp/` are binary-only but some types leak into lib

---

## 2. Target Arborescence

```
src/
├── main.rs                          # Binary entry point (clap CLI)
├── lib.rs                           # Library root (public API)
├── tracing_init.rs                  # Logging infrastructure (stays)
│
├── core/                            # Foundation types (Tier 1 imports)
│   ├── mod.rs                       # Re-exports all core types
│   ├── error.rs                     # AiError, AiResult
│   ├── types/
│   │   ├── mod.rs
│   │   ├── agent.rs                 # Agent, AgentStatus, CoordinationMode, ThreadMode
│   │   ├── bridge.rs                # ThinkBridge, BridgeType, BridgeStatus
│   │   ├── thread.rs                # Thread, ThreadStatus, ThreadMessage, InjectionStats
│   │   ├── message.rs               # Message, MessagePriority, Attachment
│   │   ├── shared.rs                # SharedThread, SharedVisibility, Subscription
│   │   ├── session.rs               # SessionState
│   │   ├── health.rs                # HealthStatus, HealthLevel (from lib.rs)
│   │   └── user_profile.rs          # UserProfile
│   ├── constants.rs                 # System-wide constants
│   ├── id_gen.rs                    # ID generation (SHA-256, UUID)
│   └── time_utils.rs               # Chrono helpers
│
├── config/                          # Configuration system
│   ├── mod.rs                       # GuardianConfig, load/save, validate()
│   ├── decay.rs                     # DecayConfig
│   ├── gossip.rs                    # GossipConfig
│   ├── extraction.rs                # ExtractionConfig, TaskLlmConfig, ClaudeModel
│   ├── engram.rs                    # EngramConfig
│   ├── embedding.rs                 # EmbeddingConfig, EmbeddingMode
│   ├── hooks.rs                     # HooksConfig
│   ├── daemon.rs                    # DaemonConfig (from daemon_config.json)
│   ├── backup.rs                    # BackupConfig
│   ├── thread_matching.rs           # ThreadMatchingConfig
│   ├── scoring.rs                   # ImportanceScoreMap, AlertThresholds, GuardianAlert(s), LlmHealthState, FallbackPatterns (~110L)
│   └── sync.rs                      # Config propagation (from config_sync.rs)
│
├── storage/                         # SQLite persistence (stays, minor splits)
│   ├── mod.rs
│   ├── database.rs                  # Connection factory
│   ├── migrations.rs                # Schema V1-V6
│   ├── path_utils.rs                # Cross-platform paths
│   ├── manager.rs                   # StorageManager
│   ├── threads/                     # Split from threads.rs (968 lines)
│   │   ├── mod.rs                   # Re-exports
│   │   ├── crud.rs                  # Insert, get, update, delete
│   │   ├── search.rs                # search, search_by_labels, search_by_topics
│   │   └── bulk.rs                  # Batch operations, purge
│   ├── bridges.rs                   # Bridge CRUD (488 lines — OK)
│   ├── cognitive_inbox.rs           # Cognitive inbox storage
│   ├── concept_index.rs             # Concept inverted index
│   ├── topic_index.rs               # Topic inverted index
│   ├── mcp_messages.rs              # MCP broker messages
│   ├── shared_storage.rs            # Shared thread storage
│   ├── beat.rs                      # Beat system (temporal perception)
│   ├── backup.rs                    # Backup/restore manager
│   ├── transcript.rs                # JSONL transcript parser
│   ├── credentials.rs               # Credential reader
│   ├── quota_probe.rs               # API quota probing
│   └── project_registry_impl.rs     # ProjectRegistryTrait impl
│
├── intelligence/                    # AI-powered intelligence (stays)
│   ├── mod.rs
│   ├── thread_manager.rs
│   ├── engram/                      # Split from engram_retriever.rs (771 lines)
│   │   ├── mod.rs                   # Pipeline orchestrator
│   │   ├── scoring.rs               # Score computation
│   │   └── validators.rs            # 9 validators (from validators/mod.rs)
│   ├── gossip.rs
│   ├── decayer.rs                   # Includes archiver logic (merge archiver.rs)
│   ├── merge_evaluator.rs
│   ├── merge_metadata.rs
│   ├── reactivation_decider.rs
│   └── synthesis.rs
│
├── processing/                      # Data processing pipeline (stays)
│   ├── mod.rs
│   ├── cleaner.rs
│   ├── coherence.rs
│   ├── embeddings.rs
│   ├── extractor.rs
│   └── llm_subprocess.rs           # → becomes adapters/llm.rs in V2+
│
├── registry/                        # Agent registry (absorbs project_registry)
│   ├── mod.rs
│   ├── registry.rs                  # Agent CRUD (consider splitting at 907 lines)
│   ├── discovery.rs
│   ├── heartbeat.rs
│   ├── tasks.rs
│   └── project.rs                   # ProjectRegistryTrait + impl (merge from root + storage)
│
├── adapters/                        # NEW — Provider abstraction layer
│   ├── mod.rs                       # Trait re-exports, detect_provider()
│   ├── provider.rs                  # AiProvider trait, HookMechanism (from root provider.rs)
│   ├── claude/                      # Claude Code adapter
│   │   ├── mod.rs
│   │   ├── hooks.rs                 # Hook setup for .claude/ dir (from hook_setup.rs)
│   │   ├── format.rs                # Injection/capture format (from hook/providers/claude.rs)
│   │   └── credentials.rs           # Claude credentials (future: move from storage/)
│   ├── generic/                     # Generic/fallback adapter
│   │   ├── mod.rs
│   │   └── format.rs               # Generic format (from hook/providers/generic.rs)
│   └── _template/                   # Template for new provider adapters
│       └── mod.rs                   # Empty scaffold with trait impls
│
├── hook/                            # Hook handlers (stays, restructured)
│   ├── mod.rs                       # Hook dispatcher + anti-loop guard
│   ├── capture.rs                   # PostToolUse handler
│   ├── compact.rs                   # Context compaction
│   ├── health.rs                    # Health check hook
│   ├── inject/                      # Split from inject.rs (1,160 lines)
│   │   ├── mod.rs                   # Main inject orchestrator
│   │   ├── context.rs               # Context layer (session, beat, agent identity)
│   │   ├── memory.rs                # Memory injection (engram recall)
│   │   ├── inbox.rs                 # Cognitive inbox + pins
│   │   └── nudges.rs                # Proactive nudges + health findings
│   ├── pretool.rs
│   └── virtual_paths.rs
│
├── healthguard/                     # Proactive health monitoring (stays)
│   ├── mod.rs
│   ├── checks.rs
│   ├── formatter.rs
│   └── merge_detector.rs
│
├── daemon/                          # Background daemon (stays)
│   ├── mod.rs
│   ├── capture_queue.rs
│   ├── connection_pool.rs
│   ├── controller.rs                # Consider splitting at 781 lines
│   ├── ipc_server.rs
│   ├── periodic_tasks.rs
│   └── processor.rs
│
├── mcp/                             # MCP server (stays, minor adjustments)
│   ├── mod.rs
│   ├── jsonrpc.rs
│   ├── server.rs
│   └── tools/
│       ├── mod.rs
│       ├── agents.rs
│       ├── bridges.rs
│       ├── discover.rs
│       ├── focus.rs
│       ├── merge.rs
│       ├── messaging.rs
│       ├── recall.rs
│       ├── share.rs
│       ├── split.rs
│       ├── status.rs
│       ├── threads.rs
│       └── windows.rs
│
├── cli/                             # CLI handlers (stays)
│   ├── mod.rs
│   ├── agent.rs
│   ├── bridges.rs
│   ├── config.rs
│   ├── daemon.rs
│   ├── init.rs
│   ├── project.rs
│   ├── search.rs
│   ├── setup_onnx.rs
│   ├── status.rs
│   └── threads.rs
│
├── gui/                             # Tauri GUI (stays, split commands.rs)
│   ├── mod.rs
│   ├── commands/                    # Split from commands.rs (1,650 lines)
│   │   ├── mod.rs                   # Re-exports all handlers
│   │   ├── dashboard.rs             # Dashboard, overview, resources
│   │   ├── threads.rs               # Thread CRUD, search, detail
│   │   ├── agents.rs                # Agent CRUD, hierarchy
│   │   ├── projects.rs              # Project management
│   │   ├── settings.rs              # Config, daemon settings, backup settings
│   │   ├── maintenance.rs           # Backup, reindex, debug logs
│   │   └── daemon.rs                # Daemon start/stop/status
│   └── frontend/                    # HTML/JS/CSS (stays)
│
├── network/                         # P2P networking (stays as placeholder)
│   ├── mod.rs
│   ├── discovery.rs
│   ├── peer.rs
│   └── protocol.rs
│
└── testing/                         # Test infrastructure
    ├── mod.rs
    └── helpers.rs                   # ThreadBuilder, BridgeBuilder, DB setup (from test_helpers.rs)
```

### 2.1 Deleted Modules

| Module | Reason |
|--------|--------|
| `admin/` (107 lines) | Dead code — all `todo!()` stubs, superseded by `gui/commands.rs` |
| `guardcode/` (121 lines) | Content safety rules → merge into `adapters/` (enforcer pattern) or `hook/` depending on usage |
| `hook/setup.rs` (2 lines) | 2-line re-export — delete, code now lives in `adapters/claude/hooks.rs` |
| `hook/providers/` (99 lines) | Moved to `adapters/claude/format.rs` and `adapters/generic/format.rs` |
| `intelligence/memory_retriever.rs` (20 lines) | Thin wrapper — merge into `engram/mod.rs` |
| `intelligence/archiver.rs` (40 lines) | Simple logic — merge into `decayer.rs` |

### 2.2 New Modules

| Module | Purpose | Per cor's directive |
|--------|---------|-------------------|
| `core/` | Foundation types used everywhere | Standard Rust pattern for large projects |
| `core/types/` | Domain types (agent, thread, bridge, message) | Eliminates root-level type scatter |
| `config/` | Split mega-config into per-subsystem files | config.rs is 1,768 lines |
| `adapters/` | Provider abstraction layer | Meta-layer vision: pluggable providers |
| `adapters/claude/` | Claude Code-specific code | Isolates all Claude coupling |
| `adapters/generic/` | Generic fallback adapter | MCP-only providers |
| `adapters/_template/` | New provider scaffold | Onboarding pattern |
| `testing/` | Test infrastructure | Separates test-only code |

---

## 3. Migration Plan

### Phase 0: Preparation (0 risk, 0 LOC functional changes)

1. **Create target directories**: `core/`, `core/types/`, `config/`, `adapters/`, `adapters/claude/`, `adapters/generic/`, `testing/`, `hook/inject/`, `storage/threads/`, `intelligence/engram/`, `gui/commands/`
2. **Delete dead code**: Remove `admin/` module entirely (107 lines of `todo!()`)
3. **Run `cargo test`** — baseline green

### Phase 1: Core types extraction (LOW RISK)

Move root-level type files into `core/types/`:

| Move | From | To |
|------|------|----|
| 1 | `src/error.rs` | `src/core/error.rs` |
| 2 | `src/agent.rs` | `src/core/types/agent.rs` |
| 3 | `src/thread.rs` | `src/core/types/thread.rs` |
| 4 | `src/bridge.rs` | `src/core/types/bridge.rs` |
| 5 | `src/message.rs` | `src/core/types/message.rs` |
| 6 | `src/shared.rs` | `src/core/types/shared.rs` |
| 7 | `src/session.rs` | `src/core/types/session.rs` |
| 8 | `src/user_profile.rs` | `src/core/types/user_profile.rs` |
| 9 | `src/constants.rs` | `src/core/constants.rs` |
| 10 | `src/id_gen.rs` | `src/core/id_gen.rs` |
| 11 | `src/time_utils.rs` | `src/core/time_utils.rs` |
| 12 | Move `HealthStatus`/`HealthLevel` from `lib.rs` | `src/core/types/health.rs` |

**Create** `src/core/mod.rs` and `src/core/types/mod.rs` with re-exports.

**Update** `lib.rs`: Replace 12 individual `pub mod X` with `pub mod core` + re-exports for backward compatibility:
```rust
pub mod core;
// Backward-compatible re-exports
pub use core::error::{AiError, AiResult};
pub use core::types::agent;
pub use core::types::thread;
// ... etc
```

**Impact**: ~60 import changes across codebase (`use crate::thread` → `use crate::core::types::thread` or via re-export). Can use `pub use` re-exports in lib.rs to minimize churn.

**Verification**: `cargo test && cargo clippy`

### Phase 2: Config split (MEDIUM RISK)

Split `config.rs` (1,768 lines) into `config/` module:

| New file | Content from config.rs | Approx lines |
|----------|----------------------|-------------|
| `config/mod.rs` | `GuardianConfig`, `load()`, `save()`, `validate()` | ~300 |
| `config/decay.rs` | `DecayConfig` | ~80 |
| `config/gossip.rs` | `GossipConfig` | ~100 |
| `config/extraction.rs` | `ExtractionConfig`, `TaskLlmConfig`, `ClaudeModel` | ~150 |
| `config/engram.rs` | `EngramConfig` | ~80 |
| `config/embedding.rs` | `EmbeddingConfig`, `EmbeddingMode` | ~60 |
| `config/hooks.rs` | `HooksConfig` | ~40 |
| `config/daemon.rs` | `DaemonConfig` | ~80 |
| `config/backup.rs` | `BackupConfig` | ~60 |
| `config/thread_matching.rs` | `ThreadMatchingConfig` | ~50 |
| `config/scoring.rs` | `ImportanceScoreMap`, `AlertThresholds`, `GuardianAlert(s)`, `LlmHealthState`, `FallbackPatterns` | ~110 |
| `config/sync.rs` | `config_sync.rs` content | ~70 |

**Strategy**: `config/mod.rs` imports all sub-configs (12 modules total) and composes `GuardianConfig`. Sub-configs are `pub` types. Serde `#[serde(flatten)]` or explicit fields in `GuardianConfig`.

> **Sub correction**: 7th module `scoring.rs` needed for orphan structs that don't fit in the original 6 modules. 27 config structs → 12 files.

**Impact**: ~30 import changes (`use crate::config::` mostly stays the same if GuardianConfig remains in `config/mod.rs`).

**Verification**: `cargo test` — JSON serialization roundtrip must be identical (add a test).

### Phase 3: Adapters module (LOW RISK)

Create `adapters/` from existing scattered provider code. Also migrate `llm_subprocess.rs` from `processing/` to `adapters/llm/claude.rs` alongside the `LlmProvider` trait definition.

> **Sub correction**: `llm_subprocess.rs` stays in `processing/` during Phases 1-2 (no intermediate move). Phase 3 is the right time to migrate it because the `LlmProvider` trait is defined simultaneously — no orphaned code.

| Move | From | To |
|------|------|----|
| 1 | `src/provider.rs` | `src/adapters/provider.rs` (trait definitions) |
| 2 | `src/hook/providers/claude.rs` | `src/adapters/claude/format.rs` |
| 3 | `src/hook/providers/generic.rs` | `src/adapters/generic/format.rs` |
| 4 | `src/hook/providers/mod.rs` | `src/adapters/mod.rs` (merge detect_provider) |
| 5 | `src/hook_setup.rs` | `src/adapters/claude/hooks.rs` |
| 6 | `src/guardcode/injector.rs` | `src/adapters/mod.rs` (merge 12 lines) |
| 7 | `src/processing/llm_subprocess.rs` | `src/adapters/llm/claude.rs` (with LlmProvider trait) |

**Atomic dead code deletion** (sub-verified scope):

| File to delete | Reason |
|---------------|--------|
| `src/provider.rs` | Dead — trait + InjectionPayload never used at runtime |
| `src/guardcode/injector.rs` | Dead — `Injector::inject` never called |
| `src/hook/providers/claude.rs` | Dead — ClaudeProvider never instantiated |
| `src/hook/providers/generic.rs` | Dead — GenericProvider never instantiated |
| `src/hook/providers/mod.rs` | Dead — detect_provider() never called outside module |

**References to clean simultaneously**:
- `src/guardcode/mod.rs`: remove `pub mod injector;`
- `src/lib.rs`: remove `pub mod provider;`

> **Sub note**: These 5 files + 2 ref lines MUST be deleted atomically (single commit) — partial deletion causes compilation errors due to cross-references within the dead code cluster.

**Keep**: `src/guardcode/enforcer.rs` and `src/guardcode/rules.rs` — generic content safety, not provider-specific.

**Note**: `EmbeddingManager` (also in `processing/`) stays in `processing/embeddings.rs` — it's local inference logic, not an adapter.

**Impact**: ~10 import changes. Most of this code is dead, so import breakage is minimal.

**Verification**: `cargo test && cargo clippy`

### Phase 4: Large file splits (MEDIUM RISK)

Split files over 700 lines:

#### 4a. `gui/commands.rs` (1,650 lines) → `gui/commands/`

| New file | Functions moved | Approx lines |
|----------|----------------|-------------|
| `gui/commands/mod.rs` | Re-exports + shared helpers | ~50 |
| `gui/commands/dashboard.rs` | get_dashboard, get_project_overview, get_system_resources, check_update | ~250 |
| `gui/commands/threads.rs` | get_threads, search_*, get_thread_detail, delete_thread, list_all_* | ~350 |
| `gui/commands/agents.rs` | list_agents, add_agent, update_agent, remove_agent, get_hierarchy, purge_agent_db | ~350 |
| `gui/commands/projects.rs` | list_projects, add_project, update_project, remove_project | ~200 |
| `gui/commands/settings.rs` | get/save_settings, get/save_daemon_settings, get/save_backup_settings, sync_* | ~200 |
| `gui/commands/maintenance.rs` | trigger_backup, list_backups, restore_backup, delete_backup, reindex_agent, get_*_debug_logs | ~200 |
| `gui/commands/daemon.rs` | daemon_start, daemon_stop, daemon_status | ~50 |

#### 4b. `hook/inject.rs` (1,160 lines) → `hook/inject/`

| New file | Injection layers | Approx lines |
|----------|-----------------|-------------|
| `hook/inject/mod.rs` | Main `run()` orchestrator, layer composition | ~200 |
| `hook/inject/context.rs` | Session context, beat state, agent identity | ~250 |
| `hook/inject/memory.rs` | Engram recall, thread injection | ~300 |
| `hook/inject/inbox.rs` | Cognitive inbox, pins, focus topics | ~200 |
| `hook/inject/nudges.rs` | Proactive nudges, health findings, suggestions | ~210 |

#### 4c. `storage/threads.rs` (968 lines) → `storage/threads/`

| New file | Functions | Approx lines |
|----------|----------|-------------|
| `storage/threads/mod.rs` | Re-exports, `thread_from_row()` | ~80 |
| `storage/threads/crud.rs` | insert, get, update, delete, add_message | ~350 |
| `storage/threads/search.rs` | search, search_by_labels, search_by_topics, list_by_status | ~300 |
| `storage/threads/bulk.rs` | delete_batch, update_status_batch, delete_by_status, purge | ~240 |

#### 4d. `intelligence/engram_retriever.rs` (771 lines) → `intelligence/engram/`

| New file | Content | Approx lines |
|----------|---------|-------------|
| `intelligence/engram/mod.rs` | Pipeline orchestrator, EngramRetriever::recall() | ~250 |
| `intelligence/engram/scoring.rs` | Score computation, candidate ranking | ~250 |
| `intelligence/engram/validators.rs` | 9 validators (from validators/mod.rs, 258 lines) | ~270 |

**Impact**: Internal restructuring only. No public API changes.

**Verification**: `cargo test` after each split.

### Phase 5: Minor cleanups (LOW RISK)

| Action | Details |
|--------|---------|
| Merge `intelligence/archiver.rs` (40 lines) into `intelligence/decayer.rs` | Same lifecycle concern |
| Merge `intelligence/memory_retriever.rs` (20 lines) into `intelligence/engram/mod.rs` | Thin wrapper |
| Merge `intelligence/synthesis.rs` (36 lines) into `intelligence/thread_manager.rs` | Tightly coupled |
| Move `processing/daemon_ipc_client.rs` to `daemon/ipc_client.rs` | Misplaced — IPC client belongs with daemon |
| Move `test_helpers.rs` to `testing/helpers.rs` | Separate test infrastructure |
| Move `project_registry.rs` trait to `registry/project.rs` | Registry concern |
| Merge `storage/project_registry_impl.rs` into `registry/project.rs` | Trait + impl together |

**Verification**: `cargo test` after each move.

### Phase 6: lib.rs cleanup (LOW RISK)

Update `lib.rs` to reflect new structure:

```rust
// Foundation
pub mod core;
pub mod config;
pub mod tracing_init;

// Persistence
pub mod storage;

// Intelligence
pub mod intelligence;
pub mod processing;
pub mod healthguard;

// Multi-agent
pub mod registry;

// Provider abstraction
pub mod adapters;

// Infrastructure
pub mod hook_setup; // backward-compat re-export from adapters::claude::hooks

// Placeholder
pub mod network;

// Testing
#[cfg(test)]
pub mod testing;

// Re-exports for backward compatibility
pub use core::error::{AiError, AiResult};
pub use core::types::health::{HealthStatus, HealthLevel};
```

Binary-only modules stay in `main.rs`:
```rust
mod cli;
mod daemon;
mod gui;
mod hook;
mod mcp;
```

---

## 4. Migration Order and Dependencies

```
Phase 0 (prep + delete dead code)
    │
    ▼
Phase 1 (core/ extraction)     ← FOUNDATION — do first
    │
    ▼
Phase 2 (config/ split)        ← lib.rs ownership: sequential after Phase 1
    │
    ▼
Phase 3 (adapters/)            ← Requires core/ types from Phase 1
    │
    ▼
Phase 4 (file splits)          ← After 1-3 stabilize
    │
    ▼
Phase 5 (minor cleanups)       ← Low-risk merges
    │
    ▼
Phase 6 (lib.rs cleanup)       ← Final
```

> **Sub correction**: Phases 1→2→3 MUST be sequential (not 2+3 parallel). Reason: both Phase 1 and Phase 2 modify `lib.rs` (module re-exports) — parallel execution causes merge conflicts. Phase 3 depends on Phase 1 (adapters/ references core/ types).

**Total estimated impact:**
- Files moved: ~25
- Files split: 5 large files → ~20 smaller files
- Files deleted: ~8 (dead code, re-exports, merged)
- Import changes: ~120 across codebase
- New files created: ~30 (mod.rs files, split results)
- LOC changed: ~0 functional (all moves/renames)
- Risk: LOW-MEDIUM (pure refactoring, no logic changes)

---

## 5. Conventions

### 5.1 File Organization

| Rule | Convention |
|------|-----------|
| **Max file size** | 500 lines soft limit, 800 lines hard limit. Split above 800. |
| **Module structure** | Directories for modules with 3+ files. Single file for 1-2 file modules. |
| **mod.rs content** | Only re-exports and shared types. No business logic in mod.rs (except orchestrators). |
| **One concern per file** | A file should have one primary type or one coherent set of functions. |
| **Test location** | `#[cfg(test)] mod tests` at bottom of each file for unit tests. Integration tests in `tests/`. |

### 5.2 Naming

| Rule | Convention | Example |
|------|-----------|---------|
| **Modules** | snake_case, singular noun or verb_noun | `thread_manager`, `concept_index` |
| **Types** | PascalCase, descriptive | `ThreadManager`, `BridgeStatus` |
| **Traits** | PascalCase, adjective or noun | `AiProvider`, `ProjectRegistryTrait` → `ProjectRegistry` (drop `Trait` suffix) |
| **Constants** | SCREAMING_SNAKE_CASE | `GOSSIP_OVERLAP_WEIGHT` |
| **Config structs** | PascalCase + `Config` suffix | `DecayConfig`, `GossipConfig` |
| **Error variants** | PascalCase, domain-prefixed | `ThreadNotFound`, `BridgeNotFound` |

### 5.3 Module Hierarchy

| Rule | Convention |
|------|-----------|
| **core/** | Only pure types and utilities. No I/O, no DB, no network. |
| **config/** | Configuration types and load/save. No business logic. |
| **storage/** | SQLite persistence only. No intelligence, no hooks. |
| **intelligence/** | AI-powered logic. Depends on storage/ and core/. |
| **processing/** | Data pipeline (extraction, embeddings, LLM calls). |
| **registry/** | Agent/project registry. Depends on storage/. |
| **adapters/** | Provider abstraction. Trait definitions + per-provider implementations. |
| **hook/** | Hook handlers (binary-only). Can use any module. |
| **mcp/** | MCP server (binary-only). Can use any module. |
| **cli/** | CLI handlers (binary-only). Can use any module. |
| **daemon/** | Daemon process (binary-only). Can use any module. |
| **gui/** | Tauri GUI (binary-only, feature-gated). Can use any module. |

### 5.4 Import Rules

| Rule | Convention |
|------|-----------|
| **Library imports** | `use crate::core::types::thread::Thread` or via re-export `use crate::core::Thread` |
| **Binary imports** | `use ai_smartness::core::Thread` (library crate name) |
| **No `super::` for cross-module** | Use absolute `crate::` paths. `super::` only within a module's files. |
| **Group imports** | `std` → `external crates` → `crate::` with blank lines between groups |

### 5.5 New Module Checklist

When adding a new module:
1. Create `module_name/mod.rs` with `//! Module-level doc comment`
2. Add `pub mod module_name;` to parent module
3. Keep files under 500 lines
4. Add `#[cfg(test)] mod tests` if non-trivial logic
5. If provider-specific: put in `adapters/provider_name/`

---

## 6. Adapters Architecture (Meta-Layer Vision)

Per cor's directive — ai-smartness as universal meta-layer.

### 6.1 Adapter Trait Hierarchy

```rust
// src/adapters/provider.rs

/// Core provider trait — every integration must implement this.
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn hook_mechanism(&self) -> HookMechanism;
}

/// Providers that support hook-based injection (Claude Code, Cursor, etc.)
pub trait HookProvider: AiProvider {
    fn format_injection(&self, payload: &InjectionPayload) -> String;
    fn parse_capture(&self, raw: &str) -> Option<CapturedOutput>;
    fn install_hooks(&self, project_path: &Path, project_hash: &str) -> AiResult<()>;
}

/// Providers that support LLM calls (for Guardian intelligence tasks)
pub trait LlmProvider: AiProvider {
    fn call(&self, prompt: &str, model_hint: ModelHint) -> AiResult<String>;
    fn is_available(&self) -> bool;
}

/// Providers that expose billing/quota information
pub trait BillingProvider: AiProvider {
    fn detect_plan(&self) -> Option<PlanInfo>;
    fn probe_quota(&self) -> Option<QuotaSnapshot>;
}

pub enum ModelHint { Fast, Balanced, Powerful }
pub enum HookMechanism { NativeHooks, McpOnly, CustomStdio, None }
```

### 6.2 Target Directory Structure

```
src/adapters/
├── mod.rs              # detect_provider(), registry of adapters
├── provider.rs         # Trait definitions (above)
├── claude/
│   ├── mod.rs          # ClaudeProvider impl
│   ├── hooks.rs        # .claude/settings.json installation
│   ├── format.rs       # <system-reminder> injection format
│   ├── credentials.rs  # ~/.claude/.credentials.json reader
│   └── billing.rs      # Anthropic API quota probe
├── generic/
│   ├── mod.rs          # GenericProvider impl (MCP-only)
│   └── format.rs       # Plain text injection format
├── openai/             # Future — placeholder
│   └── mod.rs
├── ollama/             # Future — placeholder
│   └── mod.rs
└── _template/
    └── mod.rs          # Empty scaffold for new adapters
```

### 6.3 Skills and Integrations (Future)

Per cor's meta-layer vision, space for:
- `src/skills/` — Import and manage Claude Code slash commands
- `src/integrations/` — Wrappers for external tools and MCP servers

These are empty placeholders for now, to be filled as the meta-layer develops.

---

## 7. Risk Assessment

| Phase | Risk | Mitigation |
|-------|------|-----------|
| Phase 0 (delete dead code) | NONE | Dead code confirmed by audit |
| Phase 1 (core/ types) | LOW | Pure move + re-exports. `cargo test` validates. |
| Phase 2 (config split) | MEDIUM | Serde serialization must remain identical. Add roundtrip test. |
| Phase 3 (adapters) | LOW | Moving dead code has zero runtime risk. |
| Phase 4 (file splits) | MEDIUM | Internal restructuring. Function signatures unchanged. |
| Phase 5 (minor merges) | LOW | Small files, clear ownership. |
| Phase 6 (lib.rs cleanup) | LOW | Re-exports ensure backward compatibility. |

**Total regression risk**: LOW — this is pure structural refactoring with no functional changes. Every phase is verifiable with `cargo test && cargo clippy`.

---

## 8. Summary

| Metric | Before | After |
|--------|--------|-------|
| Root-level .rs files | 20 | 2 (main.rs, lib.rs) + tracing_init.rs |
| Directories | 13 | 15 (+ core/, config/, adapters/, testing/) |
| Files > 800 lines | 5 | 0 |
| Dead code modules | 2 (admin/, network stubs) | 0 (admin deleted, network kept as placeholder) |
| Provider code isolation | Scattered across 8+ files | Centralized in adapters/ |
| Config monolith | 1,768 lines in 1 file | ~10 files, ~170 lines avg |
| Max file size | 1,768 lines | ~500 lines target |
| Privacy boundaries | None (everything pub) | core/ public, internals pub(crate) |
| Import style | Mixed (super:: and crate::) | Consistent crate:: |

---

## 9. Dynamic Quota Engine (cor directive: Adaptive Memory Scaling)

> **Directive cor**: Remove fixed thread modes (Light/Normal/Heavy/Max). Daemon calculates quota dynamically per agent. Variables: agent count, resources, activity, Anthropic quota remaining. `thread_mode` in DB stays for lifecycle (normal/archive) but quota is no longer mode-mapped.

### 9.1 Current State — Fixed Mode Architecture

**ThreadMode enum** (`src/agent.rs:90-143`):
- 4 fixed values: `Light(15)`, `Normal(50)`, `Heavy(100)`, `Max(200)`
- Stored as TEXT in `agents` table (`registry.db`, migration V2)
- User-selectable via GUI dropdown or `agent_configure` MCP tool

**Quota enforcement chain** (8 production callsites + 3 fallbacks):

| # | File | Line | Role |
|---|------|------|------|
| 1 | `daemon/capture_queue.rs` | L167 | `agent.thread_mode.quota()` — enforcement principal |
| 2 | `daemon/ipc_server.rs` | L363-373 | `set_thread_mode` handler + `mode.quota()` |
| 3 | `gui/commands.rs` | L849-850 | `.quota()` in list_agents response |
| 4 | `gui/commands.rs` | L921,971-974 | add_agent with ThreadMode |
| 5 | `gui/commands.rs` | L1081 | update_agent thread_mode in AgentUpdate |
| 6 | `gui/commands.rs` | L1107-1109 | IPC send set_thread_mode to daemon |
| 7 | `mcp/tools/agents.rs` | ~L142 | `optional_str(params, "thread_mode")` in agent_configure |
| 8 | `cli/agent.rs` | L100 | `ThreadMode::Normal` default at CLI registration |

Plus 3 fallback callsites in `capture_queue.rs` (L180/184/190) hardcoding `quota=15`.

> **Sub correction**: Initial count was 6 (enforcement-only). Full inventory is 8 production + 3 fallbacks. Phase C must cover all 11.

**ConnectionPool** (`daemon/connection_pool.rs`):
- `thread_quota: AtomicUsize` — already accepts arbitrary `usize` (not mode-restricted)
- `set_thread_quota()`, `refresh_quota()`, `get_thread_quota()` — all work with raw numbers

**BeatState** (`storage/beat.rs:63`):
- `quota: usize` — plain integer, persisted to `beat.json`, backward-compatible default 50

**ThreadManager::enforce_quota()** (`intelligence/thread_manager.rs:586-614`):
- Accepts arbitrary `usize` quota
- Sorts active threads by importance ASC, weight ASC
- Suspends least-important excess threads
- Deterministic and reversible

**Verdict: Infrastructure is READY.** ConnectionPool, BeatState, and enforce_quota all accept arbitrary quota values. The only code tied to fixed modes is the `ThreadMode` enum itself and the 6 callsites above.

### 9.2 Shared Memory Pool Analysis

**Current shared.db** (`storage/shared_storage.rs`):
- 7 operations: publish, unpublish, subscribe, unsubscribe, discover, list_published, update_sync
- All sharing is **manual** via `ai_share` MCP tool — zero auto-promotion
- `importance_score` affects decay half-life only, NOT sharing decisions
- Discovery is topic-based (`LIKE` matching), not importance-ranked
- Subscription is metadata-only — no automatic content sync

**Gap for cor's Option 2 (shared memory pool that grows)**: No auto-promotion mechanism exists. Need:
- Importance threshold check in periodic_tasks (new task)
- Auto-publish to shared.db when `importance > threshold`
- Auto-tag with `__shared__` for decay protection (pattern already exists in share.rs:20-22)

### 9.3 Proposed Design — Dynamic Quota Engine

#### Architecture

New module: `src/daemon/quota_engine.rs` (~200 LOC)

```rust
/// Dynamic Quota Engine — replaces fixed ThreadMode quota mapping.
/// Runs every prune cycle (~5 min) in periodic_tasks.rs.
pub struct QuotaEngine;

impl QuotaEngine {
    /// Compute dynamic quota for an agent based on project context.
    pub fn compute(ctx: &QuotaContext) -> usize {
        let base = Self::base_from_agents(ctx.total_agents);
        let activity_factor = Self::activity_multiplier(ctx.activity);
        let resource_factor = Self::resource_multiplier(ctx.resources);
        let quota_factor = Self::anthropic_quota_factor(ctx.anthropic_quota);

        let raw = (base as f64 * activity_factor * resource_factor * quota_factor) as usize;
        raw.clamp(MIN_QUOTA, MAX_QUOTA)
    }
}

pub struct QuotaContext {
    pub total_agents: usize,          // AgentRegistry::count() for project
    pub activity: AgentActivity,       // From BeatState metrics
    pub resources: SystemResources,    // Thread/bridge counts, disk usage
    pub anthropic_quota: QuotaStatus,  // From heartbeat MAX probe
}
```

#### Quota Formula Variables

| Variable | Source | Already collected? | Impact |
|----------|--------|--------------------|--------|
| `total_agents` | `AgentRegistry::count(conn, project_hash)` | YES (registry.rs:418) | More agents → higher per-agent quota |
| `last_interaction_beat` | `BeatState.last_interaction_beat` | YES (beat.rs) | Dormant agents → lower quota |
| `prompt_count` | `BeatState.prompt_count` | YES (beat.rs) | High activity → higher quota |
| `tool_call_count` | `BeatState.tool_call_count` | YES (beat.rs) | High tool usage → higher quota |
| `context_compactions` | `BeatState.context_compaction_count` | YES (beat.rs) | Memory pressure indicator |
| `active_thread_count` | `ThreadStorage::count_by_status()` | YES (threads.rs) | Current usage level |
| `quota_status` | `BeatState.quota_status_*` | YES (beat.rs) | Anthropic quota remaining |
| `disk_usage` | `std::fs::metadata` on agent data dir | YES (always available) | Resource constraint |
| `CPU/memory` | `sysinfo` crate | NO — GUI-only feature | Needs `sysinfo` as non-optional dep |

#### Base Quota Scaling Formula

```
base_quota(n_agents) = SOLO_QUOTA / sqrt(n_agents)

Where:
  SOLO_QUOTA = 100  (single agent gets 100 threads)
  1 agent  → 100 threads
  2 agents → 70 each (140 total)
  4 agents → 50 each (200 total)
  9 agents → 33 each (300 total)

Total project capacity grows with sqrt(n), preventing linear explosion
while giving each agent meaningful room.
```

#### Activity Multiplier (0.5x — 1.5x)

```
activity_factor = clamp(
    0.5 + (prompts_last_hour / ACTIVITY_NORMALIZATION),
    0.5,  // dormant agents get 50% of base
    1.5   // hyperactive agents get 150% of base
)
```

#### Anthropic Quota Factor (0.3x — 1.0x)

```
anthropic_factor = match quota_status {
    Healthy     → 1.0   // Plenty of quota — full allocation
    Warning     → 0.7   // Approaching limits — reduce memory churn
    Critical    → 0.5   // Near exhaustion — conserve aggressively
    Exhausted   → 0.3   // No quota — minimal memory, no LLM-dependent ops
}
```

#### Absolute Bounds

```
MIN_QUOTA = 5    // Always allow at least 5 active threads
MAX_QUOTA = 300  // Hard ceiling regardless of factors
```

### 9.4 Migration Path — ThreadMode Deprecation

**Phase A**: Add `QuotaEngine` alongside existing ThreadMode (non-breaking)
1. Create `daemon/quota_engine.rs` with `compute()` function
2. Add `quota_engine_enabled: bool` to `DaemonConfig` (default: false)
   - **CRITICAL**: Field MUST have `#[serde(default)]` attribute — existing `daemon_config.json` files lack this field. Without it, daemon deserialization fails on upgrade (sub-verified risk).
3. In `periodic_tasks.rs` quota_sync: if enabled, use QuotaEngine instead of `thread_mode.quota()`
4. Log both values for comparison: `tracing::info!(mode_quota, dynamic_quota, "Quota comparison")`

**Phase B**: Switch default to dynamic (opt-out for legacy)
1. Set `quota_engine_enabled: true` default
2. `set_thread_mode` IPC method: warn that mode is deprecated, set as override hint
3. GUI: replace dropdown with "auto" + optional manual override

**Phase C**: Remove ThreadMode quota mapping
1. `ThreadMode::quota()` method removed
2. `ThreadMode` enum kept for lifecycle semantics only (if needed) or removed entirely
3. 6 callsites updated to read from pool cache (already computed by engine)
4. DB migration: `thread_mode` column becomes optional/deprecated

**Estimated LOC**:
- Phase A: ~200 (quota_engine.rs) + ~30 (periodic_tasks.rs integration) + ~15 (daemon config)
- Phase B: ~50 (IPC deprecation warnings, GUI change)
- Phase C: ~120 (cleanup of 8 production callsites + 3 fallbacks, remove enum method, update 12 test refs)

### 9.5 Shared Memory Auto-Promotion (cor Option 2)

**New periodic task** in `periodic_tasks.rs`:

```rust
// Auto-promote high-importance threads to shared.db
run_task("auto_share_promotion", || {
    let threshold = guardian_config.auto_share_importance_threshold; // default: 0.8
    let candidates = ThreadStorage::list_above_importance(&conn, threshold);
    for thread in candidates {
        if SharedStorage::count_by_thread_id(&shared_conn, &thread.id)? == 0 {
            // Not already shared — auto-promote
            SharedStorage::publish(&shared_conn, SharedThread::from_thread(&thread, agent_id));
            // Tag for decay protection
            ThreadStorage::add_label(&conn, &thread.id, "__shared__");
        }
    }
});
```

**Estimated LOC**: ~60 (new periodic task) + ~20 (config field) + ~15 (list_above_importance query)

### 9.6 Coordinator Memory Hub (cor Option 3)

The coordinator agent (`cor`) gets a quota boost:

```rust
fn compute(ctx: &QuotaContext) -> usize {
    let base = Self::base_from_agents(ctx.total_agents);
    let role_factor = if ctx.agent_role == "coordinator" { 2.0 } else { 1.0 };
    // ... other factors ...
    (base as f64 * role_factor * activity_factor * ...) as usize
}
```

**Already supported**: `Agent` struct has `report_to` field (registry V5 migration) and coordination hierarchy is tracked. `AgentRegistry::build_hierarchy_tree()` identifies coordinators.

**Estimated LOC**: ~15 (role factor in quota engine)

### 9.7 Resource Monitoring Gap

**Current**: `sysinfo` crate is optional, GUI-only (`features = ["gui"]`).

**For Dynamic Quota Engine**: CPU/memory monitoring should be available in daemon (headless) builds.

**Options**:
1. **Make `sysinfo` non-optional** — adds ~500KB to binary, always available
2. **Lightweight alternative** — read `/proc/meminfo` and `/proc/stat` on Linux (zero deps, but Linux-only)
3. **Skip system resources** — use only thread/bridge counts and disk usage as resource proxies

**Recommendation**: Option 3 for V1 of DQE (avoid new dependency). Thread count relative to quota is already the best proxy for memory pressure in this system. Add `sysinfo` as non-optional in V2 if needed.

### 9.8 Integration with Arborescence

The Dynamic Quota Engine fits cleanly in the target arborescence:

```
src/daemon/
├── quota_engine.rs    # NEW — QuotaEngine::compute(), QuotaContext
├── periodic_tasks.rs  # Modified — quota_sync calls QuotaEngine
├── connection_pool.rs # Unchanged — already accepts arbitrary usize
├── ...

src/config/
├── daemon.rs          # Modified — add quota_engine_enabled, auto_share_importance_threshold
├── ...
```

**No new modules needed** — DQE lives inside existing `daemon/` module. Config extends existing `DaemonConfig`.

### 9.9 Summary — Dynamic Quota Engine

| Aspect | Status |
|--------|--------|
| ConnectionPool accepts arbitrary quota | READY |
| BeatState accepts arbitrary quota | READY |
| enforce_quota() works with any value | READY |
| Agent count per project | READY (AgentRegistry::count) |
| Activity metrics collection | READY (BeatState fields) |
| Anthropic quota tracking | READY (heartbeat MAX) |
| System resources (CPU/RAM) | NOT READY (GUI-only) — use proxies for V1 |
| Auto-promotion to shared.db | NOT IMPLEMENTED — ~95 LOC |
| Coordinator quota boost | NOT IMPLEMENTED — ~15 LOC |
| QuotaEngine module | NOT IMPLEMENTED — ~200 LOC |
| **Total new code** | **~310 LOC** (Phase A only) |
| **Migration risk** | **LOW** (additive, opt-in via config flag) |
