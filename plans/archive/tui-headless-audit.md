# Audit — Headless/TUI Capability for Remote Servers

**Auditor**: arc (architect) | **Reviewer**: sub (triangulation)
**Date**: 2026-02-22 (updated 2026-02-24)
**Codebase**: ai-smartness v1.1.0, Rust, ~24K LOC, 117 files
**Scope**: Per cor's directive — 100% of functionalities must be accessible without GUI Tauri + cross-project interaction assessment

---

## 1. Executive Summary

**Verdict: HEADLESS-READY with 15 functional gaps + 3 isolation bugs + zero cross-project capability**

The Rust binary compiles cleanly without `--features gui` (`default = []`). The GUI module is properly gated with `#[cfg(feature = "gui")]` and provides a helpful error message when invoked without the feature. All non-GUI code paths are fully functional.

However:
- **15 operations** are currently GUI-only (no CLI or MCP equivalent)
- **3 isolation bugs** leak data across project boundaries
- **Cross-project interaction** has dead scaffolding (`federation_links` table, `gossip_cross_project` config) but zero implementation

**Coverage stats:**
- CLI commands: 12 command groups, 23 leaf commands
- MCP tools: 72 tools
- GUI Tauri commands: ~40 commands
- Operations with CLI or MCP path: ~85%
- **GUI-only gaps: 15 operations (15%)**

---

## 2. Build Verification

| Aspect | Status |
|--------|--------|
| `default = []` in Cargo.toml | Headless by default |
| `cargo build --no-default-features` | Clean build (11 unrelated warnings) |
| `#[cfg(not(feature = "gui"))]` stub | Yes — prints error + exit(1) |
| `build.rs` conditioned on gui | Yes — `tauri_build::build()` gated |
| Non-GUI imports of GUI types | Zero |
| GUI crate deps (tauri, sysinfo) | All `optional = true` |

**Headless build command:**
```bash
cargo build --release          # default = headless
cargo build --features gui     # explicit GUI
```

---

## 3. Coverage Matrix by Category

### 3.1 Agents — 1 GAP + 1 PARTIAL

| Operation | CLI | MCP | GUI | Status |
|-----------|-----|-----|-----|--------|
| Register agent | `agent add` | — | `add_agent` | OK |
| Remove agent | `agent remove` | — | `remove_agent` | OK |
| List agents | `agent list` | `agent_list` | `list_agents` | OK |
| Update metadata | — | `agent_configure` | `update_agent` | **PARTIAL** (1) |
| Hierarchy tree | `agent hierarchy` | — | `get_hierarchy` | OK |
| Agent tasks | `agent tasks <id>` | `agent_tasks` | — | OK |
| Select agent | `agent select` | `ai_agent_select` | — | OK |
| Agent status | — | `agent_status` | — | OK |
| Query by capability | — | `agent_query` | — | OK |
| Agent cleanup | — | `agent_cleanup` | — | OK |
| **Purge agent DB** | — | — | `purge_agent_db` | **GAP** |

(1) **Sub triangulation correction F1**: `agent_configure` (agents.rs:133-147) hardcodes `specializations: None, capabilities: None` instead of reading from MCP params. `AgentUpdate` (registry.rs:759-767) supports these fields, and `update()` applies them if `Some`. Fix: add `optional_str_array(params, "specializations")` and `capabilities` params. Current parity: **PARTIAL** (missing specializations + capabilities).

### 3.2 Threads — 3 GAPS

| Operation | CLI | MCP | GUI | Status |
|-----------|-----|-----|-----|--------|
| List threads | `threads` | `ai_thread_list` | `get_threads` | OK |
| Search threads | `search <q>` | `ai_thread_search` | `search_threads` | OK |
| Create thread | — | `ai_thread_create` | — | OK |
| Delete thread | — | `ai_thread_rm` | `delete_thread` | OK |
| Delete batch | — | `ai_thread_rm_batch` | — | OK |
| Activate | — | `ai_thread_activate` | — | OK |
| Suspend | — | `ai_thread_suspend` | — | OK |
| Purge by status | — | `ai_thread_purge` | — | OK |
| Rename | — | `ai_rename` / `ai_rename_batch` | — | OK |
| Manage labels | — | `ai_label` | — | OK |
| Show labels | — | `ai_labels_suggest` | `list_all_labels` | OK |
| Manage concepts | — | `ai_concepts` / `ai_backfill_concepts` | — | OK |
| Rate importance | — | `ai_rate_importance` | — | OK |
| Rate context | — | `ai_rate_context` | — | OK |
| Mark used | — | `ai_mark_used` | — | OK |
| Merge / split | — | `ai_merge` / `ai_split` + batch variants | — | OK |
| **Thread detail** | — | — | `get_thread_detail` | **GAP** |
| **Filter by label** | — | — | `search_threads_by_label` | **GAP** |
| **Filter by topic** | — | — | `search_threads_by_topic` | **GAP** |

Note (sub F2 confirmed): `ai_thread_search` (threads.rs:129-137) calls `ThreadStorage::search()` which does fuzzy LIKE on JSON columns — NOT the same as `search_by_labels()` (L457) or `search_by_topics()` (L423) which do exact array entry matching. `search()` offers accidental partial coverage but without guarantees.

### 3.3 Bridges — FULL COVERAGE

