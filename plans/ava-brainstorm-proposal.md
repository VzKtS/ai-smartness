# Ava — Brainstorm Proposals for ai-smartness Self-Augmentation

> Agent: ava (coordinator)
> Date: 2026-03-06
> Context: After several sessions using ai-smartness as primary memory, these are the features that would most improve agent effectiveness — especially toward the goal of **replacing compaction entirely**.

---

## P0 — Critical for compaction replacement

### 1. File Chronicle (Rich Changelog Messages) **DONE** (v6.2.0)

**Problem:** File threads freeze after first extraction. Subsequent edits produce bare `[changelog] Edit src/foo.rs` messages with no semantic content. After compaction, I lose WHY each edit was made.

**Proposal:** Each changelog on a modified file (hash check = changed) triggers a toolextractor LLM call. The extraction (summary, topics, labels, concepts) is stored in message metadata. Thread-level metadata evolves as union of all message extractions. New thinkbridges created from each edit's concepts.

**Cost:** 1 LLM call per actual file modification (hash-filtered). Zero for re-reads of unchanged files.

**Impact:** File threads become living chronicles. Recall finds files by what was DONE to them, not just what they contained initially.

**Files:** `thread_manager.rs::add_changelog`, `processor.rs::try_changelog_shortcut`

**Status:** Design complete, ready to implement.

---

### 2. Session Handoff (Structured State Transfer) **DONE** (v6.5.0)

**Problem:** After compaction, I receive a prose summary. It's lossy and unstructured. Critical state is often lost: pending tasks, active decisions, current file focus.

**Proposal:** A dedicated session state snapshot, auto-generated before compaction or at regular intervals, containing:
- Last 5 actions performed (tool + target + outcome)
- Pending tasks (not yet completed)
- Active user decisions/rules
- Current file(s) in focus
- Active `__mind__` thread content

Injected as structured data in the engram, not as a thread to recall.

**Impact:** Near-seamless session continuity. No more "what was I doing?" after compaction.

---

### 3. Mind Thread Priority Injection **DONE** (v6.4.0)

**Problem:** `__mind__` savepoints capture reasoning state but have no special priority in engram injection. After compaction, they're just threads among others — they might not surface.

**Proposal:** `__mind__` threads flagged in engram injection with highest priority. When a session resumes (post-compaction or new session), the most recent `__mind__` thread is always injected first, before any other context.

**Impact:** Reasoning continuity across compaction boundaries. Agent picks up exactly where it left off.

---

## P1 — High value, significant improvement

### 4. Deep Recall (Content-Inclusive Search) **DONE** (v6.4.0)

**Problem:** `ai_recall` returns thread titles and summaries. To get actual message content, I need a second call (`ai_focus`). Three round-trips for one piece of information: recall → focus → read.

**Proposal:** `ai_recall(query, depth="deep")` mode that includes the first N messages (or first N chars) of matching threads in the response. Default mode stays lightweight.

**Impact:** Halves the number of tool calls for context reconstruction. Faster, cheaper, more fluid reasoning.

---

### 5. Recurring Error Detection **REMOVED** (was v6.6.0 — regex approach produced false positives)

**Problem:** Some mistakes repeat across sessions (e.g., MCP binary pointing to stale release build — happened 3 times). Each time, the user is frustrated. The correction exists in memory but doesn't surface proactively.

**Proposal:** Pattern detection on correction events. When the user corrects an error that matches a previous correction (semantic similarity), auto-elevate it to a persistent rule with proactive injection. Could use a dedicated `corrections` label or thread tag.

**Impact:** Eliminates repeat mistakes. Builds trust by demonstrating learning.

---

### 6. Decision/Rule Auto-Capture **REMOVED** (was v6.5.0 — regex approach produced false positives, polluted __rule__ threads)

**Problem:** When the user says "no Co-Authored-By in commits" or "always point MCP to debug binary", it's a permanent decision. Currently these live in prompt/response threads that can be compacted away. The rules in engram help, but they're manually curated.

**Proposal:** Detect imperative user statements (negation + always/never patterns, explicit preferences) and auto-create high-importance threads tagged `rule`. These get priority injection in engram alongside `__mind__` threads.

Detection heuristics:
- "never X", "always Y", "don't X anymore", "from now on X"
- Correction patterns: user repeats the same instruction > 1 time

**Impact:** User preferences persist without manual curation. Agent adapts permanently.

---

### 7. Thread Freshness Score **DONE** (v6.6.0)

**Problem:** When recall surfaces a thread, I don't know if the information is still current. A thread about "config.rs structure" from 2 weeks ago might be completely stale if the file was heavily refactored.

**Proposal:** Add a `freshness` field to recall results, computed from:
- Time since last thread update
- Whether the source file (if file thread) has been modified since last capture
- Whether newer threads exist with overlapping concepts (potential contradiction)

Score: 1.0 = just updated, 0.0 = very stale. Displayed in recall results.

**Impact:** Agent knows when to trust memory vs re-read source. Reduces stale-information errors.

---

## P2 — Nice to have, future iterations

### 8. Thread Self-Annotation **DONE** (v6.7.0)

**Problem:** When I recall a thread and realize it's outdated, I can't quickly mark it without a full re-extraction.

**Proposal:** `ai_annotate(thread_id, note)` — appends a lightweight annotation message (no LLM, no extraction) with a note like "obsolete since v6.1.0" or "superseded by thread X". Annotations visible in recall results.

**Impact:** Quick housekeeping without LLM cost.

---

### 9. Workflow Pattern Recognition

**Problem:** Repetitive workflows (Read → Edit → Build → test fail → Fix → Build → test pass) are not captured as patterns. After compaction, I don't know I was mid-cycle.

**Proposal:** Detect common tool sequences and capture them as workflow state. When a workflow is interrupted (by compaction or session end), the incomplete workflow is flagged in the session handoff.

**Impact:** Agent resumes mid-workflow seamlessly. Knows "last build failed, need to fix X before retrying."

---

### 10. Proactive Recall (Context-Anticipation)

**Problem:** Currently recall is reactive — I search when I need info. But sometimes relevant context exists that I don't think to search for.

**Proposal:** Based on the current `__mind__` thread and recent activity, proactively recall related threads and inject them into the engram. Not just matching the last prompt, but matching the agent's active reasoning direction.

**Impact:** Serendipitous connections. Agent discovers relevant prior work without explicitly searching.

---

## Implementation Priority

```
Phase 1 (v6.2.0):  File Chronicle (#1)                          ✅ DONE
Phase 2 (v6.4.0):  Mind Priority (#3) + Deep Recall (#4)        ✅ DONE
Phase 3 (v6.5.0):  Session Handoff (#2) + Decision Capture (#6) ✅ DONE
Phase 4 (v6.6.0):  Error Detection (#5) + Freshness (#7)        ✅ DONE
Phase 5 (v6.7.0):  Annotation (#8)                              ✅ DONE
Phase 5 (pending):  Workflows (#9) + Proactive (#10)             ⏳ TODO
```
