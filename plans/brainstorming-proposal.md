# Proposition technique — 10 micro-tiers inter-agents

**Auteur:** dev (étape 1 / workflow K)
**Date:** 2026-02-25
**Source:** brainstorming-inter-agents.md (48+ propositions, 7/7 agents)
**Review arc:** 2026-02-25 — 3 corrections (C1-C3), 4 alertes archi (A1-A4), réordonnancement sprints
**Statut:** Post-review arc — corrections intégrées, prêt pour sub+pub (étape 3)

---

## Tier 1 — Ghost wake fix + rappels proactifs heartbeat (~15 LOC) **DONE**

**Scope:** M2, H4, H5

### 1.1 Ghost wake fix (M2)

**Problème:** Wake signal persiste après ACK → inbox vide → ~10-15% tours gaspillés.

**Analyse:** Le ghost wake fix partiel existe déjà dans `messaging.rs:23-41` (non-interrupt signals ne remplacent pas un signal pending). Mais le signal n'est jamais supprimé après lecture réussie de l'inbox.

**Fichiers impactés:**
- `src/mcp/tools/messaging.rs` — `handle_msg_inbox()` L303-344

**Modifications:**
- Dans `handle_msg_inbox()`, après la boucle `McpMessages::ack()` (L339-341), supprimer le fichier wake signal si l'inbox est maintenant vide :

```rust
// After ack loop, clean up wake signal if inbox is now drained
if McpMessages::inbox(ctx.shared_conn, &effective_agent)?.is_empty() {
    let signal_path = ai_smartness::storage::path_utils::wake_signal_path(&effective_agent);
    std::fs::remove_file(&signal_path).ok();
}
```

**Estimation:** ~5 LOC

**Tests:**
- `test_inbox_drain_removes_wake_signal` — Envoyer message → lire inbox → vérifier que le wake signal file est supprimé
- `test_inbox_partial_keeps_wake_signal` — Envoyer 2 messages, simuler read de 1 seul → signal toujours présent

### 1.2 Rappel proactif mémoire (H4) + rappel ai_help (H5)

**Problème:** Agents "amnésiques" qui n'utilisent jamais `ai_recall`, `ai_share`, `ai_help`.

**Fichiers impactés:**
- `src/hook/inject.rs` — `build_cognitive_nudge()` L857-917

**Modifications:**
Ajouter 2 nouveaux cas dans la chaîne de priorité de `build_cognitive_nudge()` :

```rust
// 5. ai_help reminder (every 100 prompts for new sessions)
if beat_state.prompt_count > 0
    && beat_state.prompt_count % 100 == 0
    && (beat_state.last_nudge_type != "help" || beat_state.last_nudge_beat + cooldown <= beat)
{
    return Some(("help".into(),
        "Reminder: use ai_help to see all available tools. Use ai_share for cross-agent context.".into()));
}

// 6. ai_share reminder (when agent has shared threads subscribed)
// (relies on existing ai_recall nudge at position 1 — already handles recall)
```

**Estimation:** ~10 LOC

**Tests:**
- `test_nudge_help_fires_at_100_prompts` — Simuler beat_state avec prompt_count=100, vérifier nudge type="help"
- `test_nudge_help_cooldown` — Après un nudge help, le prochain ne doit pas refirer avant cooldown

**Dépendances inter-tiers:** Aucune (tier standalone)

---

## Tier 2 — Suivi tâches + threads partagés en heartbeat (~30 LOC) **DONE**

**Scope:** H1, H2

### 2.1 Suivi de tâches dans le heartbeat (H1)

**Problème:** L'agent ne voit pas les tâches déléguées/en cours dans son contexte.

**Fichiers impactés:**
- `src/hook/inject.rs` — `build_lightweight_context()` L442-466

**Modifications (révisé — alerte A1 arc):**

> **A1 arc:** Ne PAS ouvrir registry.db dans le hot path inject. Utiliser le cache heartbeat.

Approche révisée : le heartbeat (10s tick) synchronise les tâches dans `beat.json` ou `session_state.json`. Le hook inject lit depuis le cache fichier.

**Heartbeat** (`src/mcp/server.rs` — `heartbeat_loop()`):
```rust
// After beat.save(), sync tasks into beat.json:
if let Ok(reg_conn) = database::open_connection(&registry_db, ConnectionRole::Daemon) {
    if let Ok(tasks) = AgentTaskStorage::list_tasks_for_agent(&reg_conn, &agent_id, project_hash) {
        let active: Vec<_> = tasks.iter()
            .filter(|t| t.status != TaskStatus::Completed)
            .map(|t| serde_json::json!({"id": t.id, "title": t.title, "status": t.status.as_str(), "from": t.assigned_by}))
            .collect();
        beat.pending_tasks = active;  // New field in BeatState
        beat.save(&data_dir);
    }
}
```

**Inject** (`src/hook/inject.rs` — `build_lightweight_context()`):
```rust
// Read cached tasks from beat.json (no DB hit):
if !beat.pending_tasks.is_empty() {
    ctx["pending_tasks"] = serde_json::Value::Array(beat.pending_tasks.clone());
}
```

**New BeatState field** (`src/storage/beat.rs`):
```rust
pub pending_tasks: Vec<serde_json::Value>,  // Cached from registry by heartbeat
```

**Estimation:** ~10 LOC heartbeat + ~3 LOC inject + ~2 LOC BeatState = ~15 LOC

### 2.2 Threads partagés dans le heartbeat (H2)

**Problème:** L'agent ne sait pas quels threads partagés sont disponibles.

**Fichiers impactés:**
- `src/hook/inject.rs` — `build_lightweight_context()` L442-466

**Modifications:**
Ajouter la liste des threads partagés pertinents :

```rust
// In build_lightweight_context(), after task injection:
if let Ok(shared_conn) = open_connection(&path_utils::shared_db_path(), ConnectionRole::Hook) {
    if let Ok(shared_threads) = SharedStorage::list_subscriptions(&shared_conn, agent_id) {
        if !shared_threads.is_empty() {
            let shared: Vec<_> = shared_threads.iter().take(5).map(|s| {
                serde_json::json!({"id": s.shared_id, "title": s.title, "from": s.owner_agent})
            }).collect();
            ctx["shared_threads"] = serde_json::Value::Array(shared);
        }
    }
}
```

**Estimation:** ~12 LOC

**Note:** Nécessite d'importer `SharedStorage` dans inject.rs.

**Tests:**
- `test_lightweight_context_includes_tasks` — Créer une tâche pour l'agent → vérifier `pending_tasks` dans le JSON
- `test_lightweight_context_includes_shared_threads` — Subscribe à un thread → vérifier `shared_threads` dans le JSON
- `test_lightweight_context_no_tasks_no_key` — Sans tâches → la clé `pending_tasks` ne doit pas exister

**Dépendances inter-tiers:** Aucune

---

## Tier 3 — Broadcast fiable + git branch tracking (~25 LOC) **DONE**

**Scope:** M5, H3

### 3.1 Fix broadcast — delivery à tous les agents (M5)

**Problème:** Broadcast non reçu par certains agents. Le broadcast insère un seul row avec `to_agent='*'`, et `inbox()` le filtre correctement. Le vrai bug est probablement côté wake signals : si un agent a déjà un signal non-ACK, le nouveau signal (non-interrupt) est ignoré.

**Fichiers impactés:**
- `src/mcp/tools/messaging.rs` — `handle_msg_broadcast()` L251-301

**Modifications:**
Changer le wake signal du broadcast en `interrupt: true` pour garantir la delivery :

```rust
// L296: Change from false to true for broadcast wake signals
emit_wake_signal(&agent.id, ctx.agent_id, &msg.subject, "inbox", true);
```

**Alternative (plus robuste):** Insérer un message individuel par agent au lieu d'un seul `to_agent='*'`. Cela permet l'ACK individuel et le suivi par agent. Mais c'est plus invasif (~20 LOC de plus) et peut être fait plus tard.

**Estimation:** ~1 LOC (fix rapide) ou ~20 LOC (insertion individuelle)
**Recommandation:** Fix rapide (interrupt=true) maintenant, refactor insertion individuelle en Tier 5 si besoin.

### 3.2 Git branch tracking dans heartbeat (H3)

**Problème:** L'agent ne voit pas la branche courante ni l'état git.

**Fichiers impactés:**
- `src/hook/inject.rs` — `build_lightweight_context()` L442-466

**Modifications (révisé — alerte A2 arc):**

> **A2 arc:** Ne PAS spawner subprocess git dans le hot path inject. Cacher dans beat.json via heartbeat.

Approche révisée : le heartbeat (10s tick) exécute git et cache dans `beat.json`. Le hook inject lit depuis le cache.

**Heartbeat** (`src/mcp/server.rs` — `heartbeat_loop()`):
```rust
// After task sync, update git info:
fn update_git_info(beat: &mut BeatState) {
    if let Ok(branch) = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"]).output() {
        beat.git_branch = Some(String::from_utf8_lossy(&branch.stdout).trim().to_string());
    }
    beat.git_dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"]).output()
        .map(|o| !o.stdout.is_empty()).unwrap_or(false);
}
update_git_info(&mut beat);
```

**Inject** (`src/hook/inject.rs` — `build_lightweight_context()`):
```rust
// Read cached git info from beat.json (no subprocess):
if let Some(ref branch) = beat.git_branch {
    ctx["git"] = serde_json::json!({"branch": branch, "dirty": beat.git_dirty});
}
```

**New BeatState fields** (`src/storage/beat.rs`):
```rust
pub git_branch: Option<String>,
pub git_dirty: bool,
```

**Estimation:** ~8 LOC heartbeat + ~3 LOC inject + ~2 LOC BeatState = ~13 LOC

**Tests:**
- `test_broadcast_wakes_all_agents` — Broadcast → vérifier wake signal avec interrupt=true pour chaque agent
- `test_git_branch_info_returns_branch` — En contexte git → vérifie que branch_name est non-vide
- `test_git_branch_info_detects_dirty` — Modifier un fichier → vérifie dirty=true

**Dépendances inter-tiers:** Aucune

---

## Tier 4 — Task lifecycle auto (~40 LOC) **DONE**

**Scope:** T1, T7

### 4.1 task_complete structuré (T7)

**Problème:** Pas de tool dédié `task_complete`. Actuellement via `agent_tasks(action="complete", task_id=..., result=...)` qui est non-intuitif.

**Fichiers impactés:**
- `src/mcp/tools/agents.rs` — nouveau handler
- `src/mcp/tools/mod.rs` — route le nouveau tool
- `src/mcp/server.rs` — enregistrer le tool dans `tool_definitions()`

**Nouvelles structures:**
Aucune struct additionnelle — réutilise `AgentTask` existant.

**Nouveau tool MCP:** `task_complete`