| Operation | CLI | MCP | Status |
|-----------|-----|-----|--------|
| List bridges | `bridges` | `ai_bridges` | OK |
| Bridge analysis | — | `ai_bridge_analysis` | OK |
| Scan orphans | — | `ai_bridge_scan_orphans` | OK |
| Purge by status | — | `ai_bridge_purge` | OK |
| Kill / kill batch | — | `ai_bridge_kill` / `_batch` | OK |

### 3.4 Messages & Shared Cognition — FULL COVERAGE

| Operation | MCP | Status |
|-----------|-----|--------|
| Cognitive message | `ai_msg_focus` | OK |
| Ack message | `ai_msg_ack` | OK |
| Send / broadcast | `msg_send` / `msg_broadcast` | OK |
| Inbox / reply | `msg_inbox` / `msg_reply` | OK |
| Share / unshare / publish | `ai_share` / `ai_unshare` / `ai_publish` | OK |
| Discover / subscribe / sync | `ai_discover` / `ai_subscribe` / `ai_sync` | OK |
| Recommendations | `ai_recommend` | OK |
| Shared status | `ai_shared_status` | OK |

### 3.5 Configuration — 2 GAPS (BOTH FULL GAPS)

| Operation | CLI | MCP | GUI | Status |
|-----------|-----|-----|-----|--------|
| Show full config | `config show` | — | `get_settings` | OK |
| Get config key | `config get <key>` | — | — | OK |
| Set config key | `config set <key> <val>` | — | `save_settings` | OK |
| **Daemon settings** | — | — | `get/save_daemon_settings` | **FULL GAP** |
| **Backup settings** | — | — | `get/save_backup_settings` | **GAP** |

**Sub triangulation correction F5**: DaemonConfig (config.rs:1203) is a **completely separate struct** with its own file `daemon_config.json` (config.rs:1251), distinct from `config.json` (GuardianConfig). `config get/set daemon.*` returns "Key not found" because the `daemon` prefix does not exist in GuardianConfig. G10 is a **full gap**, not partial.

Fix options: (a) New MCP tool `ai_daemon_config` (read/write daemon_config.json), or (b) extend `config get/set` to detect `daemon.*` prefix and route to DaemonConfig.

### 3.6 Projects — 1 GAP

| Operation | CLI | MCP | GUI | Status |
|-----------|-----|-----|-----|--------|
| Add project | `project add` | — | `add_project` | OK |
| Remove project | `project remove` | — | `remove_project` | OK |
| List projects | `project list` | — | `list_projects` | OK |
| **Update project** | — | — | `update_project` | **GAP** |

### 3.7 System Status — 3 GAPS

| Operation | CLI | MCP | GUI | Status |
|-----------|-----|-----|-----|--------|
| Memory status | `status` | `ai_status` | `get_dashboard` | OK |
| System info | — | `ai_sysinfo` | — | OK |
| Daemon start/stop/status | `daemon *` | — | `daemon_*` | OK |
| Cross-agent metrics | — | `metrics_cross_agent` | — | OK |
| Health check | — | `health_check` | — | OK |
| Lock / unlock memory | — | `ai_lock` / `ai_unlock` / `ai_lock_status` | — | OK |
| User profile | — | `ai_profile` | `get/save_user_profile` | OK |
| Topics | — | `ai_topics` / `topics_network` | `list_all_topics` | OK |
| **System resources** | — | — | `get_system_resources` | **GAP** |
| **Debug logs** | — | — | `get/save_debug_logs` | **GAP** |
| **Check update** | — | — | `check_update` | **GAP** |

### 3.8 Backup & Maintenance — 3 GAPS

| Operation | CLI | MCP | GUI | Status |
|-----------|-----|-----|-----|--------|
| Create backup | — | `ai_backup` (create) | `trigger_backup` | OK |
| Restore backup | — | `ai_backup` (restore) | `restore_backup` | OK |
| Backup status | — | `ai_backup` (status) | — | OK |
| Cleanup titles | — | `ai_cleanup` | — | OK |
| ONNX setup | `setup-onnx` | — | — | OK |
| Beat / self-wake | — | `beat_wake` | — | OK |
| **List backups** | — | — | `list_backups` | **GAP** |
| **Delete backup** | — | — | `delete_backup` | **GAP** |
| **Reindex agent** | — | — | `reindex_agent` | **GAP** |

### 3.9 Focus, Recall, Misc — FULL COVERAGE

| Operation | MCP | Status |
|-----------|-----|--------|
| Focus / unfocus | `ai_focus` / `ai_unfocus` | OK |
| Pin content | `ai_pin` | OK |
| Semantic recall | `ai_recall` | OK |
| Suggestions | `ai_suggestions` | OK |
| Help | `ai_help` | OK |
| VSCode window | `ai_windows` | OK (IDE-specific, acceptable) |

---

## 4. Definitive Gap List — 15 Operations

### Tier 1 — Critical Functional Gaps (8)

These block real headless workflows:

