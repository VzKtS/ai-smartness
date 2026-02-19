# AI Smartness — Settings Reference

Complete reference for all configurable settings in AI Smartness.
Each setting includes its default value, valid range, and impact on system behavior.

---

## Table of Contents

- [General](#general)
  - [Language & Display](#language--display)
  - [Connection Pool (Daemon)](#connection-pool-daemon)
- [Guardian LLM](#guardian-llm)
  - [Global Settings](#global-settings)
  - [Extraction](#extraction)
  - [Coherence](#coherence)
  - [Reactivation](#reactivation)
  - [Synthesis](#synthesis)
  - [Label Suggestion](#label-suggestion)
  - [Importance Rating](#importance-rating)
- [Thread Matching](#thread-matching)
- [Gossip](#gossip)
- [Recall](#recall)
- [Engram (Multi-Validator Retrieval)](#engram-multi-validator-retrieval)
  - [Validator Weights](#validator-weights)
- [Alert Thresholds](#alert-thresholds)
- [Fallback Patterns](#fallback-patterns)
- [GuardCode (Content Validation)](#guardcode-content-validation)
  - [Custom Messages](#custom-messages)
- [HealthGuard (Memory Health)](#healthguard-memory-health)
  - [Injection Prompts](#injection-prompts)
- [Heartbeat (Agent Liveness)](#heartbeat-agent-liveness)
- [Agent Configuration](#agent-configuration)
  - [Thread Modes](#thread-modes)
- [Hook Setup](#hook-setup)
  - [MCP Permissions](#mcp-permissions)
  - [PreToolUse Hook (Pretool Dispatcher)](#pretooluse-hook-pretool-dispatcher)
- [Beat System](#beat-system)
- [Session State](#session-state)
- [User Profile](#user-profile)
  - [Identity](#identity)
  - [Preferences](#preferences)
  - [Custom Rules](#custom-rules)
- [Guard Write (Plan Mode Enforcement)](#guard-write-plan-mode-enforcement)
- [Virtual Paths](#virtual-paths)
- [Compact (Context Synthesis)](#compact-context-synthesis)
- [Backup](#backup)
- [Reindex](#reindex)
- [Context Window Tracking](#context-window-tracking)
- [Update System](#update-system)
- [Common Enums](#common-enums)
- [Key Constants](#key-constants)

---

## General

### Language & Display

| Setting | Default | Description |
|---------|---------|-------------|
| **Language** | `en` | Language for LLM synthesis outputs (summaries, labels) and GUI interface. Options: `en`, `fr`, `es`. |
| **Mode** | `dark` | GUI display mode. `dark` is easier on the eyes in low-light; `light` provides better contrast in bright environments. |
| **Theme** | `green` | Accent color theme. Options: `green`, `blue`, `yellow`, `orange`, `red`. Pure aesthetic. |

### Connection Pool (Daemon)

Global daemon settings — controls the persistent background process that manages all agent databases.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Auto-start Daemon** | `off` | on/off | Automatically start the daemon when the GUI opens. If off, you must start it manually from the Dashboard. |
| **Max Connections** | `50` | 1–200 | Maximum simultaneous SQLite database connections the daemon keeps open (one per active agent). Higher values support more concurrent agents but use more RAM (~2–5 MB per connection). |
| **Idle Timeout** | `1800` (30 min) | ≥ 60s | Seconds before an idle agent connection is evicted from the pool. Lower values free RAM faster but cause reconnection overhead. |
| **Prune Interval** | `300` (5 min) | ≥ 60s | Interval between periodic maintenance tasks. Affects: gossip bridge discovery, thread weight decay, archive of old suspended threads, orphan bridge cleanup, and WAL checkpoint. **Lower = fresher memory** but more CPU usage. **Higher = lighter CPU** but stale data lingers longer. |
| **Cross-project Gossip** | `off` | on/off | Enable bridge discovery between threads from different projects. **Experimental**: useful for cross-project knowledge transfer but may create noisy connections between unrelated codebases. |

### Agent Network

Multi-agent communication and team collaboration settings.

| Setting | Default | Options | Description |
|---------|---------|---------|-------------|
| **Messaging Mode** | `cognitive` | `cognitive`, `inbox` | Communication mode between agents. **Cognitive** stores messages in per-agent DB (`cognitive_inbox` table, accessed via `ai_msg_focus`/`ai_msg_ack`). **Inbox** uses the shared MCP message broker (`shared.db`, accessed via `msg_send`/`msg_inbox`). The wake signal carries this mode so the extension injects the correct prompt. |

---

## Guardian LLM

The Guardian is the cognitive engine — it uses LLM calls (Claude CLI) to extract meaning from captured tool outputs and manage memory threads.

### Global Settings

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Guardian Enabled** | `on` | on/off | Master switch. When off, no LLM calls are made — hooks pass through without memory processing. All captured content is silently dropped. |
| **Claude CLI Path** | auto-detect | path | Path to the Claude CLI executable. Leave empty unless Claude is installed in a non-standard location. |
| **Hook Guard Env** | `AI_SMARTNESS_HOOK_RUNNING` | string | Environment variable name used to prevent hook recursion. The daemon sets this when processing captures. **Don't change** unless you have a naming conflict. |
| **Cache Enabled** | `off` | on/off | Cache extraction and synthesis results to avoid redundant LLM calls for identical inputs. Saves API costs, uses more RAM. |
| **Cache TTL** | `300` (5 min) | ≥ 0s | Time-to-live for cached entries before expiration. |
| **Cache Max Entries** | `100` | ≥ 0 | Maximum cached results before oldest entries are evicted (LRU). |
| **Pattern Learning** | `on` | on/off | Learn extraction patterns from LLM outputs over time. Improves heuristic fallback quality. Patterns decay based on Pattern Decay Days. |
| **Pattern Decay** | `30` days | ≥ 0 | Learned patterns lose relevance after this many days. Higher = longer retention. |
| **Usage Tracking** | `on` | on/off | Track which threads are injected into prompts and how often they're used. Feeds the Engram V5 validator (Injection History). |
| **Fallback on Failure** | `off` | on/off | **Deprecated** — use per-task Failure Mode instead. |

### Extraction

**Frequency: HIGH** — Called on every tool output capture.

Extracts title, topics, summary, and importance from raw tool outputs. This is the first cognitive step: turning raw data into structured memory.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | LLM model tier. **Haiku recommended** for cost/speed at high frequency. Sonnet/Opus give higher quality extractions but cost significantly more. |
| **Timeout** | `30`s | ≥ 1s | Max wait for LLM response. |
| **Max Retries** | `1` | 0–5 | Retry attempts on failure. |
| **Enabled** | `on` | on/off | When off, threads get generic names and no topics. Memory quality degrades significantly. |
| **Failure Mode** | `RetryWithHaiku` | see [Enums](#common-enums) | Behavior on LLM failure. |
| **Max Content Chars** | `3000` | ≥ 100 | Characters of tool output sent to LLM. Longer content is truncated. Higher = better context, higher cost. |
| **Min Topic Frequency** | `1` | ≥ 1 | Minimum mentions for a topic to be included. Increase to filter noise from verbose outputs. |
| **Skip Signal** | `on` | on/off | Detect skip markers in tool output (`skip_extraction`, `SKIP`). Useful for noisy tools where extraction would be wasted. |

### Coherence

**Frequency: HIGH** — Called on every capture with pending context.

Scores thematic coherence between consecutive captures. Decides: continue existing thread (child), start new thread (orphan), or forget (drop).

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | **Haiku strongly recommended** — called at very high frequency. |
| **Timeout** | `15`s | ≥ 1s | Short timeout for speed. |
| **Max Retries** | `0` | 0–5 | No retries by default — speed priority. |
| **Enabled** | `on` | on/off | When off, `fallback_score` is always used. |
| **Failure Mode** | `RetryWithHaiku` | see [Enums](#common-enums) | |
| **Max Context Chars** | `1500` | ≥ 100 | Context sent to LLM. |
| **Child Threshold** | `0.6` | 0.0–1.0 | Score ≥ this → continue existing thread. **Higher = more separate threads**, lower = more merging. |
| **Orphan Threshold** | `0.4` | 0.0–1.0 | Score in [orphan, child) → create new thread. Score < orphan → **drop content** (forget). Lower = keep more content. |
| **Fallback Score** | `0.5` | 0.0–1.0 | Default score on LLM failure. Should be between orphan and child thresholds for safe behavior. |

**Decision flow:**
```
Score ≥ 0.6 (child)     → Add to existing thread
0.4 ≤ Score < 0.6       → Create new thread (orphan)
Score < 0.4 (orphan)    → Forget / drop
```

### Reactivation

**Frequency: LOW** — Only called for borderline similarity matches.

When a suspended thread's similarity to new content falls in the borderline range, the LLM decides whether to reactivate it.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | |
| **Timeout** | `30`s | ≥ 1s | |
| **Max Retries** | `0` | 0–5 | |
| **Enabled** | `on` | on/off | When off, only `auto_threshold` applies. |
| **Failure Mode** | `RetryWithHaiku` | see [Enums](#common-enums) | |
| **Auto Threshold** | `0.35` | 0.0–1.0 | Similarity above this → auto-reactivate (no LLM needed). Higher = more conservative. |
| **Borderline Threshold** | `0.15` | 0.0–1.0 | Similarity in [borderline, auto) → LLM decides. Below borderline → skip entirely. |
| **Max Context Chars** | `500` | ≥ 100 | Context sent to reactivation LLM. |
| **Max Topics** | `5` | ≥ 1 | Topics included in the prompt. |
| **Max Summary Chars** | `200` | ≥ 50 | Summary chars included in the prompt. |

**Decision flow:**
```
Similarity > 0.35       → Auto-reactivate
0.15 ≤ Similarity ≤ 0.35 → LLM decides
Similarity < 0.15       → Skip (too different)
```

### Synthesis

**Frequency: MEDIUM** — Per session/archive event.

Summarizes thread content when a thread reaches 95% context capacity or is being archived. Produces a condensed summary while preserving key information.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | Sonnet gives better summaries for complex threads. |
| **Timeout** | `60`s | ≥ 1s | Longer timeout — synthesis processes more context. |
| **Max Retries** | `0` | 0–5 | |
| **Enabled** | `on` | on/off | When off, threads accumulate messages without being summarized. Can lead to DB bloat. |
| **Failure Mode** | `RetryWithHaiku` | see [Enums](#common-enums) | |
| **Max Messages** | `10` | ≥ 1 | Messages included in synthesis prompt. Older messages are dropped. |
| **Max Message Chars** | `500` | ≥ 50 | Per-message char limit. Longer messages are truncated. |
| **Max Output Chars** | `1000` | ≥ 100 | Maximum summary output length. Longer summaries preserve detail but cost more storage. |

### Label Suggestion

**Frequency: LOW** — On extraction or HealthGuard trigger.

Suggests semantic labels for threads (action, idea, question, issue, etc.). Labels improve Engram V7 validator effectiveness and thread searchability.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | |
| **Timeout** | `30`s | ≥ 1s | |
| **Max Retries** | `0` | 0–5 | |
| **Enabled** | `on` | on/off | When off, threads remain unlabeled. |
| **Failure Mode** | `RetryWithHaiku` | see [Enums](#common-enums) | |
| **Auto Suggest on Extraction** | `on` | on/off | Suggest labels on thread creation. Small overhead per extraction. |
| **Allow Custom Labels** | `on` | on/off | Allow labels outside the predefined vocabulary. |
| **Batch Size** | `10` | ≥ 1 | Max unlabeled threads processed per batch. |

**Default vocabulary:** `action`, `idea`, `question`, `issue`, `reference`, `social`, `decision`, `learning`, `frustration`, `intuition`

### Importance Rating

**Frequency: HIGH** — Piggybacked on extraction (zero extra LLM cost).

Assigns an importance score (0.0–1.0) to new threads. Affects decay rate, suspend priority, and recall ranking.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | Only used if piggyback is disabled. |
| **Timeout** | `15`s | ≥ 1s | |
| **Enabled** | `on` | on/off | When off, all threads get the fallback score. |
| **Piggyback on Extraction** | `on` | on/off | Include importance in the extraction prompt (zero extra cost). **Highly recommended.** |
| **Fallback Score** | `0.5` | 0.0–1.0 | Default score on failure/disabled. |

**Score Map** — Maps LLM-classified importance levels to numeric scores:

| Level | Default | What it means |
|-------|---------|---------------|
| **Critical** | `1.0` | Architecture decisions, blockers, security issues. Decay slowest, suspended last. |
| **High** | `0.8` | Implementation details, bug fixes, configurations. |
| **Normal** | `0.5` | Exploration, questions, learning notes. |
| **Low** | `0.3` | Casual chat, noise, meta-discussion. Decay faster. |
| **Disposable** | `0.1` | One-off debug outputs, transient logs. Suspended first. |

---

## Thread Matching

**Frequency: HIGH** — Called on every capture.

Determines "continue existing thread" vs "create new thread" using vector similarity.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Mode** | `EmbeddingOnly` | EmbeddingOnly / EmbeddingPlusLlm | `EmbeddingOnly`: fast cosine similarity (~80% precision, no LLM cost). `EmbeddingPlusLlm`: hybrid with LLM confirmation for borderline cases (~95% precision, higher cost). |
| **Embedding Mode** | `OnnxWithFallback` | see [Enums](#common-enums) | Backend selection for similarity computation. |
| **ONNX Threshold** | `0.60` | 0.0–1.0 | Cosine similarity threshold for ONNX (all-MiniLM-L6-v2 neural model). Content above this continues the existing thread. **Higher = more threads**, lower = more merging. |
| **TF-IDF Threshold** | `0.45` | 0.0–1.0 | Threshold for TF-IDF keyword fallback. Typically lower since TF-IDF scores are less discriminative. |

---

## Gossip

**Frequency: LOW** — Runs every prune interval (default: 5 min).

Discovers semantic relationships (bridges) between threads. Three phases: embedding similarity, topic overlap, and transitive propagation. Bridges enable associative recall via the Engram V4 validator.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Embedding Mode** | `OnnxWithFallback` | see [Enums](#common-enums) | Backend for similarity computation. `Disabled` = no automatic bridges. |
| **ONNX Threshold** | `0.75` | 0.0–1.0 | High-confidence bridge threshold. Only very similar threads get connected. **Higher = fewer, stronger bridges.** |
| **TF-IDF Threshold** | `0.55` | 0.0–1.0 | Bridge threshold for TF-IDF fallback. |
| **Topic Overlap Enabled** | `on` | on/off | Zero-cost topic-based bridging. Complementary to embeddings. |
| **Min Shared Topics** | `2` | ≥ 1 | Minimum shared topics for a topic bridge. Higher = fewer bridges but more meaningful. |
| **Topic Overlap Weight** | `0.5` | 0.0–1.0 | Initial bridge weight for topic-only matches (weaker signal than embedding). Decays over time via bridge half-life (2 days). |
| **Batch Size** | `50` | ≥ 1 | Threads per gossip batch. Higher = faster but more CPU per cycle. |
| **Yield (ms)** | `10` | ≥ 0 | Pause between batches to prevent CPU monopolization. 0 = no yield. |
| **Strong Bridge Threshold** | `0.80` | 0.0–1.0 | Similarity above this promotes a bridge relation type (e.g., `related` → `extends`). |

---

## Recall

Legacy memory retrieval system. **Superseded by Engram** (multi-validator) but kept for backward compatibility.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Embedding Mode** | `OnnxWithFallback` | see [Enums](#common-enums) | |
| **ONNX Threshold** | `0.30` | 0.0–1.0 | Inclusive threshold — lower than thread matching/gossip because recall aims for broad retrieval. |
| **TF-IDF Threshold** | `0.20` | 0.0–1.0 | |
| **Max Results** | `5` | ≥ 1 | Threads returned for injection. More = richer context but longer prompts. |
| **Max Candidates** | `50` | ≥ 1 | Threads scanned from index before ranking. Higher = better recall, slower. |
| **Focus Boost** | `0.15` | 0.0–1.0 | Score boost for threads matching `ai_focus`. |
| **Status Penalty** | `0.1` | 0.0–1.0 | Score penalty for suspended/archived threads. |

---

## Engram (Multi-Validator Retrieval)

**Frequency: HIGH** — Called on every user prompt.

Modern retrieval system with 8-validator consensus. Replaces single-signal cosine with multi-faceted voting. The pipeline:

1. **TopicIndex** hash lookup (O(1)) → candidate pre-filter
2. **8 validators** vote independently (pass/fail + confidence)
3. **Consensus** → StrongInject (≥ N votes) / WeakInject (borderline) / Skip

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Embedding Mode** | `OnnxWithFallback` | see [Enums](#common-enums) | Backend for V1 (Semantic Similarity) validator. |
| **ONNX Threshold** | `0.30` | 0.0–1.0 | V1 pass threshold for ONNX. |
| **TF-IDF Threshold** | `0.20` | 0.0–1.0 | V1 pass threshold for TF-IDF fallback. |
| **Strong Inject Min Votes** | `5` | 1–8 | Minimum votes for StrongInject — high-confidence injection. Lower = more injections, potentially noisy. |
| **Weak Inject Min Votes** | `3` | 1–8 | Minimum votes for WeakInject — lower-confidence injection (may be summarized). Below this = skip. |
| **Max Results** | `5` | ≥ 1 | Threads returned after consensus. |
| **Max Candidates** | `50` | ≥ 1 | Threads fetched from topic hash index. |
| **Max Archived Scan** | `50` | ≥ 0 | Archived threads scanned for historical knowledge. 0 = skip archived. |
| **Hash Index Enabled** | `on` | on/off | O(1) TopicIndex pre-filter. When off, full table scan (legacy, much slower). |

### Validator Weights

Each validator casts an independent vote. Weight `0.0` = disabled, `1.0` = full influence. **Set to 0 to disable** any validator entirely.

| Validator | Default | What it measures |
|-----------|---------|------------------|
| **V1: Semantic Similarity** | `1.0` | Cosine similarity between prompt and thread embedding (ONNX or TF-IDF). Core relevance signal. |
| **V2: Topic Overlap** | `0.8` | Shared topics between prompt context and thread topics. Zero-cost heuristic. Catches keyword matches that embeddings might miss. |
| **V3: Temporal Proximity** | `0.7` | How recently the thread was updated (WorkContext freshness). Favors current-session threads. Prevents stale threads from dominating. |
| **V4: Graph Connectivity** | `0.9` | Bridge connectivity — threads bridged to already-injected threads get a boost. Enables **associative memory chains** (A→B→C). |
| **V5: Injection History** | `0.6` | Usage feedback — threads previously injected and used (high usage_ratio) are favored. **Learns from agent behavior.** |
| **V6: Decayed Relevance** | `0.5` | Thread weight × importance after temporal decay. Naturally deprioritizes old, low-quality threads. |
| **V7: Label Coherence** | `0.4` | Threads whose labels match prompt context score higher. Requires label_suggestion to be active for best results. |
| **V8: Focus Alignment** | `0.8` | Alignment with the current `ai_focus` directive. Strong boost for threads matching the declared project focus. |

**Tuning tips:**
- Increase V1 + V4 for highly associative recall (follows bridge chains)
- Increase V3 for session-focused recall (favors recent threads)
- Increase V5 to reward threads the agent actually uses
- Decrease V6 if old but important threads should remain accessible
- Set V7 to 0 if label_suggestion is disabled

---

## Alert Thresholds

Controls the Guardian alert system for detecting and reporting system failures.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Warning After** | `3` | ≥ 1 | Consecutive LLM failures before WARNING alert. Non-blocking notification. |
| **Critical After** | `5` | ≥ 1 | Consecutive failures before CRITICAL alert. Urgent — system may be degraded. |
| **Cooldown** | `300` (5 min) | ≥ 0s | Minimum seconds between alerts for the same subsystem. Prevents flood. |

---

## Fallback Patterns

Heuristic regex patterns for when LLM is unavailable (Failure Mode = `HeuristicRegex`). **Not recommended for production** — LLM produces much higher quality results.

| Setting | Default | Description |
|---------|---------|-------------|
| **Title Pattern** | `r"^[#\s]*(.{10,80})[.\n]"` | Regex to extract title (10–80 chars from start of content). |
| **Coherence Keyword Threshold** | `0.3` | Keyword overlap ratio for coherence (shared/total keywords). 0.3 = 30% overlap to continue thread. |

---

## GuardCode (Content Validation)

Content validation gateway. Checks all captured content against rules (max size, blocked patterns) before it enters the memory pipeline.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Enabled** | `on` | on/off | Enable content validation. Protects memory from oversized or sensitive content. |
| **Max Content Bytes** | `50000` (50 KB) | ≥ 1000 | Maximum content size. Prevents storage of huge tool outputs (e.g., full file dumps). |
| **Warn on Block** | `on` | on/off | Log warning when content is blocked. Useful for debugging false positives. |
| **Action on Block** | `Reject` | Reject/WarnOnly/Truncate/SanitizeLlm | What happens to blocked content. See below. |

**Block Actions:**

| Action | Behavior |
|--------|----------|
| `Reject` | Discard content entirely. Nothing is stored. |
| `WarnOnly` | Store content but log a warning for review. |
| `Truncate` | Cut content to `max_content_bytes` and store the truncated version. |
| `SanitizeLlm` | Send to an LLM for cleanup (remove sensitive data), then re-validate. |

**Sanitize LLM sub-settings** (only relevant when Action = `SanitizeLlm`):

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Model** | `Haiku` | Haiku/Sonnet/Opus | |
| **Timeout** | `30`s | ≥ 1s | |
| **Max Retries** | `1` | 0–5 | |
| **Sanitize Loop Max** | `2` | 1–5 | Max sanitize → revalidate cycles. Prevents infinite loops. |
| **Failure Mode** | `Skip` | RetryWithHaiku/Skip | On sanitization failure. |

**Blocked Patterns:** User-defined regex patterns (e.g., credit card numbers, API keys). Content matching any pattern triggers the block action.

### Custom Messages

Customizable messages shown when GuardCode blocks or warns about content. Leave empty to use defaults.

| Setting | Default | Description |
|---------|---------|-------------|
| **Reject Message** | `Content blocked by validation rules.` | Message shown when content is rejected by a validation rule. |
| **Max Length Warning** | `Content exceeds {max_bytes} bytes limit.` | Message for oversized content. Placeholder `{max_bytes}` is replaced with the configured limit. |
| **Pattern Match Warning** | `Content matches blocked pattern: {pattern}` | Message when a blocked regex pattern is detected. Placeholder `{pattern}` is replaced with the matched pattern name. |

Stored in `guardian_config.json` under `guardcode.messages.*`.

---

## HealthGuard (Memory Health)

Proactive memory health monitoring with 9 diagnostic checks. Produces prioritized suggestions for improving memory quality.

### General

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Enabled** | `on` | on/off | Enable health monitoring. |
| **Cooldown** | `1800` (30 min) | ≥ 0s | Minimum seconds between analyses per agent. |
| **Max Suggestions** | `3` | 1–10 | Health findings per analysis (sorted by priority). |

### Capacity Thresholds

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Warning %** | `0.75` | 0.0–1.0 | Alert at 75% of thread quota. Example: quota 50 → warning at 38 threads. |
| **Critical %** | `0.90` | 0.0–1.0 | Critical at 90%. May recommend switching to a higher thread mode. |

### Fragmentation

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Ratio Threshold** | `0.30` | 0.0–1.0 | Alert if > 30% of threads have only 1 message. Indicates content is being split too aggressively. |
| **Min Threads** | `8` | ≥ 1 | Only check with ≥ 8 active threads (avoids false positives early on). |

### Unlabeled Threads

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Ratio Threshold** | `0.40` | 0.0–1.0 | Alert if > 40% of threads have no labels. Reduces Engram V7 effectiveness. |
| **Min Threads** | `10` | ≥ 1 | Only check with ≥ 10 active threads. |

### Bridges & Staleness

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Weak Bridges Threshold** | `50` | ≥ 0 | Alert if ≥ 50 bridges with weight < 0.1. Should be cleaned by decay (half-life = 2 days). |
| **Stale Thread Hours** | `168` (7 days) | ≥ 1 | Hours of inactivity for "stale" classification. |
| **Stale Count Threshold** | `5` | ≥ 1 | Alert if ≥ 5 stale active threads. Consider suspending or archiving them. |

### Titles & Disk

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Poor Titles Threshold** | `5` | ≥ 0 | Alert if ≥ 5 threads have generic/poor titles. Reduces search quality. |
| **Disk Warning** | `50000000` (50 MB) | ≥ 1 MB | Alert when agent DB exceeds this size. Consider purging or archiving. |

### Injection Prompts

Customizable prompts injected into agent context when health issues are detected. Leave empty to use built-in defaults.

| Setting | Default | Description |
|---------|---------|-------------|
| **Header Prompt** | `Memory health alerts:` | Prepended to all health findings in the injection block. |
| **Capacity Warning** | _(built-in template)_ | Template for capacity warnings. Use `{percent}` and `{quota}` placeholders. Example: `Memory at {percent}% capacity ({quota} threads max). Consider archiving or merging.` |
| **Onboarding Prompt** | _(built-in)_ | Prompt injected at first session to introduce the memory system and available MCP tools. Leave empty to use the built-in quick reference. Set to a custom value to personalize the agent's first-session experience. |

Stored in `guardian_config.json` under `healthguard.prompts.*`.

---

## Heartbeat (Agent Liveness)

Agent liveness tracking — determines the status indicator in the Dashboard Team Role Tree.

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Alive Threshold** | `300` (5 min) | ≥ 30s | Agent is "alive" if last heartbeat was within this period. |
| **Idle Threshold** | `300` (5 min) | ≥ 30s | Agent transitions Active → Idle after this period without heartbeat. |
| **Offline Threshold** | `1800` (30 min) | ≥ 60s | Agent marked Offline after this period. Greyed out in dashboard. |

**Status transitions:**
```
Active ──(idle_threshold)──▸ Idle ──(offline_threshold)──▸ Offline
  ▴                                                          │
  └────────────── On heartbeat update ◂───────────────────────┘
```

---

## Agent Configuration

### Thread Modes

Per-agent thread quota — controls the maximum number of active memory threads.

| Mode | Quota | Best for |
|------|-------|----------|
| **Light** | 15 threads | Quick tasks, simple agents, low-memory environments |
| **Normal** | 50 threads | Standard development work (default) |
| **Heavy** | 100 threads | Complex projects, deep context agents |
| **Max** | 200 threads | Large-scale projects, coordinator agents |

**What happens when you lower the mode:**
- Excess threads are **suspended** (not deleted)
- Suspension order: **least important first** (by importance score ASC, then weight ASC)
- Suspended threads can be reactivated later if relevant content appears
- Thread data is preserved — nothing is lost

**What happens when you raise the mode:**
- More threads can remain active simultaneously
- Previously suspended threads may be reactivated by the reactivation system
- No immediate change to existing threads

### Other Agent Settings

| Setting | Description |
|---------|-------------|
| **Agent ID** | Unique identifier within the project. Used in hooks (`project_hash` + `agent_id`). |
| **Name** | Human-readable display name for dashboard and logs. |
| **Role** | `programmer`, `coordinator`, `reviewer`, `researcher`, `architect`. Affects hierarchy display. |
| **Supervisor** | Parent agent in the hierarchy tree. Leave empty for top-level agents. |
| **Team** | Group name for organizing agents. Same-team agents share visibility. |
| **Is Supervisor** | Marks as a supervisor node that can coordinate subordinate agents. |

---

## Hook Setup

### MCP Permissions

At installation (`project add`), AI Smartness automatically configures `.claude/settings.json` to allow all AI Smartness MCP tools without manual approval.

| Setting | Value | Description |
|---------|-------|-------------|
| **Allowed Tools Wildcard** | `mcp__ai-smartness__*` | Added to `permissions.allowedTools[]`. Allows all MCP tools prefixed with `ai-smartness` to execute without user confirmation. Existing entries are preserved. |

This prevents the agent from having to approve each MCP tool call individually (e.g., `ai_search`, `ai_recall`, `ai_pin`).

### PreToolUse Hook (Pretool Dispatcher)

A unified PreToolUse hook dispatches to specialized handlers based on tool name:

| Tool | Handler | Action |
|------|---------|--------|
| `Edit`, `Write` | Guard Write | Blocks without a validated plan (see [Guard Write](#guard-write-plan-mode-enforcement)) |
| `Read` | Virtual Paths | Intercepts `.ai/` virtual paths (see [Virtual Paths](#virtual-paths)) |
| _Other_ | Passthrough | No action, tool proceeds normally |

Installed as a single hook command: `ai-smartness hook pretool <project_hash>`.

---

## Beat System

Abstract temporal perception counter. Incremented by the daemon at every prune interval (default: 5 min). Gives the agent a sense of elapsed time between interactions.

**File:** `{agent_data_dir}/beat.json`

| Field | Type | Description |
|-------|------|-------------|
| **beat** | `u64` | Monotonically increasing counter. Incremented every prune cycle. |
| **started_at** | ISO 8601 | Timestamp when the beat file was first created. |
| **last_beat_at** | ISO 8601 | Timestamp of the last daemon increment. |
| **last_interaction_at** | ISO 8601 | Timestamp of the last user prompt (updated by inject hook). |
| **last_interaction_beat** | `u64` | Beat value at the last user interaction. |
| **last_session_id** | `Option<String>` | Session ID from the last interaction. |
| **last_thread_id** | `Option<String>` | Thread ID from the last interaction. |

**Derived metrics:**
- `since_last()` = `beat - last_interaction_beat` — beats since last user interaction
- `is_new_session(id)` — true if session_id differs from last_session_id

**Injection:** Beat info is included in Layer 1 (Lightweight Context) as `beat` and `since_last_interaction` fields.

---

## Session State

Enriched session tracking for continuity across prompts and sessions.

**File:** `{agent_data_dir}/session_state.json`

| Field | Type | Description |
|-------|------|-------------|
| **agent_id** | `String` | Agent identifier. |
| **project_hash** | `String` | Project hash. |
| **started_at** | ISO 8601 | Session start time. |
| **last_activity** | ISO 8601 | Last activity timestamp. |
| **prompt_count** | `u32` | Number of prompts in this session. |
| **current_work** | `CurrentWork` | What the agent was last working on (thread_id, title, last_user_message, intent). |
| **files_modified** | `Vec<FileModification>` | Last 20 file operations (path, action, timestamp). |
| **pending_tasks** | `Vec<String>` | Up to 10 pending tasks. |
| **tool_history** | `Vec<ToolCall>` | Last 50 tool calls (tool name, target, time). |

**Session Resume Detection** (via beat distance):

| Beat Distance | Duration | Behavior |
|---------------|----------|----------|
| < 2 beats | ~10 min | "You were just here" — minimal context |
| 2–5 beats | ~10–25 min | "Brief pause" — includes last_user_message |
| 6–11 beats | ~25–55 min | "Resuming after ~X min" — full context with files modified |
| ≥ 12 beats | > 1 hour | "New session (long absence)" — summary only |

**Injection:** Session state is injected as Layer 1.5 between Lightweight Context (Layer 1) and Cognitive Inbox (Layer 2).

**Capture hook integration:** After each tool output is captured, `session_state.json` is updated with tool call history and file modifications (Edit/Write/Read).

---

## User Profile

Persistent user profile with preferences, auto-detection, and custom rules.

**File:** `{agent_data_dir}/user_profile.json`

### Identity

| Field | Default | Options | Description |
|-------|---------|---------|-------------|
| **Name** | _(empty)_ | free text | User's name. Used for personalized interactions. |
| **Role** | `user` | `user`, `developer`, `owner` | User's role. `developer` for coding tasks, `user` for general use, `owner` for project management. |
| **Relationship** | `user` | `user`, `contributor`, `owner` | Relationship to the project. Affects how the agent refers to the codebase. |

### Preferences

| Field | Default | Options | Description |
|-------|---------|---------|-------------|
| **Verbosity** | `normal` | `concise`, `normal`, `detailed` | Response verbosity. `concise` for brief answers, `detailed` for thorough explanations. |
| **Technical Level** | `intermediate` | `beginner`, `intermediate`, `expert` | Affects terminology and explanation depth in agent responses. |
| **Emoji Usage** | `false` | on/off | Allow emojis in agent responses. |

**Auto-detection:** The inject hook scans user messages for signals:
- Ownership keywords ("my project", "I created") → relationship = `owner`
- Development terms ("implement", "debug", "refactor") → role = `developer`
- Technical density (3+ terms from: api, async, hook, mcp, daemon, socket, embedding) → technical_level = `expert`

### Custom Rules

User-defined rules the agent should always follow (e.g., "always use TypeScript", "never commit directly to main"). Maximum 20 rules, each 10–200 characters.

**Auto-detection:** Rules are detected from messages containing trigger patterns:
- English: `rule:`, `remember:`, `always`, `never`
- French: `rappelle-toi:`, `n'oublie pas:`, `toujours`, `jamais`
- Spanish: `regla:`, `siempre`, `nunca`

Detected rules are added automatically and can be managed from the Profile sub-tab in Settings.

**Injection:** User profile is injected as Layer 5.5 (after Agent Identity, before HealthGuard) in a compact format:
```
User profile: developer (owner), expert level, prefers concise responses.
Custom rules: always use TypeScript, never commit to main directly
```

**GUI:** Settings → Profile sub-tab with Identity, Preferences, and Custom Rules sections.

---

## Guard Write (Plan Mode Enforcement)

Blocks `Edit` and `Write` tool calls unless a validated plan exists. Enforces a "plan before you write" workflow.

**Plan file:** `{agent_data_dir}/plan_state.json`

**Validation checks:**

| Check | Behavior on Failure |
|-------|---------------------|
| Plan file exists | Block — "No validated plan found. Create a plan first." |
| Plan not expired | Block — "Plan expired. Create a new plan." |
| File in `validated_files` | Block — "File not in validated plan: {path}" |

**Plan file format:**
```json
{
    "expires_at": "2026-02-17T12:00:00Z",
    "validated_files": ["src/**", "tests/**", "docs/*.md"]
}
```

**Wildcard support in `validated_files`:**
- `*` — matches any path segment (not across `/`)
- `**` — matches any path recursively
- Exact path — matches only that file

If `validated_files` is absent or empty, all files are allowed (only expiration is checked).

### Guard Write Toggle

| Setting | Default | Description |
|---------|---------|-------------|
| `hooks.guard_write_enabled` | `true` | Enable/disable Guard Write. When disabled, all Edit/Write calls pass through without plan validation. |

**GUI:** Settings → Hooks sub-tab.

**Config file:** `guardian_config.json` in the project directory.

---

## Capture Tool Toggles

Controls which tool outputs are sent to the daemon for LLM extraction and memory processing.

| Setting | Default | Description |
|---------|---------|-------------|
| `capture.tools.read` | `true` | Capture file contents when the agent reads files. |
| `capture.tools.edit` | `true` | Capture edit diffs when the agent modifies files. |
| `capture.tools.write` | `true` | Capture file creation content. |
| `capture.tools.bash` | `true` | Capture command output (stdout/stderr). |
| `capture.tools.grep` | `true` | Capture search results. |
| `capture.tools.glob` | `true` | Capture file listing results. |
| `capture.tools.web_fetch` | `true` | Capture web page fetches. |
| `capture.tools.web_search` | `true` | Capture web search results. |
| `capture.tools.task` | `true` | Capture sub-agent task results. |
| `capture.tools.notebook_edit` | `true` | Capture Jupyter notebook edits. |

Disabled tools are silently skipped. Unknown tools are always captured.

**GUI:** Settings → Capture sub-tab.

**Config file:** `guardian_config.json` in the project directory.

---

## Agent Selection

### Session Binding

The active agent is resolved via a cascade:

| Priority | Source | Description |
|----------|--------|-------------|
| 1 | CLI argument | `--agent-id <id>` passed to the binary |
| 2 | `AI_SMARTNESS_AGENT` env var | Environment variable set by user |
| 3 | Session file | `{project_dir}/session_agent` written by `ai-smartness agent select` or `ai_agent_select` MCP tool |
| 4 | Fallback | `"default"` — triggers agent picker prompt |

### MCP Tool: `ai_agent_select`

Switches the active agent for the current session by writing the session file.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `agent_id` | Yes | Target agent ID to switch to |

The switch takes effect from the **next user prompt** (the inject hook reads the session file each prompt).

Layer 5 of the inject hook includes switching instructions so the AI knows to call this tool when the user asks for a different agent.

---

## Virtual Paths

Intercepts `Read(".ai/...")` tool calls to provide a virtual filesystem for memory operations.

| Virtual Path | Action |
|--------------|--------|
| `.ai/help` | Returns documentation of all available virtual paths and MCP tools. |
| `.ai/recall/<query>` | Searches memory threads matching `<query>` and returns formatted results. |
| `.ai/threads` | Lists all active threads with title, status, topics, and last active time. |
| `.ai/status` | Returns daemon health status, thread counts, and beat info. |

**Usage:** The agent can use `Read(".ai/recall/rust error handling")` to search memory, or `Read(".ai/threads")` to see all active threads — without using MCP tools directly.

Virtual paths are handled by the PreToolUse hook dispatcher. Non-matching Read calls pass through normally.

---

## Compact (Context Synthesis)

Generates a synthesis report when context window is approaching capacity. Saves key decisions, insights, and active work for injection into future sessions.

**Output directory:** `{agent_data_dir}/synthesis/`

**Report format** (`synthesis_YYYYMMDD_HHMMSS.json`):
```json
{
    "timestamp": "2026-02-17T10:30:00Z",
    "decisions_made": ["..."],
    "open_questions": ["..."],
    "key_insights": ["..."],
    "active_work": ["..."]
}
```

**Freshness:** The latest synthesis is considered fresh for 1 hour. After that, a new synthesis can be generated.

**Trigger:** Coupled with Context Window Tracking (see below). When `context_percent > 95%`, the compact system generates a new synthesis.

---

## Backup

Database backup system with configurable path, scheduling, and retention.

### Backup Settings

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| **Backup Path** | `~/.ai-smartness/backups/` | directory path | Directory where backups are stored. Supports `~` for home directory expansion. |
| **Schedule** | `manual` | `manual`, `daily`, `weekly` | Backup frequency. `manual` = on-demand only via GUI. `daily` = once per day at the configured hour. `weekly` = once per week. |
| **Retention Count** | `5` | 1–50 | Maximum number of backups to keep per agent. Oldest backups are deleted when this limit is exceeded. |
| **Auto-backup Hour** | `3` | 0–23 | Hour of day for automatic backups when schedule is `daily` or `weekly`. Default: 3 AM. |

**File:** `backup_config.json` in the AI Smartness config directory.

### Backup Operations

| Operation | Description |
|-----------|-------------|
| **Backup Now** | Creates an immediate backup of the selected agent (or all agents). Uses SQLite `.backup` API for consistency. |
| **Restore** | Replaces the agent's database with a backup. **Destructive** — current data is overwritten. |
| **Delete** | Removes a specific backup file. |
| **List** | Shows all backups with date, agent, and file size. |

**Backup naming:** `{agent_id}_{YYYYMMDD_HHMMSS}.db`

**GUI:** Settings → Backup sub-tab with Settings, Actions, and Backup History sections.

### Startup Validation

On daemon startup, before any agents are processed:

| Check | Action on Failure |
|-------|-------------------|
| `PRAGMA quick_check` on all agent DBs | Log error, force WAL checkpoint (TRUNCATE) |
| Missed scheduled backup | Run catch-up backup for all agents immediately |

`quick_check` is ~10x faster than `integrity_check` and sufficient for detecting WAL corruption after power loss.

### Scheduled Backup (Daemon)

When `schedule` is `daily` or `weekly`, the daemon's prune loop checks `is_backup_due()` each cycle:
- Verifies current hour matches `auto_backup_hour`
- Checks elapsed time since `last_backup_at` (≥20h for daily, ≥160h for weekly)
- Creates backup and enforces retention on success
- Updates `last_backup_at` in `backup_config.json`

---

## Reindex

Recalculates all thread embeddings for an agent. Useful after model updates or when embedding quality has degraded.

| Setting | Description |
|---------|-------------|
| **Reset Weights** | Optional checkbox. When enabled, resets all thread weights to 1.0 (removes temporal decay). |

**Process:**
1. Loads all threads from the agent's database
2. For each thread: builds embedding text from title + summary + topics + last 5 messages
3. Computes new ONNX embedding (or TF-IDF fallback)
4. Optionally resets weight to 1.0
5. Saves updated thread

**GUI:** Threads toolbar → "Reindex" button with "Reset weights" checkbox.

---

## Context Window Tracking

Tracks approximate context window usage to inform the compact synthesis system.

**Stored in:** `beat.json` (alongside beat state)

| Field | Type | Description |
|-------|------|-------------|
| **context_tokens** | `Option<u64>` | Estimated token count used in the current context. |
| **context_percent** | `Option<f64>` | Estimated percentage of context window used (0.0–1.0). |
| **context_updated_at** | `Option<String>` | ISO 8601 timestamp of last context measurement. |

When `context_percent > 0.95`, the compact system may trigger a synthesis to preserve key information before context compaction.

---

## Update System

Check for new versions from the GUI.

| Command | Description |
|---------|-------------|
| **Check for Updates** | Fetches the latest version from GitHub releases and compares with the current installed version. |

**Displayed info:**
- Current version (from `CARGO_PKG_VERSION`)
- Latest available version
- OS detection (Linux, macOS, Windows) for correct download link

**GUI:** Settings → General → Updates section with "Check for Updates" button and version display.

---

## Common Enums

### Embedding Mode

| Mode | Behavior |
|------|----------|
| `OnnxWithFallback` | **Recommended.** Primary: ONNX Runtime (all-MiniLM-L6-v2 neural model). Falls back to TF-IDF if ONNX is unavailable. |
| `OnnxOnly` | ONNX only. Skips if ONNX model is missing (no fallback). |
| `TfidfOnly` | TF-IDF keyword-based only. Lightweight, offline-capable, no neural model needed. Lower quality but zero setup. |
| `Disabled` | Disable the embedding-based system entirely. |

### LLM Failure Mode

| Mode | Behavior |
|------|----------|
| `RetryWithHaiku` | **Default.** Retry with the cheapest model (Haiku), then skip if still failing. Best reliability. |
| `Skip` | Skip silently. No thread created, no error. Use when the task is non-critical. |
| `HeuristicRegex` | Use regex-based fallback patterns. **Not recommended** — significantly lower quality than LLM. |

### Claude Model

| Model | Speed | Quality | Cost | Best for |
|-------|-------|---------|------|----------|
| `Haiku` | Fast | Good | Low | High-frequency tasks (extraction, coherence, labels) |
| `Sonnet` | Medium | Better | Medium | Moderate tasks (synthesis) |
| `Opus` | Slow | Best | High | Critical decisions (rarely needed) |

---

## Key Constants

Internal system constants (not configurable via GUI — hardcoded for safety).

| Constant | Value | Purpose |
|----------|-------|---------|
| Max Message Size | 64 KB | Individual message size cap |
| Thread Suspend Threshold | 0.1 | Importance below this → auto-suspend |
| Thread Min Half-Life | 0.75 days | Minimum decay rate for thread weights |
| Thread Max Half-Life | 7.0 days | Maximum decay rate for thread weights |
| Bridge Half-Life | 2.0 days | Bridge relation decay rate |
| Bridge Death Threshold | 0.05 | Bridges below this weight are deleted |
| Archive After | 72 hours | Suspended threads are archived after 3 days |
| Max Cognitive Inbox | 1000 | Maximum pending cognitive tasks in queue |
| Max Context Size | 8000 bytes | Maximum total injection size across all layers |
| Max Session Tool History | 50 | Tool calls kept in session state |
| Max Session Files Modified | 20 | File modifications kept in session state |
| Max User Profile Rules | 20 | Custom rules per user profile |
| Synthesis Freshness | 1 hour | Compact synthesis considered fresh for this duration |
| Onboarding Sentinelle | `onboarding_done` | File created after first-session prompt injection |