```rust
// In agents.rs:
pub fn handle_task_complete(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let task_id = required_str(params, "task_id")?;
    let result = optional_str(params, "result");

    // Update task status
    AgentTaskStorage::update_task_status(
        ctx.registry_conn, &task_id, TaskStatus::Completed, result.as_deref()
    )?;

    // T1: Auto-callback — notify delegator
    if let Some(task) = AgentTaskStorage::get_task(ctx.registry_conn, &task_id, ctx.project_hash)? {
        let subject = format!("Task completed: {}", task.title);
        let content = format!(
            "Agent {} completed task \"{}\".\nResult: {}",
            ctx.agent_id, task.title, result.as_deref().unwrap_or("(no result)")
        );
        // Send message to delegator
        let now = time_utils::now();
        let msg = Message {
            id: id_gen::message_id(),
            from_agent: ctx.agent_id.to_string(),
            to_agent: task.assigned_by.clone(),
            subject: subject.clone(),
            content,
            priority: MessagePriority::Normal,
            status: MessageStatus::Pending,
            created_at: now,
            ttl_expiry: now + chrono::Duration::hours(24),
            read_at: None,
            acked_at: None,
            attachments: vec![],
        };
        McpMessages::send(ctx.shared_conn, &msg).ok();
        emit_wake_signal(&task.assigned_by, ctx.agent_id, &subject, "inbox", false);

        Ok(serde_json::json!({
            "completed": true,
            "task_id": task_id,
            "notified": task.assigned_by,
        }))
    } else {
        Ok(serde_json::json!({"completed": true, "task_id": task_id, "notified": null}))
    }
}
```

**Estimation:** ~35 LOC (handler) + ~5 LOC (routing + tool definition)

**Tool definition à ajouter dans server.rs:**
```json
{
    "name": "task_complete",
    "description": "Mark a delegated task as completed and auto-notify the delegator",
    "inputSchema": {
        "type": "object",
        "properties": {
            "task_id": {"type": "string", "description": "Task ID to complete"},
            "result": {"type": "string", "description": "Completion result/summary"}
        },
        "required": ["task_id"]
    }
}
```

**Tests:**
- `test_task_complete_updates_status` — Créer tâche → task_complete → vérifier status=completed
- `test_task_complete_notifies_delegator` — Créer tâche (assigned_by=cor) → task_complete → vérifier message dans inbox de cor
- `test_task_complete_with_result` — Vérifier que le result est bien stocké
- `test_task_complete_unknown_task` — task_id invalide → résultat avec notified=null (pas d'erreur fatale)

**Dépendances inter-tiers:** Aucune (utilise infra messaging existante)

---

## Tier 5 — Message threading + attachments (~30 LOC) **DONE** (5.1+5.2)

**Scope:** M4, M7, T2

### 5.1 Message threading avec in_reply_to (M4)

**Problème:** Les messages ne forment pas de conversations chaînées. La colonne `reply_to` existe dans la table `mcp_messages` (DB) mais n'est PAS mappée dans le struct `Message` (C3 fix). Le champ doit être ajouté au struct ET mappé dans `mcp_msg_from_row()`.

**Fichiers impactés:**
- `src/mcp/tools/messaging.rs` — `handle_msg_inbox()` L303-344
- `src/storage/mcp_messages.rs` — `inbox()` L71-87

**Modifications:**
1. Inclure `reply_to` dans la réponse JSON de `handle_msg_inbox()` :
```rust
// In handle_msg_inbox(), L314-334, add to the JSON:
if let Some(ref reply_to) = m.reply_to_id {
    obj["in_reply_to"] = serde_json::Value::String(reply_to.clone());
}
```

2. Ajouter le champ `reply_to_id` dans le struct `Message` (ou lire depuis la row) :

**Fichiers impactés additionnels:**
- `src/message.rs` — Ajouter `pub reply_to_id: Option<String>` dans `Message`
- `src/storage/mcp_messages.rs` — Lire `reply_to` depuis la row dans `mcp_msg_from_row()`

```rust
// In message.rs, add to Message struct:
pub reply_to_id: Option<String>,

// In mcp_messages.rs, mcp_msg_from_row():
reply_to_id: row.get("reply_to").ok(),
```

**Estimation:** ~10 LOC

### 5.2 Attachments améliorés dans msg_reply (M7)

**Problème:** Les attachments existent déjà dans msg_reply (L366-375) mais ne sont pas documentés ni exposés dans ai_help.

**État actuel:** Déjà implémenté ! `resolve_attachments()` est appelé dans `handle_msg_reply()` L366-375. Les attachments sont des chemins de fichiers résolus en contenu.

**Action:** Documenter dans ai_help (Tier 9). Pas de code supplémentaire nécessaire.

### 5.3 Task attachments / plan_path (T2)

**Problème:** Les tâches déléguées n'ont pas de champ pour les fichiers attachés ou le plan path.

**Fichiers impactés:**
- `src/agent.rs` — `AgentTask` struct
- `src/registry/tasks.rs` — `create_task()`, `get_task()`
- `src/mcp/tools/agents.rs` — `handle_task_delegate()`
- `src/storage/migrations.rs` — V8 migration

**Modifications:**
1. Ajouter `context_path: Option<String>` à `AgentTask`
2. Migration V8 dans `migrate_registry()` (A3 fix — registry.db, pas agent.db) : `ALTER TABLE agent_tasks ADD COLUMN context_path TEXT;`
3. Dans `handle_task_delegate()`, lire le param `context` (déjà existant) et aussi `plan_path`:
```rust
let context_path = optional_str(params, "plan_path");
// Store in task.context_path
```

**Estimation:** ~15 LOC

**Tests:**
- `test_msg_reply_includes_in_reply_to` — Envoyer msg → reply → lire inbox → vérifier `in_reply_to` dans le JSON
- `test_message_struct_has_reply_to_id` — Vérifier que Message::default a reply_to_id=None
- `test_task_delegate_with_plan_path` — Déléguer avec plan_path → vérifier stocké en DB

**Dépendances inter-tiers:** Aucune

---

## Tier 6 — Context sharing (~35 LOC) **PARTIAL** (6.1 agent_context DONE)

**Scope:** C4, C5, C3

### 6.1 Agent context/handoff (C4)

**Problème:** Pas de résumé de session pour reprise par un autre agent.

**Fichiers impactés:**
- `src/mcp/tools/status.rs` — Nouveau handler `handle_agent_context`
- `src/mcp/tools/mod.rs` — Route
- `src/mcp/server.rs` — Tool definition

**Nouveau tool MCP:** `agent_context`

```rust
pub fn handle_agent_context(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let target = optional_str(params, "agent_id")
        .unwrap_or_else(|| ctx.agent_id.to_string());

    let data_dir = path_utils::agent_data_dir(ctx.project_hash, &target);
    let beat = BeatState::load(&data_dir);
    let session = SessionState::load(&data_dir, &target, ctx.project_hash);

    Ok(serde_json::json!({
        "agent_id": target,
        "beat": beat.beat,
        "prompt_count": beat.prompt_count,
        "tool_call_count": beat.tool_call_count,
        "last_session_id": beat.last_session_id,
        "files_modified": session.files_modified,  // C1 fix: was "recent_files"
        "context_percent": beat.context_percent,
        "model": beat.model,
        "last_error": beat.last_error,
    }))
}
```

**Estimation:** ~15 LOC

### 6.2 ai_recall cross-agent read-only (C5)

**Problème:** Un agent ne peut pas chercher dans les threads partagés d'un autre agent sans souscrire.

**Fichiers impactés:**
- `src/mcp/tools/memory.rs` — `handle_recall()` — Ajouter param `scope`

**Modifications:**
Quand `scope="shared"`, chercher dans les threads partagés (shared.db) au lieu de l'agent DB locale :

```rust
// In handle_recall():
let scope = optional_str(params, "scope").unwrap_or_else(|| "local".into());
if scope == "shared" {
    // C2 fix: use SharedStorage::discover() (not search())
    // SharedStorage::discover() searches by topics and agent_id
    ...
}
```

**Estimation:** ~10 LOC

### 6.3 ai_sync avec delta (C3)

**Problème:** `ai_sync` ne retourne pas ce qui a changé depuis la dernière sync.

**Fichiers impactés:**
- `src/mcp/tools/shared.rs` — `handle_sync()`
- `src/storage/shared.rs` — Ajouter `last_synced_at` tracking

**Modifications:**
Comparer `updated_at` du thread partagé avec `last_synced_at` de la souscription :

```rust
// In handle_sync(), after fetching snapshot:
let delta = if snapshot.updated_at > subscription.last_synced_at {
    "updated"
} else {
    "unchanged"
};
// Return delta info
```

**Estimation:** ~10 LOC

**Tests:**
- `test_agent_context_returns_beat_info` — Vérifier que le JSON contient beat, prompt_count, etc.
- `test_recall_shared_scope` — Partager un thread → recall(scope="shared") → le trouve
- `test_sync_delta_detects_changes` — Modifier un thread partagé → sync → delta="updated"

**Dépendances inter-tiers:** Aucune

---

## Tier 7 — Build workflow standardisé (~30 LOC)

**Scope:** B1, B2, B4

### 7.1 Build result JSON standardisé (B1)

**Problème:** Pas de format standard pour les résultats de build/test.

**Fichiers impactés:**
- `src/agent.rs` ou nouveau fichier `src/build.rs` — Struct `BuildResult`

**Nouvelles structures:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub command: String,       // "cargo build --features gui"
    pub success: bool,
    pub exit_code: i32,
    pub errors: Vec<String>,   // Parsed error lines
    pub warnings: usize,
    pub tests_passed: Option<usize>,
    pub tests_failed: Option<usize>,
    pub duration_ms: u64,
}
```

**Estimation:** ~15 LOC (struct + impl)

### 7.2 Build status broadcasting (B2)

**Problème:** Un agent qui build ne partage pas le résultat.

**Implémentation recommandée:** Pas un nouveau tool MCP, mais une convention : l'agent construit un `BuildResult` JSON et l'envoie via `msg_send` ou `msg_broadcast` avec un subject standardisé `"build-result: {branch}"`.

Le heartbeat ou le hook inject peut détecter les messages de type build-result et les afficher dans le contexte.

**Fichiers impactés:**
- `src/hook/inject.rs` — Nouveau layer ou enrichissement de Layer 1

**Estimation:** ~10 LOC (détection dans inject) — la convention est principalement documentaire.

### 7.3 Notification post-commit auto → doc (B4)

**Problème:** doc ne sait pas quand un commit feat/fix/bump est fait.

**Implémentation recommandée:** Hook `PostToolUse` côté Claude Code (pas Rust). Quand l'outil Bash exécute `git commit` avec un message contenant `feat:`, `fix:`, ou `bump:`, envoyer un message à doc.

**Alternative Rust:** Ajouter la détection dans le heartbeat loop — scanner le git log pour les nouveaux commits depuis le dernier check.

**Fichiers impactés (option heartbeat):**
- `src/mcp/server.rs` — `heartbeat_loop()` — Ajouter git log check

```rust
// In heartbeat_loop(), after beat.save():
fn check_new_commits(agent_id: &str, project_hash: &str) {
    // Compare HEAD with last_known_commit in beat
    // If new commits with feat/fix/bump prefix → msg_send to "doc"
}
```

**Estimation:** ~15 LOC

**Tests:**
- `test_build_result_serialization` — BuildResult → JSON → désérialise correctement
- `test_build_result_broadcast_format` — Vérifier que le message broadcast suit le format standardisé

**Dépendances inter-tiers:** Tier 3 (broadcast fiable) pour B2

---

## Tier 8 — Onboarding/Guardcode + conventions

**Scope:** P1-P5, P6

### 8.1 Conventions plans (P1-P5) — Coût zéro

**Ce sont des conventions de travail, pas du code :**
- P1: Bloc `## Fichiers à lire` en tête de chaque plan
- P2: `SOURCE_OF_TRUTH.md` ou commentaire en tête des .js édités
- P3: Champ `## Prérequis` dans les plans (dépendances inter-plans)
- P4: Template verdict review standardisé (APPROUVÉ/BLOQUÉ + tableau)
- P5: Questions critiques + fichiers clés dans le message de review