| # | GUI Command | What it does | Impact |
|---|-------------|-------------|--------|
| G1 | `get_thread_detail` | Full thread + messages + bridges composite view | Cannot inspect thread content headless |
| G2 | `search_threads_by_label` | Filter threads by label | No label-based navigation headless |
| G3 | `search_threads_by_topic` | Filter threads by topic | No topic-based navigation headless |
| G4 | `purge_agent_db` | Nuclear delete all threads/bridges/messages | Cannot reset agent memory headless |
| G5 | `reindex_agent` | Rebuild embeddings + optionally reset weights | Cannot repair corrupted embeddings headless |
| G6 | `update_project` | Modify project name/path/provider | Cannot fix project metadata headless |
| G7 | `list_backups` | Enumerate available backup files | Cannot see what backups exist headless |
| G8 | `delete_backup` | Remove a specific backup file | Cannot manage backup storage headless |

### Tier 2 — Operational Visibility Gaps (4)

| # | GUI Command | What it does | Impact |
|---|-------------|-------------|--------|
| G9 | `get_system_resources` | CPU/mem/disk + daemon pool info | No resource monitoring headless |
| G10 | `get/save_daemon_settings` | Daemon config read/write | **Full gap** — DaemonConfig is in separate `daemon_config.json`, not accessible via `config get/set` |
| G11 | `get/save_backup_settings` | Backup config read/write | Separate config, same issue as G10 |
| G12 | `get/save_debug_logs` | Stream daemon.log (paginated) | Workaround: `tail -f` on log file directly |

### Tier 3 — Nice-to-Have (3)

| # | GUI Command | What it does | Impact |
|---|-------------|-------------|--------|
| G13 | `get_project_overview` | Aggregated multi-agent dashboard | Nice dashboard, not blocking |
| G14 | `check_update` | GitHub release version check | `cargo install` or manual check |
| G15 | `get_all_agents_threads` | Multi-agent graph visualization | Visualization-only, not functional |

### Not a gap (GUI-inherent)

- `open_debug_window` — Tauri devtools window. Headless users use `tail -f` on logs.

---

## 5. Remediation Recommendations

### Phase A — New/Extended MCP Tools

| Tool | Covers Gap | Implementation | Sub recommendation |
|------|-----------|----------------|-------------------|
| **`ai_thread_detail`** (new) | G1 | `ThreadStorage::get()` + `list_messages()` + `BridgeStorage::get_for_thread()` | Separate tool, not a param on `ai_thread_list` (sub Q1) |
| **`ai_thread_search`** scope extension | G2, G3 | Add `scope: "labels"\|"topics"\|null` → dispatch to `search_by_labels()` / `search_by_topics()` / `search()` | Agreed (sub Q2) |
| **`ai_purge`** (new) | G4 | `DELETE FROM threads; DELETE FROM bridges; DELETE FROM thread_messages; VACUUM` with `confirm: true` required | MCP + CLI dual path, confirm mandatory (sub Q3) |
| **`ai_reindex`** (new) | G5 | Async: immediate `{"status": "reindex_started"}` + daemon background processing | Non-blocking, state visible via `ai_sysinfo` (sub Q4) |
| **`ai_backup`** extension | G7, G8 | Add `action: "list"` + `action: "delete"` | Extend existing tool |
| **`ai_daemon_config`** (new) | G10 | Read/write `daemon_config.json` directly | Or extend `config get/set` to detect `daemon.*` prefix |
| **`agent_configure`** fix | partial parity | Add `specializations` and `capabilities` params (agents.rs:140-141) | Trivial fix (sub F1) |

### Phase B — New CLI Commands

| New CLI Command | Covers Gap |
|----------------|-----------|
| `project update <hash> [--name X] [--path Y]` | G6 |
| `agent update <id> [--role X] [--thread-mode Y] [--full-permissions]` | CLI parity with MCP `agent_configure` |
| `agent purge <id> [--confirm]` | G4 (CLI path) |

### Phase C — Operational Visibility

| Fix | Covers Gap |
|-----|-----------|
| `ai_sysinfo` extension: add CPU/mem/disk fields (optional `sysinfo` crate without Tauri) | G9 |
| `ai_debug_logs` MCP tool: read last N lines of daemon.log | G12 |

### Phase D — Deployment Script

No Rust-specific install script exists. Create `scripts/install-headless.sh`:
1. `cargo build --release` (headless, no GUI deps)
2. Copy binary to `~/.local/bin/`
3. `ai-smartness init` for current project
4. `ai-smartness daemon start`
5. Print MCP config snippet for Claude Code / other AI providers

---

## 6. install.sh Clarification

The existing `install.sh` (1420 lines) is the **legacy Python-based installer**. It:
- Installs a Python MCP server, not the Rust binary
- Uses python3 + sentence-transformers
- Has interactive prompts (language, mode, permissions, backup)
- Is NOT relevant to the Rust headless deployment

A new Rust-specific headless install script is needed (Phase D above).

---

## 7. Architectural Notes

### What works perfectly headless today

1. **Daemon**: `ai-smartness daemon start/stop/status` — fully functional, no GUI dependency
2. **Hooks**: All 4 hook types (inject, capture, health, pretool) work headless via `.claude/settings.json`
3. **MCP server**: `ai-smartness mcp <hash> <agent>` — JSON-RPC on stdin/stdout, 72 tools
4. **Memory operations**: Thread CRUD, bridge management, search, recall — all via MCP
5. **Multi-agent**: Agent registration, hierarchy, task delegation, messaging — all via CLI+MCP
6. **Configuration**: Global config via `config show/get/set` CLI commands (but NOT daemon/backup config)
7. **ONNX embeddings**: `setup-onnx` downloads runtime, no GUI needed