**Action:** Ajouter ces conventions dans `CLAUDE.md` ou un fichier `CONVENTIONS.md` dédié.

**Fichiers impactés:**
- `CLAUDE.md` — Section conventions
- Aucun code Rust

**Estimation:** 0 LOC Rust, ~30 lignes de documentation

### 8.2 Guardcode enforcement (P6)

**Problème:** Les nouveaux agents/sessions ne connaissent pas les conventions.

**Fichiers impactés:**
- `src/hook/inject.rs` — `build_onboarding_prompt()` L63-73
- `src/guardcode/` — Enforcer rules

**Modifications:**
Enrichir le prompt d'onboarding avec les conventions de l'équipe :

```rust
// In build_onboarding_prompt():
// Load conventions from project data dir (conventions.json or CLAUDE.md parsing)
// Inject as part of onboarding layer
```

**Estimation:** ~10 LOC

**Tests:**
- `test_onboarding_includes_conventions` — Première session → vérifier que le prompt contient les conventions
- `test_conventions_loaded_from_file` — Fichier conventions.json présent → contenu injecté

**Dépendances inter-tiers:** Aucune

---

## Tier 9 — ai_help mise à jour complète **DONE** (v6.8.0)

**Scope:** J1

### 9.1 Refactoring complet de ai_help

**Problème:** `ai_help` retourne un JSON statique minimaliste (L75-98 dans status.rs). Pas de doc détaillée par tool.

**Fichiers impactés:**
- `src/mcp/tools/status.rs` — `handle_help()` L75-98
- Nouveau fichier: `src/data/help_text.rs` ou `src/mcp/tools/help.rs`

**Modifications:**
Remplacer le JSON statique par une documentation complète :

```rust
pub fn handle_help(
    params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let topic = optional_str(params, "topic");

    match topic.as_deref() {
        Some("memory") => Ok(help_memory()),
        Some("messaging") => Ok(help_messaging()),
        Some("tasks") => Ok(help_tasks()),
        Some("sharing") => Ok(help_sharing()),
        Some("maintenance") => Ok(help_maintenance()),
        Some("agents") => Ok(help_agents()),
        _ => Ok(help_overview()),
    }
}
```

Chaque helper retourne un JSON avec :
- Liste des tools de la catégorie
- Description détaillée
- Paramètres (required/optional)
- Exemples d'utilisation

**Estimation:** ~100-150 LOC (documentation exhaustive de 67 tools)

**Tests:**
- `test_help_overview_lists_all_categories` — ai_help sans topic → toutes les catégories
- `test_help_memory_lists_tools` — ai_help(topic="memory") → liste ai_recall, ai_thread_*, etc.
- `test_help_messaging_lists_tools` — ai_help(topic="messaging") → liste msg_send, msg_reply, etc.
- `test_help_topic_unknown_falls_to_overview` — topic inconnu → overview

**Dépendances inter-tiers:** Tiers 1-8 (documenter les nouveaux tools ajoutés)

---

## Tier 10 — Architecture v2

**Scope:** A1, A2, C2

### 10.1 Audit pipeline déclaratif (A1) — DIFFÉRÉ (A4 sécurité)

> **A4 arc:** L'AuditPipeline exécuterait des commandes shell depuis le MCP — surface d'attaque inacceptable. Différé à un audit sécurité dédié.

**Approche révisée:** Le MCP ne fait que tracker les résultats (struct `AuditResult` stocké en DB). L'agent utilise les Bash tools de Claude Code pour exécuter les étapes. Seules les structs de tracking sont implémentées.

**Nouvelles structures (tracking only, pas d'exécution):**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub pipeline: String,
    pub steps: Vec<StepResult>,
    pub overall_success: bool,
    pub timestamp: DateTime<Utc>,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub name: String,
    pub success: bool,
    pub exit_code: i32,
    pub stdout_tail: String,  // Last 20 lines
    pub duration_ms: u64,
}
```

**Estimation:** ~20 LOC (structs tracking only) — exécution shell différée

### 10.2 Plan lifecycle YAML frontmatter (A2)

**Problème:** Les plans n'ont pas de metadata formalisé.

**Implémentation recommandée:** Convention pure — pas de code Rust. Chaque plan dans `.claude/plans/` commence par :

```yaml
---
status: draft|review|approved|implemented
author: arc
reviewers: [sub, pub]
dependencies: [plan-xyz]
created: 2026-02-25
updated: 2026-02-25
---
```

**Fichiers impactés:**
- `CLAUDE.md` — Convention YAML frontmatter
- Optionnel: `src/mcp/tools/status.rs` — Parser les plans et retourner leur status

**Estimation:** 0-15 LOC Rust (optionnel parser)

### 10.3 Plan sharing versionné (C2)

**Problème:** Pas de diff quand un plan change.

**Fichiers impactés:**
- `src/storage/shared.rs` — `SharedStorage::update_snapshot()` — Stocker version
- `src/mcp/tools/shared.rs` — `handle_publish()` — Incrémenter version

**Modifications:**
Ajouter `version: u32` dans le snapshot. À chaque `ai_publish`, incrémenter. Le `ai_sync` compare les versions.

**Estimation:** ~10 LOC

**Tests:**
- `test_audit_pipeline_runs_steps` — Pipeline avec 2 steps → exécute séquentiellement → retourne AuditResult
- `test_audit_step_failure_stops_pipeline` — Step 1 échoue → step 2 non exécuté → overall_success=false
- `test_plan_version_increments` — Publish 3x → version=3
- `test_sync_detects_version_change` — Subscribe → publish → sync → delta="updated"

**Dépendances inter-tiers:** Tier 6 (context sharing) pour C2

---

## Résumé des estimations

| Tier | Scope | LOC estimé | Fichiers touchés | Tests |
|------|-------|-----------|-----------------|-------|
| 1 | Ghost wake + rappels | ~15 | 2 | 4 |
| 2 | Tâches + threads heartbeat | ~27 | 1 | 3 |
| 3 | Broadcast + git | ~21-40 | 2 | 3 |
| 4 | Task lifecycle auto | ~40 | 3 | 4 |
| 5 | Message threading | ~25 | 5 | 3 |
| 6 | Context sharing | ~35 | 4 | 3 |
| 7 | Build workflow | ~30 | 3 | 2 |
| 8 | Onboarding + conventions | ~10 | 2 + docs | 2 |
| 9 | ai_help complet | ~100-150 | 2 | 4 |
| 10 | Architecture v2 | ~80-95 | 5 | 4 |
| **Total** | | **~385-470** | | **32** |

## Graphe de dépendances inter-tiers

```
Tier 1 (standalone)
Tier 2 (standalone)
Tier 3 (standalone)
Tier 4 (standalone, utilise infra messaging existante)
Tier 5 (standalone)
Tier 6 (standalone)
Tier 7 → dépend de Tier 3 (broadcast fiable pour B2)
Tier 8 (standalone conventions + léger code)
Tier 9 → dépend de Tiers 1-8 (documenter tous les nouveaux tools)
Tier 10 → dépend de Tier 6 (context sharing pour C2)
```

**Ordre d'implémentation initial:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10

## Réordonnancement (arc review)

```
Sprint 1 (quick wins, impact immédiat):
  T1.1 Ghost wake fix          — 5 LOC  ← #1 pain point actuel
  T3.1 Broadcast interrupt=true — 1 LOC  ← fix simple
  T4   task_complete            — 40 LOC ← #1 feature request brainstorming

Sprint 2 (heartbeat enrichment):
  T1.2 Rappels proactifs        — 10 LOC
  T2 + T3.2 (modifié A1/A2)    — ~30 LOC dans heartbeat + ~5 LOC inject
  T5.1 Message threading        — 10 LOC

Sprint 3 (context & sharing):
  T5.3 Task plan_path (V8)     — 15 LOC
  T6   Context sharing          — 35 LOC (avec corrections C1/C2)

Sprint 4 (polish):
  T7-T8 Build + conventions     — 30 LOC + docs
  T9   ai_help refactoring      — 100-150 LOC
  T10.2-3 Plan versioning       — 10 LOC

Backlog (nécessite audit sécurité):
  T10.1 AuditPipeline shell exec