### What fundamentally requires GUI

Only `open_debug_window` (Tauri devtools) has no headless equivalent and doesn't need one.

---

## 8. Cross-Project Agent Interaction

*Added per cor's directive — prerequisite for P2P network design.*

### 8.1 Current Isolation Model

The codebase uses a **structural isolation** model:

| Layer | Isolation | Mechanism |
|-------|-----------|-----------|
| Agent DB | Per-agent, per-project | File: `{data_dir}/projects/{hash}/agents/{agent_id}.db` |
| Shared DB | Per-project | File: `{data_dir}/projects/{hash}/shared.db` |
| Registry DB | **GLOBAL** | File: `{data_dir}/registry.db` — all projects, all agents |
| Wake signals | **GLOBAL** (design bug) | File: `{data_dir}/wake_signals/{agent_id}.signal` — no project_hash in path |
| Daemon | **GLOBAL** | Connection pool serves all (project, agent) pairs simultaneously |

### 8.2 Existing Cross-Project Capabilities (Bugs)

Three MCP tools/APIs leak data across project boundaries:

| Bug | Location | Severity | Description |
|-----|----------|----------|-------------|
| **B1** | `agent_query` (agents.rs:44-65) → `Discovery::find_by_capability()` (discovery.rs:11-26) | **HIGH** | No `WHERE project_hash = ?` clause. Returns agents from ALL projects. Information leak. |
| **B2** | `agent_configure` (agents.rs:126-151) | **HIGH** | Uses caller-supplied `project_hash` (L131), NOT `ctx.project_hash`. Can modify agents in ANY project via the global `registry.db`. |
| **B3** | `task_status` (agents.rs:334-356) → `AgentTaskStorage::get_task()` (tasks.rs:101-112) | **MEDIUM** | Queries `WHERE id = ?1` with no project filter. Can read any task by ID regardless of project. |

**Additional design issue**: Wake signals at `{data_dir}/wake_signals/{agent_id}.signal` have no project_hash component. Two agents named `coder1` in different projects would collide.

### 8.3 Dead Scaffolding (Schema/Config exists, zero implementation)

| Component | What exists | Where | What's missing |
|-----------|------------|-------|----------------|
| `gossip_cross_project` | Config field (bool, default false) + GUI toggle | config.rs:1212-1213, index.html:295 | **Never read by any Rust code.** Zero matches in `src/daemon/` or `src/intelligence/`. Dead code. |
| `federation_links` table | DDL with proper schema (direction, status, canonical ordering) | migrations.rs:367-377 | **Zero queries, zero inserts, zero reads.** Table created but never accessed. |
| `workspace_path` field | Stored, read, returned in Agent struct | agent.rs:177-178, registry.rs:55-58 | **Never used for logic.** Comment says "Workspace isolation" but no enforcement exists. Advisory metadata only. |
| `AgentRegistry::list(None)` | API can return agents across all projects | registry.rs:357-399 | Not exposed via MCP `agent_list` (always passes `Some(ctx.project_hash)`). Internal-only capability. |

### 8.4 What is Completely Missing

For two agents in different projects to interact, the following is needed:

| Capability | Status | What would be needed |
|-----------|--------|---------------------|
| **Cross-project thread sharing** | Missing | Mechanism to query another project's `shared.db`, or a global shared layer |
| **Cross-project messaging** | Missing | Both cognitive inbox (`ai_msg_focus`, L151: uses `ctx.project_hash`) and MCP messages (`msg_send`, writes to per-project `shared.db`) are project-scoped |
| **Cross-project gossip/bridges** | Missing | Gossip (gossip.rs) operates on single agent DB. `gossip_cross_project` config exists but is never read |
| **Federation link management** | Missing | `federation_links` table exists but has zero CRUD operations, zero query logic, zero API |
| **Cross-project task delegation** | Missing | `task_delegate` (agents.rs:313) validates target agent exists in same project (tasks.rs:42-44) |
| **Project-scoped wake signals** | Missing | Wake signal paths need `{project_hash}/{agent_id}.signal` to prevent collisions |

### 8.5 Global Infrastructure That Could Enable Cross-Project

The daemon already operates globally and could serve as the backbone:

| Component | Current state | Cross-project readiness |
|-----------|--------------|------------------------|
| **Connection Pool** (connection_pool.rs:19-22) | Uses `AgentKey { project_hash, agent_id }`. Holds connections to agents from ANY project. | Ready — can open any (project, agent) DB pair |
| **IPC Server** (ipc_server.rs:124-138) | Parses `project_hash` + `agent_id` from any IPC request | Ready — any caller can target any project |
| **Controller** (controller.rs:535-587) | `discover_active_agents()` scans ALL projects' directories | Ready — already iterates cross-project |
| **Prune Loop** (periodic_tasks.rs:59) | `pool.active_keys()` processes all agents from all projects | Ready — but each prune cycle is isolated per-agent |

### 8.6 Cross-Project Remediation Path

**Phase 1 — Fix isolation bugs (B1-B3)**
- B1: Add `project_hash` filter to `Discovery::find_by_capability()` and `find_by_specialization()`
- B2: Validate `params.project_hash == ctx.project_hash` in `agent_configure` (or remove the param and always use `ctx.project_hash`)
- B3: Add `AND project_hash = ?2` to `AgentTaskStorage::get_task()`, `update_task_status()`, `delete_task()`
- Wake signals: Add project_hash to signal file path