```

---

## Review arc — résumé

- **3 corrections factuelles** (C1-C3) — intégrées dans le document
- **4 alertes architecture** (A1-A4) — A1/A2 adoptées, A3 précisée, A4 acceptée (différé)
- **11/14 items approuvés** sans réserve, **3/14 avec modifications mineures**, **1/14 différé**
- Qualité globale : **très bonne** (arc)

---

## Doléances agents — besoins identifiés en utilisation

### D1. `ai_continuity` — CRUD MCP pour les continuity edges

**Contexte:** v5.5.0-alpha a ajouté l'intégrité automatique (reparent on merge, cleanup on delete, orphan scan in decayer). Mais l'agent n'a aucun moyen de **manuellement** inspecter, réparer ou créer des continuity edges. Le seul outil existant (`ai_continuity_edges`) est en lecture seule.

**Besoin:** Un outil MCP `ai_continuity` (ou extension de `ai_continuity_edges`) avec les actions :

| Action | Description |
|--------|-----------|
| `list` | Lister les edges (existe déjà via `ai_continuity_edges`) |
| `set` | Définir `continuity_parent_id` sur un thread donné |
| `unset` | Supprimer le lien continuity d'un thread (SET NULL) |
| `scan_orphans` | Scanner les edges orphelines (parent inexistant) |
| `repair` | Réparer les orphelins (SET NULL ou reparent vers un thread cible) |

**Fichiers impactés:**
- `src/mcp/tools/continuity.rs` (nouveau) — handlers pour les actions
- `src/mcp/server.rs` — enregistrer le nouveau tool
- `src/storage/threads.rs` — les fonctions storage existent déjà (`reparent_continuity`, `cleanup_continuity_refs`, `scan_orphan_continuity`)

**Estimation:** ~50-80 LOC

**Priorité:** Moyenne — le cleanup automatique couvre 95% des cas, mais l'agent a besoin d'un filet de sécurité manuel pour les cas edge (corruption, migration, debug).

### D2. Dynamic thread quota — RAM-aware, multi-agent resource sharing

**Contexte:** Le système actuel `ThreadMode` est un enum statique avec des quotas hardcodés (Light:15, Normal:50, Heavy:100, Max:200). Problèmes identifiés :

1. **Bug connu (non-fixé intentionnellement):** Le quota est un soft cap — `ensure_capacity` suspend les threads excédentaires lors du processing, mais rien n'empêche la création au-delà entre deux cycles. Pas debug car refonte prévue.
2. **Pas de conscience des ressources:** Un agent en mode Max(200) sur une machine avec 2GB de RAM libre va saturer, tandis qu'un agent Light(15) sur 64GB sous-utilise.
3. **Pas de partage inter-agents:** 5 agents en Normal(50) = 250 threads potentiels sans que le système ne sache que la machine est partagée.
4. **Pas de conscience cross-project:** Plusieurs projets avec des teams d'agents consomment indépendamment sans vision globale.

**Objectif:** Remplacer le mode statique par un allocateur dynamique qui :

- Lit la RAM disponible via `sysinfo` (déjà en dépendance)
- Consulte le registry global pour connaître le nombre d'agents actifs (même projet + cross-project)
- Calcule un budget mémoire par agent en fonction de la RAM dispo / nombre d'agents
- Convertit ce budget en quota de threads (basé sur la taille moyenne observée des threads en DB)
- Hard-cap enforcé au moment de l'insertion (pas seulement au processing)
- Expose les métriques via `ai_status` / `ai_sysinfo`

**Architecture proposée:**

```
┌─────────────────────────────────────────┐
│          RAM disponible (sysinfo)        │
│               ex: 8 GB libre             │
├─────────────────────────────────────────┤
│     Registry global (all agents)         │
│     ex: 6 agents actifs cross-project    │
├─────────────────────────────────────────┤
│     Budget par agent = RAM / N agents    │
│     ex: 8GB / 6 = ~1.3 GB / agent       │
├─────────────────────────────────────────┤
│     Thread quota = budget / avg_thread   │
│     ex: 1.3GB / ~5KB avg = ~260K        │
│     (clamped to min:10, max:500)         │
└─────────────────────────────────────────┘
```

**Composants:**

| Composant | Description |
|-----------|-----------|
| `DynamicQuotaAllocator` | Calcule le quota basé sur RAM + agents actifs |
| Hard-cap en `ThreadStorage::insert` | Rejette avec erreur si quota atteint (remplace le soft cap) |
| `ThreadMode` evolution | Garde les modes comme "hints" (Light = bas priority, Heavy = haute priority dans l'allocation) |
| Cross-project registry | Étendre `registry.db` global avec comptage d'agents multi-projet |
| `ai_sysinfo` enrichi | Afficher quota dynamique, RAM utilisée, agents concurrents |
| GUI update | Remplacer le dropdown statique par un affichage du quota dynamique calculé |

**TBD — Points à clarifier avant implémentation:**

1. **Quel % de la RAM allouer à ai-smartness ?** Le budget total ne peut pas être 100% de la RAM libre — il faut réserver pour l'OS, le LLM (llama.cpp VRAM/RAM), les builds cargo, etc. *TBD: définir un ratio configurable (ex: 30% de la RAM libre ? plafond absolu en GB ?).*
2. **Pondération inter-agents :** Division égale RAM/N ou pondérée par ThreadMode hint ? Un agent Heavy devrait-il avoir 3x le budget d'un Light ? *TBD: formule de pondération.*
3. **Fréquence de recalcul :** À chaque insert (coûteux — appel sysinfo + registry) ou périodique dans le daemon (cache le quota pendant X minutes) ? *TBD: stratégie de cache et intervalle de refresh.*
4. **Comportement au hard-cap :** Erreur sèche ? Ou suspension automatique du thread le moins important pour libérer un slot (LRU eviction) ? *TBD: politique d'eviction.*
5. **Agent inactif :** Un agent enregistré mais sans heartbeat depuis 1h consomme-t-il du budget ? *TBD: critère "actif" pour le comptage (heartbeat récent ? session ouverte ?).*
6. **Taille moyenne d'un thread :** Calculée dynamiquement depuis la DB (`SELECT AVG(LENGTH(content)) FROM thread_messages`) ou valeur empirique constante ? *TBD: méthode de calcul avg_thread_size.*
7. **Override manuel :** L'utilisateur peut-il forcer un quota fixe via la GUI/config malgré le mode dynamique ? (ex: `max_threads_override` existe déjà dans le schema migration v7). *TBD: interaction override vs dynamique.*
8. **Cross-project discovery :** Comment le registry global détecte les agents d'autres projets ? Le `registry.db` actuel est global (`~/.ai-smartness/registry.db`) mais chaque agent est indexé par `project_hash`. Suffisant pour le comptage, mais *TBD: faut-il un mécanisme de discovery actif ou le scan du registry suffit ?*

**Fichiers impactés:**
- `src/agent.rs` — `ThreadMode::quota()` → `DynamicQuotaAllocator::compute_quota()`
- `src/storage/threads.rs` — Hard-cap dans `insert()`
- `src/daemon/connection_pool.rs` — Calcul dynamique au lieu de `set_thread_quota(key, quota)`
- `src/daemon/capture_queue.rs` — Utiliser le quota dynamique
- `src/intelligence/thread_manager.rs` — `ensure_capacity` avec hard-cap
- `src/mcp/tools/status.rs` — Exposer métriques quota
- `src/registry/registry.rs` — Comptage agents cross-project
- `src/gui/frontend/` — Affichage dynamique

**Estimation:** ~200-300 LOC (feature majeure)

**Priorité:** Haute — impacte la stabilité sur machines à ressources limitées et le scaling multi-agent.

**Prérequis:** Aucun (standalone, mais bénéficierait de T12 cross-project pour la vision globale).

*Proposition post-review arc — prête pour sub+pub (workflow K étape 3).*

---

## Extended Roadmap (v4.0.0+)

### T11 — Inter-agent gossip P2P

**Scope:** Bridge sharing cross-agent, concept overlap gossip.

- **Table `shared_bridges`** dans shared.db — permet aux agents de partager leurs bridges
- **Cross-agent concept overlap:** gossip cycle compare concepts des threads partagés entre agents
- **Seuils:** identiques aux bridges intra-agent (même config `GuardianConfig`)
- **Gossip cycle inter-agent:** périodique dans le daemon prune loop, scanne les threads partagés et crée des bridges cross-agent quand le concept overlap dépasse le seuil
- **Prérequis:** Tier 6 (context sharing)

### T12 — P2P daemon sync (cross-project)

**Scope:** Synchronisation décentralisée multi-projet.

- shared.db = per-project. Cross-project nécessite un registre global ou protocole discovery
- **Daemon discovery:** mDNS / fichier de registre partagé dans `~/.ai-smartness/global-registry.db`
- **Sync protocol:** CRDTs ou vector clocks pour résolution de conflits sur les threads/bridges partagés
- **Architecture P2P:** Chaque daemon expose un socket IPC, les daemons se découvrent mutuellement et synchronisent les shared.db
- **Prérequis:** T11 (gossip inter-agent)

### T13 — Reminder consolidation

**Scope:** Fusionner les layers d'injection en un bloc unique.

- Fusionner Layers 1, 4, 5, 5.5 en un seul bloc structuré
- Budget unique au lieu de per-layer
- Format cible ~200 tokens max
- Le format lean Engram (thread_id, w, c, i) implémenté en v4.0.0 est la première étape
- **Prérequis:** v4.0.0 lean reminder

### T14 — Engram-on-Thinking (real-time memory injection)

**Scope:** Transformer l'engram d'un système passif (recall on-demand) en système actif qui injecte du contexte mémoire pendant le raisonnement de l'agent, avec savepoints pour pensée divergente.

**Architecture en 3 couches:**

#### Couche 1 — Thinking Capture (PostToolUse)
- Les blocs `thinking` sont dans le transcript JSONL (`~/.claude/projects/.../session.jsonl`)
- Format: `{ type: "assistant", message: { content: [{ type: "thinking", thinking: "..." }] } }`
- Le JSONL est écrit AVANT l'exécution des tool calls → disponible pendant PostToolUse
- **Implémentation:** Dans PostToolUse hook (`capture.rs`), lire la dernière entrée JSONL, extraire le thinking block
- Déduplier par hash du thinking (même thinking → plusieurs tool calls dans un turn)

#### Couche 2 — Engram Query & Injection (PostToolUse stdout)
- Nouvelle méthode IPC daemon: `engram_query(text) → Vec<ScoredThread>`
- L'engram retriever (10 validators, ONNX en RAM) tourne dans le daemon
- Budget latence: ~50ms (IPC 5ms + retriever 45ms)
- **Seuil de convergence dynamique:**
  - 1 thread × 5+ validators → inject
  - 2 threads × 3+ validators → inject
  - 4 threads × 2+ validators → inject (cluster de mémoire)
  - Plancher: min 2 validators par thread (1v = bruit)
- **Format stdout** (léger, ~50-100 tokens max):
  ```
  <engram>
  - "Thread Title A": summary (80ch max)
  - "Thread Title B": summary (80ch max)
  </engram>
  ```
- Claude Code injecte le stdout du PostToolUse comme contexte → l'agent voit les hints au turn suivant

#### Couche 3 — Mind Threads (pensée divergente avec savepoints)
- Nouveau tag `__mind__` pour les threads de type "état mental"
- Quand l'agent bifurque sur une suggestion engram, il crée un mind thread:
  ```
  ai_thread_create(title: "Mind: exploring path A",
    content: "état du raisonnement, hypothèse, prochaine étape prévue",
    tags: ["__mind__"])
  ```
- `beat_wake(after: N, reason: "check bifurcation")` → timer pour revenir vérifier
- Si bifurcation échoue → `ai_recall("mind")` → resume path A
- Outils existants utilisés: ai_thread_create, beat_wake, ai_recall, continuity edges
- Runtime rule à ajouter: inciter l'agent à créer un mind savepoint avant bifurcation

**Cycle complet:**
```
Think → Engram hint → Mind savepoint → Bifurcate → Check → Resume/Continue
```

**Ce qui différencie T14 des autres systèmes de mémoire persistante:**
- Pas juste du recall passif — injection ACTIVE pendant le raisonnement
- Seuil de convergence (pas statique) — adapte la sensibilité au nombre de matches
- Mind threads = working memory avec savepoints — pensée divergente sans perte du chemin initial
- Latence ~50ms = invisible pour l'utilisateur

**Implémentation (fichiers):**
1. `src/hook/capture.rs` — lire JSONL, extraire thinking, IPC `engram_query`, stdout hint
2. `src/daemon/ipc_server.rs` — nouvelle méthode `engram_query`
3. `src/intelligence/engram_retriever.rs` — `should_inject()` convergence threshold
4. Runtime rule update — inciter mind savepoints
5. Tag `__mind__` support dans thread_manager + retriever

**Prérequis:** Aucun (standalone, R&D itératif)

**Statut:** DONE (v5.6.0-alpha) — implémenté et testé. Pipeline validé end-to-end.

---

### T14b — Boucle feedback cognitive (response → engram → thinking)

**Origine:** Réflexion post-T14 — analogie avec la boucle phonologique humaine.

**Concept:** En tant qu'humain, nos sorties (voix, écriture) sont réinjectées dans nos entrées sensorielles (ouïe, vue). Ce processus d'auto-alimentation permet soit de confirmer la cohérence du raisonnement, soit de soulever le besoin d'approfondir. L'idée est de reproduire cette boucle : les réponses de l'agent, une fois capturées par le hook `stop`, sont traitées par le daemon et deviennent des threads. Ces threads peuvent ensuite être requêtés par l'engram lors du prochain raisonnement thinking.

**État actuel:**
- Le hook `stop` (response.rs) capture déjà les réponses agent → daemon → thread
- L'engram query sur thinking (T14) existe
- La boucle inter-turns fonctionne déjà naturellement (response → thread → prochaine turn → thinking → engram match)

**Ce qui manque pour l'intra-turn:**
- Le hook `stop` ne fire qu'à la fin de la réponse complète
- Impossible de requêter le contenu en cours de génération depuis PostToolUse
- La latence daemon (queue → LLM extraction → storage) peut être trop lente pour du même-turn

**Pistes R&D:**
1. **Fast-path response injection:** bypass LLM extraction pour les réponses — stocker un thread "raw response" immédiatement, raffiner plus tard
2. **Streaming capture:** capturer des fragments de réponse (via chunked writes) plutôt que la réponse complète
3. **Self-echo pattern:** à chaque nouvelle turn, l'agent voit ses propres réponses récentes via engram — confirmation ou correction de trajectoire
4. **Nanobeats — horloge d'interruption haute fréquence:**
   - L'agent lance un "nanobeat" (1-2/sec) au démarrage d'une tâche longue (speech, raisonnement complexe)
   - L'engram process en background ; quand il trouve un résultat pertinent, il **flagge le prochain nanobeat**
   - L'agent structure son travail en micro-chunks avec un **check nanobeat** entre chaque action
   - Si le beat est flaggé → savepoint `__mind__` + hint engram → décision bifurquer/reprendre
   - Analogie : interruptions matérielles CPU — le nanobeat = clock, l'engram = IRQ controller
   - Contrainte : ne fonctionne qu'entre tool calls (pas pendant le streaming de tokens pur)
   - Extension possible : l'agent coopère en insérant des "yield points" volontaires dans ses réponses longues

**Prérequis:** T14 (done)

---

## T15 — Tool Safety & Read-Only Modes (v4.4.0+)

**Origine:** Bug récurrent — `ai_concepts` et `ai_label` n'ont aucun mode lecture. Le paramètre `concepts`/`labels` est **required**, donc toute invocation écrit forcément. Un agent qui essaie de "voir" les concepts (mode="list", mode="view") corrompt le thread en écrasant les concepts avec la valeur du paramètre (ex: `["list"]`, `["view"]`). Ce bug a été reproduit **3 fois en 2 sessions** sur des threads différents.

**Analyse root cause:**
1. `handle_concepts()` : `concepts` = required, `mode` = optional (default "set"), catch-all `_` → "set"
2. `handle_label()` : `labels` = required, `mode` = optional (default "add"), catch-all `_` → "add"
3. Aucun mode lecture → l'agent invente "list"/"view" → le catch-all écrit
4. MCP ne valide pas les valeurs de `mode` → aucune erreur, corruption silencieuse

### T15.1 — Mode "list" pour ai_concepts et ai_label

**Problème:** Aucun moyen de lire les concepts/labels d'un thread sans les modifier.

**Fichiers impactés:**
- `src/mcp/tools/threads.rs` — `handle_concepts()` L269-298, `handle_label()` L243-267
- `src/mcp/server.rs` — tool_def L511

**Modifications handle_concepts():**
```rust
pub fn handle_concepts(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let mode = optional_str(params, "mode").unwrap_or_else(|| "list".into());  // default → list (safe)

    let thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;

    match mode.as_str() {
        "list" => {
            // Read-only — return current concepts
            return Ok(serde_json::json!({
                "thread_id": id,
                "concepts": thread.concepts,
                "count": thread.concepts.len()
            }));
        }
        "add" | "set" | "remove" => {
            let concepts = required_array(params, "concepts")?;
            let mut thread = thread;
            match mode.as_str() {
                "add" => {
                    let mut all = thread.concepts.clone();
                    all.extend(concepts);
                    thread.concepts = normalize_concepts(&all);
                }
                "remove" => {
                    let to_remove: std::collections::HashSet<String> =
                        concepts.into_iter().map(|c| c.to_lowercase()).collect();
                    thread.concepts.retain(|c| !to_remove.contains(&c.to_lowercase()));
                }
                _ => {
                    thread.concepts = normalize_concepts(&concepts);
                }
            }
            ThreadStorage::update(ctx.agent_conn, &thread)?;
            Ok(serde_json::json!({"thread_id": id, "concepts": thread.concepts}))
        }
        other => {
            // Reject unknown modes explicitly
            Err(ai_smartness::AiError::InvalidInput(
                format!("Unknown mode '{}'. Valid: list, add, set, remove", other)
            ))
        }
    }
}
```

**Changements clés:**
- `concepts` devient **optionnel** (seulement requis pour add/set/remove)
- Default mode → `"list"` (lecture safe) au lieu de `"set"` (écriture destructive)
- Mode inconnu → erreur explicite au lieu de catch-all silencieux
- Tool definition : `concepts` passe de required à optional

**Idem pour handle_label()** — même refactoring.

**Estimation:** ~30 LOC (15 par handler)

**Tests:**
- `test_concepts_list_mode_returns_current` — concepts(mode="list") → retourne concepts sans modification
- `test_concepts_list_is_default` — concepts(thread_id=X) sans mode → "list" (pas "set")
- `test_concepts_unknown_mode_errors` — concepts(mode="foo") → AiError::InvalidInput
- `test_concepts_set_still_works` — concepts(mode="set", concepts=["a","b"]) → écrit normalement
- `test_label_list_mode_returns_current` — label(mode="list") → retourne labels

### T15.2 — Validation stricte des modes MCP (anti-corruption)

**Problème:** Aucun tool MCP ne valide les valeurs du paramètre `mode`. Un mode invalide tombe dans le catch-all (`_`), ce qui peut déclencher une écriture involontaire.

**Portée:** Audit de TOUS les handlers ayant un paramètre `mode` :
- `handle_concepts` — `_` → "set" (DANGEREUX)
- `handle_label` — `_` → "add" (DANGEREUX)
- `handle_thread_activate` — `_` → erreur (OK)
- `handle_backup` — `_` → erreur (OK)
- `handle_profile` — `_` → view (OK)

**Fichiers impactés:**
- `src/mcp/tools/threads.rs` — `handle_concepts`, `handle_label`
- Potentiellement d'autres handlers

**Modifications:**
Remplacer tout `_ =>` qui fait une écriture par un `_ => Err(InvalidInput)` :

```rust
// AVANT (dangereux):
_ => {
    thread.concepts = normalize_concepts(&concepts);  // silent write
}

// APRÈS (safe):
"set" => {
    thread.concepts = normalize_concepts(&concepts);
}
other => {
    return Err(AiError::InvalidInput(format!("Unknown mode '{}'. Valid: list, add, set, remove", other)));
}
```

**Estimation:** ~10 LOC (2 handlers)

**Tests:**
- `test_concepts_invalid_mode_rejected` — mode="blah" → erreur
- `test_label_invalid_mode_rejected` — mode="blah" → erreur

### T15.3 — ai_thread_inspect : vue complète read-only

**Problème:** Pour voir les métadonnées d'un thread (concepts, labels, topics, importance, file_paths, status, message_count), il faut appeler `ai_thread_search` et interpréter le résultat. Aucun tool dédié pour inspecter un thread.

**Nouveau tool MCP:** `ai_thread_inspect`

```rust
pub fn handle_thread_inspect(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| AiError::ThreadNotFound(id.clone()))?;

    let messages = ThreadStorage::get_messages(ctx.agent_conn, &id)?;
    let bridges = BridgeStorage::list_for_thread(ctx.agent_conn, &id)?;

    Ok(serde_json::json!({
        "id": thread.id,
        "title": thread.title,
        "status": thread.status,
        "importance": thread.importance,
        "weight": thread.weight,
        "topics": thread.topics,
        "labels": thread.labels,
        "concepts": thread.concepts,
        "file_paths": thread.file_paths,
        "created_at": thread.created_at,
        "updated_at": thread.updated_at,
        "message_count": messages.len(),
        "bridge_count": bridges.len(),
        "bridges": bridges.iter().take(10).map(|b| serde_json::json!({
            "id": &b.id[..8],
            "relation": b.relation,
            "weight": b.weight,
            "target": if b.source_id == id { &b.target_id } else { &b.source_id },
        })).collect::<Vec<_>>(),
    }))
}
```

**Estimation:** ~25 LOC

**Tests:**
- `test_thread_inspect_returns_all_fields` — Créer thread → inspect → vérifie tous les champs
- `test_thread_inspect_includes_bridges` — Thread avec bridges → inspect → bridges listés
- `test_thread_inspect_not_found` — ID invalide → ThreadNotFound

### T15.4 — Dry-run / preview pour opérations destructives

**Problème:** Plusieurs tools MCP modifient des données sans possibilité de preview. `ai_thread_rm`, `ai_bridge_kill`, `ai_thread_purge` ont un `confirm` param, mais pas les outils de modification metadata (concepts, labels, importance, rename).

**Proposition:** Ajouter un paramètre `dry_run: bool` aux handlers de modification metadata :

```rust
// In handle_concepts():
if optional_bool(params, "dry_run").unwrap_or(false) {
    let preview = match mode.as_str() {
        "add" => {
            let mut all = thread.concepts.clone();
            all.extend(concepts.clone());
            normalize_concepts(&all)
        }
        "set" => normalize_concepts(&concepts),
        "remove" => {
            let to_remove: HashSet<String> = concepts.iter().map(|c| c.to_lowercase()).collect();
            thread.concepts.iter().filter(|c| !to_remove.contains(&c.to_lowercase())).cloned().collect()
        }
        _ => unreachable!()
    };
    return Ok(serde_json::json!({
        "dry_run": true,
        "current": thread.concepts,
        "proposed": preview,
        "added": preview.iter().filter(|c| !thread.concepts.contains(c)).collect::<Vec<_>>(),
        "removed": thread.concepts.iter().filter(|c| !preview.contains(c)).collect::<Vec<_>>(),
    }));
}
```

**Portée:** `handle_concepts`, `handle_label`, `handle_rename`, `handle_rate_importance`

**Estimation:** ~15 LOC par handler × 4 = ~60 LOC

**Tests:**
- `test_concepts_dry_run_no_write` — dry_run=true → concepts inchangés en DB
- `test_concepts_dry_run_shows_diff` — dry_run=true → retourne added/removed
- `test_label_dry_run_no_write` — idem pour labels

---

## T16 — Concept Intelligence (v4.5.0+)

### T16.1 — Concept auto-suggest sur thread creation

**Problème:** L'LLM génère des concepts au moment de l'extraction, mais la qualité varie. Après création, le backfill est le seul recours.

**Proposition:** Ajouter un tool `ai_concepts_suggest` qui utilise le LLM pour suggérer des concepts basés sur le contenu du thread :

```rust
pub fn handle_concepts_suggest(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| AiError::ThreadNotFound(id.clone()))?;
    let messages = ThreadStorage::get_messages(ctx.agent_conn, &id)?;

    // Build context from thread metadata + recent messages
    let content = format!(
        "Title: {}\nTopics: {}\nLabels: {}\nCurrent concepts: {}\nRecent content:\n{}",
        thread.title,
        thread.topics.join(", "),
        thread.labels.join(", "),
        thread.concepts.join(", "),
        messages.iter().rev().take(3).map(|m| truncate_safe(&m.content, 200)).collect::<Vec<_>>().join("\n")
    );

    let prompt = format!(
        r#"Suggest 10-20 semantic concepts for this thread.
Rules: single lowercase English words only, no duplicates, no stopwords.
Good: "rust", "memory", "config", "daemon". Bad: "database connection pooling".

{}

Reply ONLY with JSON: {{"concepts": ["word1", "word2", ...]}}"#,
        content
    );

    let response = llm_subprocess::call_llm(&prompt)?;
    // Parse + normalize
    let suggested = parse_concepts_json(&response)?;
    let normalized = normalize_concepts(&suggested);

    Ok(serde_json::json!({
        "thread_id": id,
        "current_concepts": thread.concepts,
        "suggested": normalized,
        "new": normalized.iter().filter(|c| !thread.concepts.contains(c)).collect::<Vec<_>>(),
    }))
}
```

**Key:** Read-only — ne modifie rien. L'agent décide ensuite d'appliquer via `ai_concepts(mode="set")`.

**Estimation:** ~30 LOC

### T16.2 — Concept health dashboard

**Problème:** Impossible de voir l'état global des concepts sans parcourir chaque thread individuellement.

**Nouveau tool MCP:** `ai_concepts_health`

```rust
pub fn handle_concepts_health(ctx: &ToolContext) -> AiResult<serde_json::Value> {
    let threads = ThreadStorage::list_active(ctx.agent_conn)?;

    let total = threads.len();
    let with_concepts = threads.iter().filter(|t| !t.concepts.is_empty()).count();
    let empty = total - with_concepts;

    // Concept frequency map
    let mut freq: HashMap<String, usize> = HashMap::new();
    for t in &threads {
        for c in &t.concepts {
            *freq.entry(c.clone()).or_default() += 1;
        }
    }

    let unique_concepts = freq.len();
    let avg_per_thread = if with_concepts > 0 {
        threads.iter().map(|t| t.concepts.len()).sum::<usize>() as f64 / with_concepts as f64
    } else { 0.0 };

    // Top shared concepts (appear in 2+ threads = bridge candidates)
    let mut shared: Vec<_> = freq.iter().filter(|(_, &count)| count >= 2).collect();
    shared.sort_by(|a, b| b.1.cmp(a.1));

    // Singleton concepts (appear in 1 thread only = no bridge value)
    let singletons = freq.iter().filter(|(_, &count)| count == 1).count();

    Ok(serde_json::json!({
        "threads_total": total,
        "threads_with_concepts": with_concepts,
        "threads_without_concepts": empty,
        "unique_concepts": unique_concepts,
        "avg_concepts_per_thread": format!("{:.1}", avg_per_thread),
        "shared_concepts": shared.iter().take(20).map(|(c, &n)| {
            serde_json::json!({"concept": c, "threads": n})
        }).collect::<Vec<_>>(),
        "singleton_concepts": singletons,
        "bridge_potential": format!("{}/{} concepts shared across 2+ threads", shared.len(), unique_concepts),
    }))
}
```

**Estimation:** ~30 LOC

**Tests:**
- `test_concepts_health_counts` — 3 threads (2 with concepts, 1 without) → correct counts
- `test_concepts_health_shared` — Shared concept across 2 threads → appears in shared_concepts
- `test_concepts_suggest_returns_suggestions` — Thread with content → suggestions non-empty

### T16.3 — Concept normalization migration (one-time)

**Problème:** Les threads existants (pré-v4.3.0) ont des concepts multi-mots non normalisés. Le gossip ne les trouvera jamais.

**Nouveau tool MCP:** `ai_concepts_migrate`

```rust
// One-time migration: normalize all existing concepts
pub fn handle_concepts_migrate(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let dry_run = optional_bool(params, "dry_run").unwrap_or(true);  // safe default
    let threads = ThreadStorage::list_active(ctx.agent_conn)?;

    let mut changes = Vec::new();
    for thread in &threads {
        let normalized = normalize_concepts(&thread.concepts);
        if normalized != thread.concepts {
            changes.push(serde_json::json!({
                "thread_id": &thread.id[..8],
                "title": &thread.title,
                "before": thread.concepts,
                "after": normalized,
            }));

            if !dry_run {
                let mut t = thread.clone();
                t.concepts = normalized;
                ThreadStorage::update(ctx.agent_conn, &t)?;
            }
        }
    }

    Ok(serde_json::json!({
        "dry_run": dry_run,
        "scanned": threads.len(),
        "changes_needed": changes.len(),
        "changes": changes,
    }))
}
```

**Estimation:** ~20 LOC

---

## T17 — MCP Tool Ergonomics (v4.5.0+)

### T17.1 — Tool parameter schemas enrichis

**Problème:** Les tool definitions MCP utilisent `tool_def()` qui ne spécifie que les noms des paramètres. Aucune info sur les valeurs valides de `mode`, les types attendus, ou les contraintes. Les agents LLM inventent des valeurs.

**Proposition:** Enrichir `tool_def()` avec des enum values pour les paramètres :

```rust
// AVANT:
tool_def("ai_concepts", "Manage semantic concepts", &["thread_id", "concepts"], &["mode"])