**Phase 2 — Enable opt-in cross-project discovery**
- Implement `federation_links` CRUD (create/delete/list links between projects)
- Extend `agent_query` with `scope: "federation"` to search linked projects only
- Implement `gossip_cross_project` logic: when enabled, gossip cycle includes threads from federated projects

**Phase 3 — Enable cross-project messaging**
- Add `ai_msg_cross` tool: send message to agent in a federated project
- Route through global `registry.db` for target resolution, then write to target project's `shared.db`
- Requires federation link to be active (Phase 2)

**Phase 4 — P2P network readiness**
- Federation links become the local model for network links
- Each P2P peer exposes its `federation_links` + `shared_threads` where `visibility = 'network'`
- Daemon controller manages both local federation and remote P2P sync

### 8.7 Cross-Project Summary

| Aspect | Status |
|--------|--------|
| Isolation model | Structural (per-project DB files) + query-level (project_hash WHERE clauses) |
| Current cross-project capability | **Zero intentional, 3 bugs** |
| Dead scaffolding | `federation_links` table, `gossip_cross_project` config, `workspace_path` field |
| Global infrastructure | Daemon pool, IPC, controller — all already cross-project capable |
| Effort to enable Phase 1 (bug fixes) | ~50 LOC |
| Effort to enable Phase 2 (federation) | ~300-400 LOC |
| Effort to enable Phase 3 (messaging) | ~200 LOC |
| P2P readiness | Phase 4 depends on Phases 1-3, then adds network transport |

**Conclusion for P2P**: The local isolation is NOT too strict — it's structurally correct with dead scaffolding already in place for federation. The daemon's global architecture is the right backbone. The path from current state to cross-project interaction is incremental (fix bugs → activate federation_links → enable cross-project messaging → extend to P2P). No architectural rewrites needed.

---

## 9. V2+ Architecture Prerequisites — Provider-Agnostic & Multi-Billing

*Added per cor's strategic directive — prerequisites for multi-provider, multi-billing support.*

### 9.1 Axis 1: CC-CLI Daemon Management

**Current state**: Partially complete.

| Feature | CLI | IPC | GUI | Status |
|---------|-----|-----|-----|--------|
| Start | `daemon start` | — | `daemon_start` | OK (no readiness wait) |
| Stop | `daemon stop` | `shutdown` | `daemon_stop` | OK |
| Status | `daemon status` | `status` | `daemon_status` | OK (basic) |
| **Reload config** | ABSENT | ABSENT | ABSENT | **Complete gap everywhere** |
| **Health check (deep)** | ABSENT | ABSENT | ABSENT | MCP `health_check` is shallow (thread count only) |
| **Log inspection** | ABSENT | ABSENT | `get_debug_logs` | Must use raw `tail -f` |
| **Log level change** | ABSENT | ABSENT | ABSENT | `RUST_LOG` env at startup only |
| **Daemon config** | ABSENT | ABSENT | `get/save_daemon_settings` | **Full gap** (daemon_config.json) |
| **Resource monitoring** | ABSENT | `pool_status`/`queue_status` | `get_system_resources` | CLI cannot see CPU/mem/disk |
| **Restart** | ABSENT | ABSENT | ABSENT | Manual stop+start required |

**Key findings:**
- **No hot-reload**: DaemonConfig read once at startup (daemon/mod.rs:32). No SIGHUP handler — only SIGINT/SIGTERM registered (daemon/mod.rs:137-139), both trigger shutdown.
- **GuardianConfig IS hot-reloaded**: CaptureQueue workers reload `config.json` per-job (capture_queue.rs:250). This works but only for extraction/processing settings.
- **No log rotation**: daemon.log opened in append mode with no size limit, rotation, or truncation.

**Gaps to close for headless daemon management:**

| New Command | Priority | Implementation |
|-------------|----------|---------------|
| `daemon reload` CLI + IPC `"reload"` method | HIGH | SIGHUP handler + DaemonConfig re-read + pool/queue reconfiguration |
| `daemon logs [--follow] [--lines N]` CLI | MEDIUM | Read daemon.log tail, optional follow mode |
| `daemon config show/set` CLI | HIGH | Read/write daemon_config.json (separate from GuardianConfig) |
| `daemon health` CLI (deep) | MEDIUM | Check daemon liveness + pool saturation + queue health + disk space |

---

### 9.2 Axis 2: Provider Portability — Runtime Decoupling

**Current provider agnosticism score: 3/10**

The codebase has two fully generic layers, one dead abstraction, and seven Claude-specific coupling points:

| Component | Classification | Effort | Key evidence |
|-----------|---------------|--------|-------------|
| MCP protocol | **GENERIC** | None | Standard JSON-RPC 2.0, any MCP client works (mcp/jsonrpc.rs) |
| Embeddings | **GENERIC** | None | Local ONNX all-MiniLM-L6-v2, no API dependency (processing/embeddings.rs) |
| Daemon IPC protocol | **GENERIC** | None | Standard JSON-RPC over local socket (daemon/ipc_server.rs) |
| DB schema `provider` field | **ADAPTABLE** | Low | `provider TEXT DEFAULT 'claude'` + `provider_config TEXT` exist (migrations.rs:280-285) |
| AiProvider trait | **DEAD CODE** | Medium | Trait + ClaudeProvider + GenericProvider designed but never wired (provider.rs, hook/providers/) |
| LLM subprocess | **CLAUDE-SPECIFIC** | Medium | Single file `llm_subprocess.rs` hardcodes `claude` binary — **single chokepoint** for 5+ features |
| Hook format (inject/capture) | **CLAUDE-SPECIFIC** | High | 1160-line inject.rs assumes Claude Code's JSON events + `<system-reminder>` tags |
| Hook setup (.claude/ directory) | **CLAUDE-SPECIFIC** | High | .claude/settings.json, settings.local.json, hook event names (hook_setup.rs) |
| Credentials & transcripts | **CLAUDE-SPECIFIC** | High | ~/.claude/.credentials.json, ~/.claude/projects/*/session.jsonl (credentials.rs, transcript.rs) |
| Daemon controller injection | **CLAUDE-SPECIFIC** | Medium | Writes to Claude CLI stdin via /proc/PID/fd/0 (controller.rs:265-369) |
| Config model names | **CLAUDE-SPECIFIC** | Medium | `ClaudeModel` enum (Haiku/Sonnet/Opus) in config.rs:20-47 |

**Ironic finding**: The `AiProvider` trait (provider.rs), `detect_provider()` (hook/providers/mod.rs), `ClaudeProvider`, `GenericProvider`, and `guardcode/injector.rs` represent a **fully designed multi-provider architecture that is entirely dead code**. The runtime bypasses all of it.

**Single highest-impact refactor**: Replace `llm_subprocess.rs` (95 lines) with a generic LLM dispatcher. This file is the chokepoint for extraction, coherence, reactivation, merge evaluation, and concept backfill. The prompts are already provider-agnostic (plain text asking for JSON). Impact: decouple 5+ intelligence features in ~150 LOC.

---

### 9.3 Axis 3: Multi-Provider Agent Integration

**Current state: Schema-ready, runtime-blocked.**

**What exists:**
- `projects.provider` column (migrations.rs:280) — supports per-project provider identity
- `projects.provider_config` JSON blob (migrations.rs:285) — extensible per-provider settings
- `ProjectRegistryTrait::update_project()` — supports changing provider field
- `Agent` struct has no provider field — agents inherit from their project

**What blocks:**
- All project creation hardcodes `provider: "claude"` (cli/project.rs:36, cli/init.rs:49, gui/commands.rs:203)
- No per-agent provider concept — agents cannot be from different providers within a project
- Registry messaging (`msg_send`, `msg_broadcast`) is project-scoped via shared.db — inter-provider messaging within a project works (DB doesn't care about provider), but cross-project messaging doesn't exist (Section 8)
- Hook system assumes ALL agents in a project use the same Claude Code hooks

**Architecture gap for multi-provider agents:**

| Gap | Description | Effort |
|-----|-------------|--------|
| **Per-agent provider field** | Add `provider: Option<String>` to Agent struct + registry | ~30 LOC |
| **Provider-aware hook dispatch** | `inject.rs` / `capture.rs` should check agent's provider to format output | ~200 LOC (if AiProvider trait is wired) |
| **Generic LLM dispatcher** | Replace `llm_subprocess.rs` with trait-based dispatcher | ~150 LOC |
| **Multi-hook-setup** | Support non-Claude hook directories (.cursor/, .continue/, custom) | ~200 LOC per provider |
| **Provider-agnostic session tracking** | Abstract session_id extraction per provider format | ~100 LOC |

**Recommended architecture:**
```
Agent.provider → match {
    "claude" → ClaudeProvider (existing hooks + inject format)
    "openai" → OpenAIProvider (custom stdin format or MCP-only)
    "ollama" → OllamaProvider (MCP-only, no hooks)
    _ → GenericProvider (MCP-only, plain text injection)
}
```

The `HookMechanism` enum already exists in the dead code (provider.rs): `ClaudeCodeHooks`, `CustomStdio`, `McpOnly`, `None`. This design was anticipated.

---

### 9.4 Axis 4: Subscription Switching

**Current state: Anthropic MAX only, zero abstraction.**

**What exists (all Anthropic-specific):**

| Component | Location | What it tracks |
|-----------|----------|---------------|
| Plan detection | credentials.rs:36-59 | Reads `~/.claude/.credentials.json` → `subscriptionType`, `rateLimitTier` |
| MAX identification | credentials.rs:54 | `is_max: sub_type == "max"` string match |
| Tier multiplier | credentials.rs:45-51 | "20x" → 20, "5x" → 5, else → 1 |
| Quota probe | quota_probe.rs:66-79 | POST to `api.anthropic.com/v1/messages` with Haiku, reads unified rate-limit headers |
| BeatState fields | beat.rs:84-106 | `plan_type`, `plan_tier`, `quota_5h`, `quota_7d`, `quota_status_*`, `quota_reset_*`, `quota_alert` |

**What's completely missing:**
- No billing model abstraction — the 5h/7d sliding window concept is Anthropic-specific
- No per-token billing support (OpenAI model)
- No API key billing tracking (no token-based cost accumulation)
- No multi-provider credential discovery
- The ureq HTTP dependency is used exclusively for the Anthropic quota probe (single call in entire codebase: quota_probe.rs:72)

**Architecture needed for subscription switching:**

```
trait BillingProvider {
    fn detect_plan(&self) -> Option<PlanInfo>;
    fn probe_quota(&self, credentials: &Credentials) -> Option<QuotaSnapshot>;
    fn alert_thresholds(&self) -> AlertConfig;
}

enum QuotaSnapshot {
    WindowBased { windows: Vec<WindowQuota> },     // Anthropic: 5h, 7d
    TokenBased { used: u64, limit: u64, cost: f64 }, // OpenAI: per-token
    Unlimited,                                       // Local: Ollama, llama.cpp
}
```

**Effort estimate:**
- Billing trait abstraction: ~100 LOC
- Anthropic implementation (extract from existing code): ~150 LOC
- OpenAI implementation: ~200 LOC
- BeatState generalization (QuotaSnapshot instead of flat fields): ~100 LOC
- Multi-credential discovery (abstract ~/.claude/ path): ~80 LOC

---

### 9.5 Axis 5: Extended Heartbeat / Consumption Tracking

**Current context tracking (Anthropic-only):**

| Metric | Source | Location |
|--------|--------|----------|
| Context tokens | Claude Code JSONL transcript parsing | transcript.rs:26-117 |
| Context percent | `tokens / window_size * 100` | inject.rs:313-351 |
| Compaction detection | Token drop >40% between readings | beat.rs:310-328 |
| 5h utilization | Anthropic unified rate-limit header | quota_probe.rs:94 |
| 7d utilization | Anthropic unified rate-limit header | quota_probe.rs:95 |
| Response latency | Timestamp delta prompt→response | beat.rs:237-246 |
| Tool call count | Incremented per MCP tool call | beat.rs:222-224 |
| Prompt count | Incremented per interaction | beat.rs:216-218 |

**What needs to change for multi-provider:**

| Metric | Current | Multi-provider adaptation |
|--------|---------|--------------------------|
| Context tokens | Reads Claude JSONL (`cache_creation_input_tokens` etc.) | Per-provider token field names. OpenAI: `usage.total_tokens`. Local: estimate from chars. |
| Context window size | Hardcoded 200K default, 1M for beta | Per-model lookup table (GPT-4: 128K, Claude: 200K, Llama: 8K-128K) |
| Quota probe | POST to api.anthropic.com | Per-provider endpoint. OpenAI: `GET /dashboard/billing/usage`. Local: none needed. |
| Billing accumulator | None (window-based only) | Add per-session cost tracking: `Σ (input_tokens * input_price + output_tokens * output_price)` |
| Rate limit headers | `anthropic-ratelimit-unified-*` | Per-provider header parsing. OpenAI: `x-ratelimit-*`. Local: none. |
| Alert thresholds | 80% of 5h, 90% of 7d | Per-billing-model: utilization % for window-based, cost ceiling for token-based |

**New BeatState fields needed:**

```rust
// Replace flat Anthropic fields with generic:
pub billing_provider: Option<String>,        // "anthropic", "openai", "local"
pub billing_model: Option<String>,           // "window", "token", "none"
pub session_cost_usd: Option<f64>,           // Cumulative session cost (token billing)
pub daily_cost_usd: Option<f64>,             // Daily cost accumulator
pub weekly_cost_usd: Option<f64>,            // Weekly cost accumulator
pub cost_alert_threshold_usd: Option<f64>,   // User-defined cost ceiling
// Keep existing for backward compat, add provider prefix
```

---

### 9.6 V2+ Readiness Summary

| Axis | Current Score | Blockers | Effort (LOC) |
|------|-------------|----------|-------------|
| 1. Daemon CLI management | 6/10 | Missing: reload, deep health, log mgmt, daemon config CLI | ~300 |
| 2. Provider portability | 3/10 | `llm_subprocess.rs` chokepoint, hook format coupling, credential paths | ~500 (Phase 1: LLM dispatcher) |
| 3. Multi-provider agents | 2/10 | No per-agent provider, hardcoded "claude", dead AiProvider trait | ~680 (full) |
| 4. Subscription switching | 1/10 | Zero abstraction, all Anthropic-specific billing code | ~630 |
| 5. Extended heartbeat | 4/10 | Generic counters exist (latency, tool calls), quota tracking Anthropic-only | ~400 |

**Recommended priority order:**
1. **Daemon CLI management** (Axis 1) — prerequisite for headless operations, low risk
2. **LLM subprocess abstraction** (Axis 2, Phase 1) — single 95-line file, decouples 5+ features, highest ROI
3. **Wire AiProvider trait** (Axis 2+3) — dead code already exists, just needs activation
4. **Per-agent provider field** (Axis 3) — schema change + registry update
5. **Billing abstraction** (Axis 4+5) — deepest refactor, depends on provider abstraction

**Key architectural insight**: The codebase has two parallel realities:
- **Designed for multi-provider** (AiProvider trait, provider column, GenericProvider, HookMechanism enum)
- **Implemented for Claude-only** (every runtime path hardcodes Claude Code)

The gap between design and implementation is ~1500-2000 LOC. The dead abstractions are architecturally sound and should be activated, not redesigned.

---

### 9.7 Axis 6: Remote GUI Client Mode

*Added per cor's directive — GUI as thin client connected to remote headless daemon.*

**Current architecture: Thick client, 92.5% local operations.**

The GUI is a thick client that opens SQLite databases directly and makes very few IPC calls:

| Metric | Count |
|--------|-------|
| `open_connection()` direct DB calls in GUI | **32** |
| `daemon_ipc_client::` IPC calls in GUI | **4** |
| Direct `std::fs::` operations in GUI | **~18** |
| Ratio local-only : IPC | **~50:4 (92.5% local)** |

**IPC transport layer:**
- Transport: `interprocess::local_socket` (Unix domain sockets on Linux, named pipes on Windows)
- Socket path: `{data_dir}/processor.sock` (daemon/mod.rs:102) — hardcoded local filesystem
- Crate: `interprocess` v2.2 — **no TCP capability**, designed exclusively for local IPC
- Client: `processing/daemon_ipc_client.rs` — connects to local socket only

**GUI daemon interactions (only 4 IPC calls):**
1. `daemon_ipc_client::shutdown()` — stop daemon (commands.rs:147)
2. `daemon_ipc_client::send_method("set_thread_mode", ...)` — notify quota change (commands.rs:1109)
3. `daemon_ipc_client::daemon_status()` — resource monitoring (commands.rs:1303)
4. `daemon_ipc_client::daemon_status()` — check_daemon helper (commands.rs:1626)

Everything else (32 DB opens + 18 FS operations) goes directly to local filesystem.

**Daemon IPC currently supports only 12 methods** (ipc_server.rs:210-416): `ping`, `shutdown`, `status`, `tool_capture`, `prompt_capture`, `injection_usage`, `lock`, `unlock`, `pool_status`, `queue_status`, `set_thread_mode`, `list_active_agents`.

The GUI needs ~30+ operations that the daemon doesn't proxy.

**Verdict: Remote GUI requires a fundamental architecture change.**

**What would be needed:**

| Change | Description | Effort |
|--------|-------------|--------|
| **TCP transport** | Replace `interprocess::local_socket` with `std::net::TcpListener` (or add alongside) | ~200 LOC |
| **Authentication** | Unix sockets have implicit FS-based ACL. TCP needs auth + TLS | ~500 LOC |
| **Daemon RPC expansion** | Add ~25 new IPC methods to proxy all DB/FS operations currently done directly | ~1500 LOC |
| **GUI thin-client rewrite** | Replace all 32 `open_connection()` + 18 `std::fs::` calls with IPC calls | ~800 LOC |
| **Config for remote daemon** | `daemon_host: String`, `daemon_port: u16` in GUI config | ~50 LOC |
| **Total** | | **~3000 LOC** |

**Alternative approach: HTTP/WebSocket API layer**

Instead of expanding the Unix IPC protocol, add a REST/WebSocket API on the daemon:
- Reuse the same `ToolContext` / handler pattern from MCP tools
- Serve on configurable `host:port` with auth token
- GUI becomes a web client talking to the API
- Bonus: enables any HTTP client (scripts, Postman, third-party GUIs)

This aligns better with the meta-layer vision (Section 9.7+) where ai-smartness becomes accessible to diverse clients.

---

## 10. Overall Summary

| Category | Operations | Covered | Gaps |
|----------|-----------|---------|------|
| Agents | 11 | 10 | 1 (purge) + partial (agent_configure) |
| Threads | 21 | 18 | 3 (detail, label filter, topic filter) |
| Bridges | 6 | 6 | 0 |
| Messages & Shared | 14 | 14 | 0 |
| Configuration | 5 | 3 | 2 (daemon/backup settings — FULL gaps) |
| Projects | 4 | 3 | 1 (update) |
| System Status | 11 | 8 | 3 (resources, logs, update check) |
| Backup & Maintenance | 6 | 3 | 3 (list, delete, reindex) |
| Focus / Recall / Misc | 6 | 6 | 0 |
| **Headless Total** | **84** | **71 (85%)** | **15 gaps** |
| Cross-project isolation bugs | — | — | 3 (B1-B3) |
| Cross-project capability | — | — | Zero (dead scaffolding only) |

**Conclusion**: The architecture is fundamentally sound for headless deployment. The 15 gaps are remediable by adding ~6 MCP tools and ~3 CLI commands (~400 LOC). Cross-project interaction requires incremental work on the existing federation scaffolding (~550 LOC for Phases 1-3), with no architectural rewrites needed for P2P readiness.

---

## Appendix: Sub Triangulation Log

| Finding | Arc assessment | Sub correction | Applied |
|---------|--------------|----------------|---------|
| F1 — agent_configure parity | "Complete parity" | **PARTIAL** — specializations + capabilities hardcoded to None | Yes — section 3.1 |
| F2 — ai_thread_search | "Full-text only, true gap" | **Confirmed** — search() gives accidental fuzzy coverage but not exact matching | Yes — section 3.2 note |
| F3 — ai_backup list/delete | "True gaps G7/G8" | **Confirmed** | Yes |
| F4 — No Rust install script | "True" | **Confirmed** | Yes |
| F5 — DaemonConfig access | "Partial gap via config get/set" | **FULL GAP** — DaemonConfig is in separate daemon_config.json, not in GuardianConfig | Yes — section 3.5, G10 |