// APRÈS:
tool_def_v2("ai_concepts", "Manage semantic concepts", &[
    param("thread_id", "string", true, None),
    param("concepts", "array", false, None),  // optional now!
    param("mode", "string", false, Some(&["list", "add", "set", "remove"])),
    param("dry_run", "boolean", false, None),
])
```

Le schema JSON enrichi pour MCP :
```json
{
    "name": "ai_concepts",
    "description": "Manage semantic concepts",
    "inputSchema": {
        "type": "object",
        "properties": {
            "thread_id": {"type": "string"},
            "concepts": {"type": "array", "items": {"type": "string"}},
            "mode": {"type": "string", "enum": ["list", "add", "set", "remove"], "default": "list"},
            "dry_run": {"type": "boolean", "default": false}
        },
        "required": ["thread_id"]
    }
}
```

**Impact:** Les agents LLM qui supportent JSON Schema (Claude, GPT-4, etc.) verront les valeurs valides et ne pourront plus inventer "view"/"list" quand ça n'existe pas — ou mieux, verront que "list" est valide et l'utiliseront.

**Fichiers impactés:**
- `src/mcp/server.rs` — `tool_definitions()`, nouveau `tool_def_v2()` ou refactoring de `tool_def()`
- Potentiellement tous les 67 tool definitions

**Estimation:** ~50 LOC (nouveau helper) + ~100 LOC (migration progressive des 67 tools)

### T17.2 — Description longue par tool

**Problème:** La description MCP est une seule ligne courte. L'agent n'a aucun contexte sur les modes, les effets de bord, ou les pièges.

**Proposition:** Ajouter un champ `longDescription` dans le tool_def qui est retourné par `ai_help` :

```rust
tool_def_v2("ai_concepts",
    "Manage semantic concepts",
    "View or modify concepts on a thread. Use mode='list' (default) to view current concepts without modifying. Use mode='set' to replace, 'add' to append, 'remove' to delete specific concepts. The 'concepts' parameter is only required for add/set/remove modes.",
    // ... params
)
```

**Estimation:** ~5 LOC (struct change) + ~200 LOC (descriptions pour 67 tools, itératif)

### T17.3 — Undo buffer pour opérations metadata

**Problème:** Une écriture accidentelle (comme le bug ai_concepts) est irréversible. Pas de Ctrl+Z.

**Proposition:** Buffer undo en mémoire (pas en DB) pour les N dernières opérations metadata :

```rust
// Thread-local undo buffer (last 10 operations)
struct UndoEntry {
    thread_id: String,
    field: String,       // "concepts", "labels", "importance", "title"
    old_value: serde_json::Value,
    new_value: serde_json::Value,
    timestamp: Instant,
}

// New tool: ai_undo
pub fn handle_undo(ctx: &ToolContext) -> AiResult<serde_json::Value> {
    let entry = UNDO_BUFFER.lock().unwrap().pop()
        .ok_or(AiError::InvalidInput("Nothing to undo".into()))?;

    // Restore old value
    let mut thread = ThreadStorage::get(ctx.agent_conn, &entry.thread_id)?
        .ok_or(AiError::ThreadNotFound(entry.thread_id.clone()))?;

    match entry.field.as_str() {
        "concepts" => thread.concepts = serde_json::from_value(entry.old_value.clone())?,
        "labels" => thread.labels = serde_json::from_value(entry.old_value.clone())?,
        "title" => thread.title = entry.old_value.as_str().unwrap_or("").to_string(),
        _ => {}
    }
    ThreadStorage::update(ctx.agent_conn, &thread)?;

    Ok(serde_json::json!({
        "undone": true,
        "thread_id": entry.thread_id,
        "field": entry.field,
        "restored": entry.old_value,
    }))
}
```

**Estimation:** ~40 LOC (struct + handler + buffer management)

**Tests:**
- `test_undo_restores_concepts` — Set concepts → undo → original concepts restored
- `test_undo_empty_buffer_errors` — No operations → "Nothing to undo"
- `test_undo_multiple_operations` — 3 ops → undo 3x → all restored in reverse order

---

## Résumé T15-T17

| Tier | Feature | LOC | Priorité | Impact |
|------|---------|-----|----------|--------|
| T15.1 | Mode "list" concepts/labels | ~30 | **P0** | Élimine le bug de corruption |
| T15.2 | Validation stricte des modes | ~10 | **P0** | Empêche les catch-all silencieux |
| T15.3 | ai_thread_inspect | ~25 | P1 | Vue complète read-only |
| T15.4 | Dry-run pour metadata ops | ~60 | P1 | Preview avant écriture |
| T16.1 | ai_concepts_suggest | ~30 | P2 | Concepts LLM-assisted |
| T16.2 | ai_concepts_health | ~30 | P1 | Dashboard concept coverage |
| T16.3 | ai_concepts_migrate | ~20 | P1 | Migration one-time |
| T17.1 | Schemas enrichis (enum) | ~150 | **P0** | Les agents voient les valeurs valides |
| T17.2 | Descriptions longues | ~205 | P2 | Documentation inline |
| T17.3 | Undo buffer | ~40 | P1 | Rollback accidentel |
| **Total** | | **~600** | | |

**Sprint prioritaire recommandé:**
```
Sprint imm (anti-corruption, 0 régression possible):
  T15.1 Mode "list" default     — 30 LOC  ← élimine LE bug
  T15.2 Validation modes        — 10 LOC  ← catch-all → erreur
  T17.1 Schemas enrichis (top5) — 30 LOC  ← ai_concepts, ai_label, ai_profile, ai_backup, ai_concepts

Sprint next:
  T15.3 ai_thread_inspect       — 25 LOC
  T16.2 ai_concepts_health      — 30 LOC
  T17.3 Undo buffer             — 40 LOC

Sprint later:
  T15.4 Dry-run                 — 60 LOC
  T16.1 ai_concepts_suggest     — 30 LOC
  T16.3 ai_concepts_migrate     — 20 LOC
  T17.1 Schemas enrichis (rest) — 120 LOC
  T17.2 Descriptions longues    — 205 LOC
```

---

## T18 — Uniformisation sémantique des tools MCP (v4.4.0)

**Origine:** Audit complet des 13 fichiers handlers dans `src/mcp/tools/`. Incohérences systémiques identifiées entre tools sémantiquement similaires.

### T18.1 — Harmonisation `mode` vs `action`

**Problème:** Certains tools utilisent `mode`, d'autres `action` pour le même concept (choisir l'opération à effectuer).

**État actuel:**
| Tool | Paramètre | Valeurs |
|------|-----------|---------|
| `ai_concepts` | `mode` | add, set, remove (+ catch-all) |
| `ai_label` | `mode` | add, set, remove (+ catch-all) |
| `ai_profile` | `action` | view, set_rule, remove_rule, list, clear_rules |
| `agent_tasks` | `action` | list, create, status, assign, complete |
| `ai_backup` | `action` | create, restore, auto_schedule |

**Convention proposée:**
- `mode` = pour les opérations CRUD sur une propriété d'un objet existant (labels, concepts)
- `action` = pour les opérations lifecycle / multi-objet (tasks, backup, profile)

Ou mieux — **tout unifier sur `mode`** puisque c'est la sémantique MCP standard et que 2/3 des tools l'utilisent déjà.

**Changements:**
```rust
// ai_profile: action → mode
let mode = optional_str(params, "mode")
    .or_else(|| optional_str(params, "action"))  // backward compat
    .unwrap_or_else(|| "view".into());

// agent_tasks: action → mode (backward compat)
let mode = optional_str(params, "mode")
    .or_else(|| optional_str(params, "action"))  // backward compat
    .unwrap_or_else(|| "list".into());

// ai_backup: action → mode (backward compat)
let mode = optional_str(params, "mode")
    .or_else(|| optional_str(params, "action"))  // backward compat
    .unwrap_or_else(|| "create".into());
```

**Stratégie migration:** Accepter `mode` ET `action` pendant 2 versions (backward compat), log un warning quand `action` est utilisé, puis supprimer `action` en v5.0.

**Estimation:** ~15 LOC + tool_def updates

### T18.2 — Harmonisation des defaults (read-first)

**Problème:** Les defaults sont incohérents ET dangereux :

| Tool | Default actuel | Dangereux ? |
|------|---------------|-------------|
| `ai_concepts` | `"set"` (écrit!) | **OUI** — écrase tout |
| `ai_label` | `"add"` (écrit!) | **OUI** — ajoute sans demander |
| `ai_profile` | `"view"` (lecture) | Non, safe |
| `agent_tasks` | `"list"` (lecture) | Non, safe |
| `ai_backup` | `"create"` (écrit) | Modéré — crée un backup |

**Convention proposée — "Read-first by default":**
> Tout tool appelé sans `mode` explicite doit être **read-only**.
> Une écriture nécessite TOUJOURS un mode explicite.

| Tool | Nouveau default | Comportement |
|------|----------------|--------------|
| `ai_concepts` | `"list"` | Retourne les concepts actuels |
| `ai_label` | `"list"` | Retourne les labels actuels |
| `ai_profile` | `"view"` | (inchangé, déjà safe) |
| `agent_tasks` | `"list"` | (inchangé, déjà safe) |
| `ai_backup` | `"status"` | Retourne le dernier backup + schedule |

**Estimation:** ~10 LOC

### T18.3 — Harmonisation confirm/dry_run

**Problème:** Deux patterns coexistent pour la même chose :

| Pattern | Tools | Sémantique |
|---------|-------|------------|
| `confirm: bool` (default false) | thread_activate, thread_suspend, thread_purge, split, bridge_scan_orphans, bridge_purge | Sans confirm → preview |
| `dry_run: bool` (default true) | backfill_concepts | Sans dry_run → preview |
| (aucun) | label, concepts, rename, rate_importance, thread_rm, bridge_kill | Écriture immédiate, 0 preview |

**Convention proposée — unifier sur `confirm`:**
- `confirm=false` (ou absent) → dry-run / preview
- `confirm=true` → exécution réelle

Raison : `confirm` est déjà le pattern dominant (6 tools vs 1 pour dry_run). Et sémantiquement, "confirmer" est plus naturel pour un agent LLM que "ne pas faire le dry run".

**Migration `backfill_concepts`:**
```rust
// Accepter les deux pendant 2 versions
let execute = optional_bool(params, "confirm").unwrap_or(false)
    || !optional_bool(params, "dry_run").unwrap_or(true);
```

**Estimation:** ~5 LOC (backfill_concepts) + ~60 LOC (ajouter confirm aux tools qui n'en ont pas)

### T18.4 — Harmonisation des réponses JSON

**Problème:** Les réponses des tools n'ont pas de format cohérent.

**Exemples de divergence:**
```json
// ai_concepts retourne:
{"thread_id": "xxx", "concepts": [...]}

// ai_label retourne:
{"thread_id": "xxx", "labels": [...]}

// ai_thread_activate retourne (dry-run):
{"dry_run": true, "thread_id": "xxx", "current_status": "...", "action": "..."}

// ai_thread_purge retourne (dry-run):
{"dry_run": true, "count": 5, "threads": [...]}
```

**Convention proposée — enveloppe standard:**
```json
{
    "ok": true,
    "action": "list|add|set|remove|create|...",
    "dry_run": false,
    "data": { /* résultat spécifique au tool */ }
}
```

Pour les preview/dry-run:
```json
{
    "ok": true,
    "action": "set",
    "dry_run": true,
    "data": {
        "current": [...],
        "proposed": [...],
        "diff": {"added": [...], "removed": [...]}
    }
}
```

**Impact:** Facilite le parsing côté agent. Un agent peut toujours checker `result.ok` et `result.dry_run` sans connaître le tool spécifique.

**Estimation:** ~100 LOC (wrapper function + migration progressive)
**Note:** Migration progressive — les nouveaux tools utilisent le format, les anciens sont migrés par vagues.

### T18.5 — Table de référence sémantique

Résumé des conventions à appliquer uniformément :

| Aspect | Convention | Raison |
|--------|-----------|--------|
| Paramètre d'opération | `mode` (partout) | Standard MCP, déjà dominant |
| Default mode | Read-only (`"list"`, `"view"`, `"status"`) | Sécurité — jamais d'écriture silencieuse |
| Mode invalide | `Err(InvalidInput)` | Pas de catch-all `_` qui écrit |
| Preview | `confirm: bool` (default false) | Pattern dominant, sémantiquement clair |
| Params write-only | Optionnels | `concepts` n'est requis que si mode != "list" |
| Réponse JSON | `{ok, action, dry_run, data}` | Parsing uniforme côté agent |
| Description | Courte + longue | Courte dans MCP, longue dans ai_help |
| Enum values | Dans inputSchema | L'agent LLM voit les valeurs valides |

#### Convention `list` vs `view` — deux sémantiques distinctes

| Mode | Sémantique | Retourne | Exemples |
|------|-----------|----------|----------|
| `"list"` | Collection d'items homogènes | **Array** | `ai_concepts` → `["rust", "memory"]`, `ai_label` → `["bug", "P0"]`, `agent_tasks` → `[{task1}, {task2}]` |
| `"view"` | Vue complète d'un objet unique | **Objet structuré** | `ai_profile` → `{name, rules, ...}`, `ai_thread_inspect` → `{id, title, concepts, ...}` |

**Règle :** Si le mode read-only retourne une propriété array d'un objet → `"list"`. Si il retourne l'objet entier → `"view"`.

**Application concrète :**
| Tool | Default mode | Raison |
|------|-------------|--------|
| `ai_concepts` | `"list"` | Retourne `thread.concepts` (array) |
| `ai_label` | `"list"` | Retourne `thread.labels` (array) |
| `ai_profile` | `"view"` | Retourne le profil complet (objet) |
| `ai_thread_inspect` | `"view"` | Retourne le thread complet (objet) |
| `agent_tasks` | `"list"` | Retourne la liste des tâches (array) |
| `ai_bridges` | `"list"` | Retourne les bridges (array) — déjà read-only implicitement |
| `ai_backup` | `"status"` | Ni list ni view — retourne l'état du système backup |
| `ai_status` | (pas de mode) | Tool purement read-only, 0 ambiguïté |

**Cas limites :**
- `ai_thread_list` / `ai_thread_search` → pas de `mode`, déjà read-only → aucun changement
- `ai_bridges` → déjà read-only (pas de mode), pas besoin d'ajouter mode
- Les tools qui sont **exclusivement** read-only (ai_status, ai_bridge_analysis, ai_help) n'ont pas besoin de mode

---

## T19 — Refonte ai_help (v4.4.0)

**Origine:** Le ai_help actuel retourne un JSON minimaliste statique qui ne mentionne ni les modes, ni les paramètres, ni les pièges. Combiné avec les incohérences sémantiques (T18), les agents opèrent à l'aveugle.

### T19.1 — ai_help contextuel par catégorie

**État actuel:** `handle_help()` retourne un JSON statique avec la liste des tools groupés par catégorie. Pas de documentation par tool.

**Proposition:** ai_help structuré en 3 niveaux :

1. `ai_help()` — overview des catégories + quick reference
2. `ai_help(topic="memory")` — tools de la catégorie avec params et modes
3. `ai_help(tool="ai_concepts")` — documentation complète d'un tool spécifique

**Nouveau paramètre `tool` (optionnel):**
```rust
pub fn handle_help(
    params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let topic = optional_str(params, "topic");
    let tool_name = optional_str(params, "tool");

    // Level 3: Single tool documentation
    if let Some(name) = tool_name {
        return Ok(help_for_tool(&name));
    }
    // Level 2: Category documentation
    if let Some(t) = topic {
        return match t.as_str() {
            "memory" => Ok(help_memory()),
            "messaging" => Ok(help_messaging()),
            "threads" => Ok(help_threads()),
            "bridges" => Ok(help_bridges()),
            "agents" => Ok(help_agents()),
            "sharing" => Ok(help_sharing()),
            "maintenance" => Ok(help_maintenance()),
            _ => Ok(help_overview()),
        };
    }
    // Level 1: Overview
    Ok(help_overview())
}
```

### T19.2 — Documentation par tool (help_for_tool)

**Exemple pour ai_concepts:**
```rust
fn help_ai_concepts() -> serde_json::Value {
    serde_json::json!({
        "tool": "ai_concepts",
        "category": "threads",
        "description": "View or modify semantic concepts on a thread",
        "modes": {
            "list": {
                "description": "View current concepts (DEFAULT, read-only)",
                "requires": ["thread_id"],
                "example": {"thread_id": "abc123"}
            },
            "add": {
                "description": "Add concepts to existing ones",
                "requires": ["thread_id", "concepts"],
                "example": {"thread_id": "abc123", "concepts": ["rust", "memory"], "mode": "add"}
            },
            "set": {
                "description": "Replace all concepts",
                "requires": ["thread_id", "concepts"],
                "example": {"thread_id": "abc123", "concepts": ["rust", "memory"], "mode": "set"}
            },
            "remove": {
                "description": "Remove specific concepts",
                "requires": ["thread_id", "concepts"],
                "example": {"thread_id": "abc123", "concepts": ["old"], "mode": "remove"}
            }
        },
        "options": {
            "dry_run": "Preview changes without applying (boolean, default false)"
        },
        "warnings": [
            "concepts parameter is ONLY required for add/set/remove modes",
            "Calling without mode defaults to 'list' (read-only, safe)",
            "Concepts are auto-normalized: multi-word phrases split, stopwords removed, lowercased"
        ],
        "related": ["ai_concepts_suggest", "ai_concepts_health", "ai_backfill_concepts"]
    })
}
```

### T19.3 — Warnings et pièges courants dans help_overview

```rust
fn help_overview() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "categories": { /* ... existing ... */ },
        "conventions": {
            "mode_parameter": "All tools use 'mode' to select operation. Default is always read-only.",
            "confirm_parameter": "Destructive operations require confirm=true. Without it, you get a preview.",
            "dry_run": "Alias for confirm=false. Shows what would change without applying.",
        },
        "common_mistakes": [
            "ai_concepts: do NOT pass concepts when you want to view — use mode='list' (default)",
            "ai_label: do NOT pass labels when you want to view — use mode='list' (default)",
            "ai_thread_purge: always preview first (no confirm), then confirm=true",
            "ai_split: requires message_groups and titles only when confirm=true",
        ],
        "tips": [
            "Use ai_thread_inspect for a complete read-only view of a thread",
            "Use ai_concepts_health to see concept coverage across all threads",
            "Use ai_bridge_analysis to check bridge network health",
            "Short thread IDs (8 chars) work everywhere — no need for full UUIDs",
        ]
    })
}
```

### T19.4 — Help auto-injecté dans l'onboarding

**Problème:** Même avec un ai_help complet, les agents ne l'appellent pas. Le rappel proactif (T1.2) aide, mais le premier contact est critique.

**Proposition:** Inclure un résumé ultra-court dans le Layer 0 (onboarding) du hook inject :

```rust
// In build_onboarding_prompt(), add:
"Quick reference: ai_concepts(thread_id) → view concepts. ai_label(thread_id) → view labels. \
 All tools default to read-only. Use mode='set' to write. Call ai_help for full docs."
```

**Estimation:** ~3 LOC dans inject.rs

---

## Résumé T18-T19

| Tier | Feature | LOC | Priorité |
|------|---------|-----|----------|
| T18.1 | mode vs action harmonisation | ~15 | P1 |
| T18.2 | Read-first defaults | ~10 | **P0** |
| T18.3 | confirm/dry_run unification | ~65 | P1 |
| T18.4 | Réponses JSON standard | ~100 | P2 |
| T18.5 | (convention, 0 LOC) | 0 | **P0** |
| T19.1 | ai_help 3-niveaux | ~30 | P1 |
| T19.2 | Doc par tool (67 tools) | ~300 | P2 (itératif) |
| T19.3 | Warnings/pièges dans overview | ~20 | **P0** |
| T19.4 | Help onboarding inject | ~3 | P1 |
| **Total T18-T19** | | **~543** | |

**Sprint combiné recommandé avec T15-T17:**
```
Sprint 1 — Anti-corruption (v4.4.0-alpha):
  T15.1 Mode "list" concepts/labels          — 30 LOC
  T15.2 Validation stricte modes             — 10 LOC
  T18.2 Read-first defaults                  — 10 LOC
  T18.5 Convention table (documentation)     — 0 LOC
  T19.3 Warnings dans help overview          — 20 LOC
  Total: ~70 LOC

Sprint 2 — Ergonomics (v4.5.0-alpha):
  T17.1 Schemas enrichis (enum values)       — 150 LOC
  T18.1 mode/action harmonisation            — 15 LOC
  T18.3 confirm/dry_run unification          — 65 LOC
  T19.1 ai_help 3-niveaux                    — 30 LOC
  T19.4 Help onboarding inject               — 3 LOC
  Total: ~263 LOC

Sprint 3 — Intelligence (v4.6.0-alpha):
  T15.3 ai_thread_inspect                    — 25 LOC
  T15.4 Dry-run metadata ops                 — 60 LOC
  T16.2 ai_concepts_health                   — 30 LOC
  T17.3 Undo buffer                          — 40 LOC
  Total: ~155 LOC

Sprint 4 — Documentation complète (v4.7.0-alpha):
  T18.4 Réponses JSON standard               — 100 LOC
  T19.2 Doc par tool (67 tools, itératif)    — 300 LOC
  T16.1 ai_concepts_suggest                  — 30 LOC
  T16.3 ai_concepts_migrate                  — 20 LOC
  T17.2 Descriptions longues                 — 205 LOC
  Total: ~655 LOC
```

---

# T20 — Pool / Worker Pipeline Optimization

**Source :** Audit v4.3.0 — observation terrain que seul 1 worker est actif malgré 4 threads spawned.

## Diagnostic

### Architecture actuelle

```
Hook capture → IPC → CaptureQueue (N workers, default min(cpu,4))
                            |
                            +── is_prompt=true (RARE):
                            |     → processor::process_capture()
                            |     → LLM extraction (Mutex<LlamaContext> ~25-50s)
                            |     → coherence check (même Mutex)
                            |
                            +── is_prompt=false (COMMON):
                                  → pool_writer::append() (~1ms filesystem)
                                  → .pending file
                                  → pool_consumer (1 SEUL thread, scan 10s)
                                    → process_pending_files() séquentiel
                                    → LLM extraction (même Mutex global)
```

### 3 points de sérialisation identifiés

| # | Point | Fichier | Sévérité | Cause |
|---|-------|---------|----------|-------|
| 1 | **LLM Singleton Mutex** | `src/processing/local_llm.rs:53` | FATAL | `persistent_ctx: Mutex<Option<LlamaContext>>` — 1 seul contexte VRAM (GTX 1650, 4GB). Intentionnel pour éviter race condition Vulkan dealloc/realloc. |
| 2 | **Pool Consumer mono-thread** | `src/daemon/periodic_tasks.rs:281` | SÉVÈRE | 1 seul thread consomme TOUS les `.pending` de TOUS les agents, séquentiellement, scan 10s. |
| 3 | **Mutex-wrapped Receiver** | `src/daemon/capture_queue.rs:72` | MINEUR | `Arc<Mutex<Receiver>>` — pattern Rust standard (mpsc Receiver !Sync). Lock relâché vite. |

### Facteur atténuant : Changelog Shortcut

Le content hash (`thread_manager.rs:708-724`) permet au **changelog shortcut** de skip le LLM pour les fichiers déjà vus. Quand un agent fait beaucoup de Read sur des fichiers connus, la majorité des captures passent sans inférence → traitement quasi-instantané → 1 worker suffit dans ce cas.

**Ce n'est pas un bug mais une optimisation qui fonctionne.** Le multi-worker serait utile uniquement pour les captures nécessitant le LLM.

---

## T20.1 — Pool Consumer multi-agent parallèle

**Problème :** `run_pool_consumer_loop` traite tous les agents séquentiellement dans 1 thread.

**Solution :** Spawner un thread (ou task) par agent découvert, traiter les `.pending` en parallèle par agent. Les agents ont des DB et pool_dir séparés → aucune contention.

```rust
// Avant (periodic_tasks.rs:281)
for key in &keys {
    process_pending_files(key, ...);  // séquentiel
}

// Après
let handles: Vec<_> = keys.iter().map(|key| {
    let key = key.clone();
    let pool = pool.clone();
    std::thread::spawn(move || {
        process_pending_files(&key, &pool, ...);
    })
}).collect();
for h in handles { h.join().ok(); }
```

**Impact :** Parallélisme inter-agent. Si 3 agents ont des pending, 3 threads travaillent en parallèle (toujours limités par le LLM Mutex, mais le filesystem I/O et le DB write se parallélisent).

**Estimation :** ~25 LOC

---

## T20.2 — Réduire `capture_workers` default à 2

**Problème :** 4 threads spawned mais 3 sont toujours bloqués sur le LLM Mutex → gaspillage.

**Solution :** `default_capture_workers()` → `min(cpu_cores, 2)`. Avec le LLM singleton, plus de 2 workers n'apporte rien (1 en inférence + 1 en attente prêt à prendre le relais).

**Estimation :** ~3 LOC (config.rs)

---

## T20.3 — Remplacer `Arc<Mutex<Receiver>>` par `crossbeam-channel`

**Problème :** `std::sync::mpsc::Receiver` n'est pas `Sync`, d'où le Mutex wrapper. Sérialisation mineure du dequeue.

**Solution :** `crossbeam-channel::bounded(capacity)` → le Receiver est `Sync`, multi-consumer natif sans Mutex.

```toml
# Cargo.toml
crossbeam-channel = "0.5"
```

**Estimation :** ~15 LOC (capture_queue.rs) + 1 dep

---

## T20.4 — LLM Batching (long terme, GPU upgrade requis)

**Problème :** Le Mutex LLM est le bottleneck fondamental. Avec 1 GPU 4GB, impossible de paralléliser.

**Options futures :**
- **GPU 8GB+** : Plusieurs contextes VRAM simultanés → vrai parallélisme
- **Batching** : Accumuler N prompts, inférer en batch (nécessite support llama-cpp batch API)
- **CPU fallback pool** : Dédier le GPU au prompt principal, router les coherence checks vers un modèle CPU léger en parallèle

**Estimation :** Architecture-level, non chiffrable. Dépend du hardware.

---

## T20.5 — Pool Consumer scan adaptatif

**Problème :** Scan fixe toutes les 10s, même quand il y a 50 pending files.

**Solution :** Scan adaptatif — si le dernier scan a trouvé des fichiers, scanner immédiatement. Sinon, backoff progressif (1s → 5s → 10s → 30s idle).

```rust
let mut interval = Duration::from_secs(1);
loop {
    std::thread::sleep(interval);
    let processed = scan_and_process();
    interval = if processed > 0 {
        Duration::from_secs(1)  // busy: scan vite
    } else {
        (interval * 2).min(Duration::from_secs(30))  // idle: backoff
    };
}
```

**Estimation :** ~15 LOC

---

## Résumé T20

| Tier | Feature | LOC | Priorité |
|------|---------|-----|----------|
| T20.1 | Pool consumer multi-agent | ~25 | P1 |
| T20.2 | Réduire workers default à 2 | ~3 | P1 |
| T20.3 | crossbeam-channel | ~15 | P2 |
| T20.4 | LLM batching (hardware) | — | P3 (long terme) |
| T20.5 | Scan adaptatif | ~15 | P1 |
| **Total T20** | | **~58** | |

**Sprint recommandé :** v4.8.0-alpha (T20.1 + T20.2 + T20.5 = ~43 LOC, quick win)

---

## Backlog — Besoins identifiés (non triés)

- **ai_thread_list : exposer work_context** — l'API thread_list/recall ne retourne pas le `work_context` (files trackés, etc.). Nécessaire pour diagnostiquer le changelog shortcut et l'absorption des captures sans passer par la DB directement.

# T21 - Continuity edges - fil logique de raisonnement inter entité **Done**

Ce que je cherche c est à générer un nouveau type de edges de "continuité"(qui doit etre visible dans le graph) qui dès la création du thread le lie à un autre thread .par exemple je te prompt (thread_prompt1 = orphan car coherence gate detecte un nouveau sujet ) tu réponds en relation (thread_response1= continuity_edge = to thread_prompt1) je te répond en validant une action (thread_prompt2= continuity_edge= to thread_response1) tu engages une revue de code en reaction à thread_prompt2 ( thread_read1= continuity edge = to thread_prompt2) etc etc ... le but étant de pouvoir donner à l agent la capacité de remonter le fil logique quand il consulte un thread sous sa propre initiative ou par proposition du engram.

les bridges doivent rester independant de ce type de edges :
continuity edges = fil logique de raisonnement humain<->agent ou agent<->agent
bridge = capacité de liaison semantique et/ou conceptuel (fais  des recherche si besoin dans le code au sujet des Thinkbridgeq) par overlapping .

dans le cadre d une action read/edit/write de la part de l agent il est possible que le fichier a deja été lu auparavent et passe non pas en traitement llm mais en changelog .Dans ce cas de l edit qui est fait sur le thread, ce dernier doit comporter un champ continuity=from <thread_id> to <thread_id> pour que l agent puisse continuer la remontée ou descente sequentielle au travers de ce thread eyant plusieurs edit d action qu il a opérer dessus.

exemple d thread traiter en changelog :

Status: Active   Weight: 1.00   Importance: 1.00   Activations: 2
Topics: agent_data, beat_state, quota, thread_quota, config, AI, thread management, processing, coherence gate, debugging   Labels: architecture, config, test-output, issue, diagnosis
Created: 2026-03-01T09:14:18.597833488+00:00   Last active: 2026-03-01T09:14:18.638743852+00:00
Summary:coherence.rs file's interaction
continuity_parent_id:<thread_id> subject_coherence:0.6

[Read] 2026-02-27T09:14:18.597833488+00:00
/home/vzcrow/Dev/ai_smartness_dev/ai-smartness/src/processing/coherence.rs
continuity=from <thread_id> to <thread_id>

[Read] 2026-02-28T17:44:21.785933598+00:00
/home/vzcrow/Dev/ai_smartness_dev/ai-smartness/src/processing/coherence.rs
continuity=from <thread_id> to <thread_id>

[Edit] 2026-03-01T05:16:53.335785998+00:00
/home/vzcrow/Dev/ai_smartness_dev/ai-smartness/src/processing/coherence.rs
continuity=from <thread_id> to <thread_id>

[Write] 2026-03-01T08:02:33.783359859+00:00
/home/vzcrow/Dev/ai_smartness_dev/ai-smartness/src/processing/coherence.rs
continuity=from <thread_id> to <thread_id>