# Plan de remédiation : Proactivité cognitive des agents

**Statut** : Validé (dev + pub alignés, cor approuvé)
**LOC total** : ~265 | **Fichiers** : 7 | **Risque global** : LOW

---

## Diagnostic

Le problème n'est pas mécanique (daemon, gossip, decay, engram = solides). Le problème est **comportemental** : les agents ont les outils mais aucune instruction pour s'en servir proactivement.

---

## Phase A1 — Onboarding comportemental (CRITIQUE)

**But** : Ajouter des directives comportementales à l'onboarding, pas juste une liste d'outils.

**Fichier** : `src/hook/inject.rs`
**Fonction** : `build_onboarding_prompt()` (ligne 546)
**LOC** : ~60

### Changement

**Avant** (lignes 554-580) : Le payload d'onboarding liste uniquement les outils MCP disponibles (ai_status, ai_search, ai_threads, ai_recall, ai_pin, ai_merge, etc.) sans directive comportementale.

**Après** : Ajouter un bloc **Cognitive Autonomy Rules** après la liste d'outils existante :

```rust
// Insertion dans le String de l'onboarding payload, après la section outils existante.
// Ajout à ~ligne 578 (avant le write du sentinel file).

let behavioral_rules = r#"
## Cognitive Autonomy Rules

You are responsible for maintaining your own memory. This is not optional.

### Thread Management
- When you encounter an important decision, pattern, or insight: call ai_thread_create
- When you notice threads with overlapping content: call ai_merge
- When a thread is no longer relevant: call ai_thread_suspend

### Context Enrichment
- At the START of complex tasks: call ai_recall with relevant keywords
- After completing a task: rate thread importance with ai_rate_importance
- Periodically: call ai_suggestions to check for maintenance tasks

### Labeling & Organization
- Always label threads you create (ai_label)
- Add concepts to threads missing them (ai_concepts)
- Use ai_focus when working on a specific topic for extended periods

### Communication
- When you discover something relevant for another agent: use ai_msg_focus
- When you finish a task delegated by another: reply via msg_reply
- Schedule follow-ups with beat_wake when needed

### When to create threads manually
The capture pipeline handles most content automatically, but you should manually
create threads (ai_thread_create) when:
- A user makes an architectural decision
- You discover a bug pattern or anti-pattern
- A conversation produces insights not captured by tool outputs
- You receive requirements or constraints that should persist
"#;
```

---

## Phase A2 — Cognitive nudge récurrent (CRITIQUE)

**But** : Injecter un rappel comportemental conditionnel à chaque prompt (Layer 1.7).

**Fichiers** :
- `src/hook/inject.rs` : nouvelle fonction + insertion dans `run()` (~80 lignes)
- `src/storage/beat.rs` : nouveaux champs dans `BeatState` (~10 lignes)

**LOC** : ~90

### Changement 1 — beat.rs : ajouter champs de tracking

**Fichier** : `src/storage/beat.rs`
**Struct** : `BeatState` (ligne 22)

**Avant** (lignes 22-51) :
```rust
pub struct BeatState {
    pub beat: u64,
    pub started_at: String,
    pub last_beat_at: String,
    pub last_interaction_at: String,
    pub last_interaction_beat: u64,
    pub last_session_id: Option<String>,
    pub last_thread_id: Option<String>,
    pub pid: Option<u32>,
    pub cli_pid: Option<u32>,
    pub scheduled_wakes: Vec<ScheduledWake>,
    pub context_tokens: Option<u64>,
    pub context_percent: Option<f64>,
    pub context_updated_at: Option<String>,
    pub current_activity: String,
}
```

**Après** : Ajouter 4 champs (avec `#[serde(default)]` pour compatibilité rétro) :
```rust
pub struct BeatState {
    // ... champs existants inchangés ...

    /// Cognitive nudge tracking
    #[serde(default)]
    pub last_nudge_type: String,          // "recall" | "capacity" | "unlabeled" | "maintenance"
    #[serde(default)]
    pub last_nudge_beat: u64,             // beat du dernier nudge émis
    #[serde(default)]
    pub last_maintenance_beat: u64,       // beat du dernier nudge maintenance
    #[serde(default)]
    pub last_recall_beat: u64,            // beat du dernier appel ai_recall (mutualisé avec B2)
}
```

### Changement 2 — inject.rs : nouvelle fonction + insertion Layer 1.7

**Fichier** : `src/hook/inject.rs`
**Point d'insertion** : après Layer 1.5 (ligne 149), avant Layer 2 (ligne 151)

**Nouvelle fonction** :
```rust
/// Layer 1.7: Cognitive nudge — conditional maintenance reminder.
/// Design anti-bruit: max 1 nudge per prompt, cooldown 10 beats per type, 300 chars max.
fn build_cognitive_nudge(
    conn: &Connection,
    beat_state: &BeatState,
    project_hash: &str,
    agent_id: &str,
) -> Option<String> {
    let beat = beat_state.beat;
    let cooldown = 10u64;

    // Priority-ordered conditions — first match wins
    let nudge = if beat_state.last_recall_beat + 10 < beat {
        let active = ThreadStorage::count(conn).unwrap_or(0);
        if active > 10 && (beat_state.last_nudge_type != "recall" || beat_state.last_nudge_beat + cooldown <= beat) {
            Some(("recall", format!(
                "You haven't used ai_recall in {} prompts and have {} active threads. Search memory for relevant context.",
                beat - beat_state.last_recall_beat, active
            )))
        } else { None }
    } else { None };

    let nudge = nudge.or_else(|| {
        let active = ThreadStorage::count(conn).unwrap_or(0);
        if active > 40 && (beat_state.last_nudge_type != "capacity" || beat_state.last_nudge_beat + cooldown <= beat) {
            Some(("capacity", format!(
                "You have {} active threads (high). Review and suspend obsolete ones with ai_thread_suspend.",
                active
            )))
        } else { None }
    });

    let nudge = nudge.or_else(|| {
        let active = ThreadStorage::count(conn).unwrap_or(0);
        let unlabeled = ThreadStorage::count_unlabeled(conn).unwrap_or(0);
        if active > 5 {
            let ratio = unlabeled as f64 / active as f64;
            if ratio > 0.4 && (beat_state.last_nudge_type != "unlabeled" || beat_state.last_nudge_beat + cooldown <= beat) {
                Some(("unlabeled", format!(
                    "You have {} unlabeled threads ({:.0}%). Consider running ai_label on important ones.",
                    unlabeled, ratio * 100.0
                )))
            } else { None }
        } else { None }
    });

    let nudge = nudge.or_else(|| {
        if beat > beat_state.last_maintenance_beat + 50
            && (beat_state.last_nudge_type != "maintenance" || beat_state.last_nudge_beat + cooldown <= beat)
        {
            Some(("maintenance", "Run ai_suggestions and address any findings. Check threads missing labels or concepts.".to_string()))
        } else { None }
    });

    if let Some((nudge_type, message)) = nudge {
        // Update beat state (will be saved by caller)
        // Note: caller must save beat_state after this
        let truncated = if message.len() > 300 { &message[..300] } else { &message };
        Some(format!("Cognitive maintenance: {}", truncated))
    } else {
        None
    }
}
```

**Insertion dans run()** — après ligne 149 :
```rust
    // Layer 1.7: Cognitive nudge (conditional maintenance reminder)
    if let Some(nudge) = build_cognitive_nudge(&conn, &beat_state, project_hash, agent_id) {
        let layer = format!("<system-reminder>\n{}\n</system-reminder>", nudge);
        if layer.len() < budget {
            budget -= layer.len();
            injections.push(layer);
            // Update nudge tracking in beat_state
            beat_state.last_nudge_type = nudge_type.to_string();
            beat_state.last_nudge_beat = beat_state.beat;
            if nudge_type == "maintenance" {
                beat_state.last_maintenance_beat = beat_state.beat;
            }
            beat_state.save(&agent_data);
            tracing::info!(nudge_type, "Layer 1.7: Cognitive nudge injected");
        }
    }
```

---

## Phase B1 — Auto-push suggestions Medium (HAUTE)

**But** : Inclure les findings Medium dans l'injection HealthGuard (pas seulement High/Critical).

**Fichier** : `src/hook/inject.rs`
**Fonction** : `build_healthguard_injection()` (ligne 660)
**LOC** : ~15

### Changement

**Avant** (dans build_healthguard_injection, partition des findings) :
```rust
// Seuls High et Critical sont injectés
let (injectable, _suggestible) = partition_findings(&findings);
```

**Après** :
```rust
// High/Critical : toujours injectés
// Medium : injecté toutes les 10 prompts (beat % 10 == 0)
let beat = BeatState::load(&agent_data_dir).beat;
let (high_critical, medium, _low) = partition_findings_by_priority(&findings);

let mut injectable = high_critical;
if beat % 10 == 0 {
    injectable.extend(medium);
}
```

Note : `partition_findings_by_priority()` est une nouvelle helper (3 lignes) qui split en 3 vecteurs au lieu de 2.

---

## Phase B2 — HealthGuard recall check + tracking (HAUTE)

**But** : Détecter quand l'agent n'utilise pas ai_recall et le signaler.

**Fichiers** :
- `src/healthguard/checks.rs` : nouveau check (~30 lignes)
- `src/storage/beat.rs` : champ `last_recall_beat` (déjà ajouté en A2)
- `src/mcp/tools/recall.rs` : écrire `last_recall_beat` (~10 lignes)

**LOC** : ~45

### Changement 1 — checks.rs : nouveau check

**Fichier** : `src/healthguard/checks.rs`
**Point d'insertion** : après `check_disk_usage()` (ligne 166)

```rust
/// Check if agent hasn't used ai_recall recently despite having rich memory.
pub fn check_recall_staleness(
    conn: &Connection,
    config: &HealthGuardConfig,
    beat_state: &BeatState,
) -> Option<HealthFinding> {
    let active = ThreadStorage::count(conn).ok()?;
    if active < 10 {
        return None; // Not enough threads to warrant recall
    }

    let beats_since_recall = beat_state.beat.saturating_sub(beat_state.last_recall_beat);
    let threshold = 15u64; // beats (~75 minutes)

    if beats_since_recall > threshold {
        Some(HealthFinding {
            priority: HealthPriority::Medium,
            category: "recall_staleness".to_string(),
            message: format!(
                "No ai_recall usage in {} prompts with {} active threads. Memory context may be stale.",
                beats_since_recall, active
            ),
            action: "Run ai_recall with keywords related to your current task.".to_string(),
            metric_value: beats_since_recall as f64,
            threshold: threshold as f64,
        })
    } else {
        None
    }
}
```

### Changement 2 — recall.rs : écrire last_recall_beat

**Fichier** : `src/mcp/tools/recall.rs`
**Fonction** : `handle_recall()` (ligne 8)
**Point d'insertion** : après le recall réussi, avant le return (vers ligne 55)

```rust
// Update last_recall_beat in beat state
let agent_data = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
let mut beat_state = BeatState::load(&agent_data);
beat_state.last_recall_beat = beat_state.beat;
beat_state.save(&agent_data);
```

---

## Phase C1 — Synthèse de compaction enrichie (MOYENNE)

**But** : Remplir `key_insights` et `open_questions` dans SynthesisReport.

**Fichier** : `src/hook/compact.rs`
**Fonction** : `generate_synthesis()` (ligne 32)
**LOC** : ~40

### Changement

**Avant** (lignes 56-63) :
```rust
    let synthesis = SynthesisReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.to_string(),
        active_work,
        key_insights: Vec::new(),       // line 61 — placeholder
        open_questions: Vec::new(),     // line 62 — placeholder
    };
```

**Après** :
```rust
    // Key insights: threads with high importance + active pins
    let key_insights: Vec<String> = threads.iter()
        .filter(|t| t.importance >= 0.7)
        .take(5)
        .map(|t| format!("[importance={:.1}] {}: {}", t.importance, t.title,
            t.summary.as_deref().unwrap_or("").chars().take(100).collect::<String>()))
        .collect();

    // Open questions: threads tagged __focus__ (active investigation topics)
    let open_questions: Vec<String> = threads.iter()
        .filter(|t| t.tags.as_deref().unwrap_or("").contains("__focus__"))
        .take(5)
        .map(|t| format!("{}: {}", t.title,
            t.summary.as_deref().unwrap_or("(no summary)").chars().take(100).collect::<String>()))
        .collect();

    // Also include active pins as insights
    let pins_path = agent_data.join("pins.json");
    if let Ok(pins_content) = std::fs::read_to_string(&pins_path) {
        if let Ok(pins) = serde_json::from_str::<serde_json::Value>(&pins_content) {
            if let Some(pin_array) = pins.get("pins").and_then(|p| p.as_array()) {
                for pin in pin_array.iter().take(3) {
                    if let Some(content) = pin.get("content").and_then(|c| c.as_str()) {
                        key_insights.push(format!("[pin] {}", &content[..content.len().min(100)]));
                    }
                }
            }
        }
    }

    let synthesis = SynthesisReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.to_string(),
        active_work,
        key_insights,
        open_questions,
    };
```

**Aussi modifier** `format_for_injection()` (ligne 88) pour inclure insights/questions dans le rendu :
```rust
fn format_for_injection(synthesis: &SynthesisReport) -> String {
    let mut out = String::from("Context synthesis (pre-compaction snapshot):\n");
    // ... existing active_work rendering ...

    if !synthesis.key_insights.is_empty() {
        out.push_str("\nKey insights:\n");
        for insight in &synthesis.key_insights {
            out.push_str(&format!("- {}\n", insight));
        }
    }
    if !synthesis.open_questions.is_empty() {
        out.push_str("\nOpen investigations:\n");
        for q in &synthesis.open_questions {
            out.push_str(&format!("- {}\n", q));
        }
    }
    out
}
```

---

## Phase D2 — Backfill concepts automatique (MOYENNE)

**But** : Ajouter un backfill concepts périodique au cycle daemon.

**Fichier** : `src/daemon/periodic_tasks.rs`
**Fonction** : `run_prune_cycle()` (ligne 217)
**Point d'insertion** : après injection_decay (ligne 315), avant wal_checkpoint
**LOC** : ~15

### Changement

**Insertion après ligne 315** :
```rust
    // Task: Concept backfill — 1x per day (every ~288 beats at 5min intervals)
    if beat_state.beat % 288 == 0 {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let threads_without_concepts = ThreadStorage::list_active(conn)
                .unwrap_or_default()
                .into_iter()
                .filter(|t| t.concepts.as_deref().unwrap_or("[]") == "[]"
                         || t.concepts.as_deref().unwrap_or("").is_empty())
                .take(10)  // batch of 10 per cycle
                .collect::<Vec<_>>();

            for thread in &threads_without_concepts {
                // Extract concepts from title + summary + topics heuristically
                let mut concepts = Vec::new();
                if let Some(ref topics) = thread.topics {
                    if let Ok(topic_list) = serde_json::from_str::<Vec<String>>(topics) {
                        concepts.extend(topic_list);
                    }
                }
                if !concepts.is_empty() {
                    let concepts_json = serde_json::to_string(&concepts).unwrap_or_default();
                    ThreadStorage::update_concepts(conn, &thread.id, &concepts_json).ok();
                }
            }
            threads_without_concepts.len()
        })) {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(count, "Concept backfill: populated {} threads");
                }
            }
            Err(_) => tracing::error!("Concept backfill panicked"),
        }
    }
```

**Clarification** : Le cycle existant (gossip/decay/archive/merge) reste inchangé. D2 est un **ajout** au cycle.

---

## Phase E — Auto-thread creation (BASSE)

**Pas de code**. Absorbé par Phase A1 (règles comportementales dans l'onboarding).

---

## Résumé des changements

| Phase | Fichier(s) | Fonction(s) | LOC |
|-------|-----------|------------|-----|
| **A1** | inject.rs | `build_onboarding_prompt()` L546 | ~60 |
| **A2** | inject.rs, beat.rs | nouvelle `build_cognitive_nudge()`, `BeatState` struct | ~90 |
| **B1** | inject.rs | `build_healthguard_injection()` L660 | ~15 |
| **B2** | checks.rs, recall.rs, beat.rs | nouvelle `check_recall_staleness()`, `handle_recall()` | ~45 |
| **C1** | compact.rs | `generate_synthesis()` L32, `format_for_injection()` L88 | ~40 |
| **D2** | periodic_tasks.rs | `run_prune_cycle()` L217 | ~15 |
| **Total** | 7 fichiers | | ~265 |

---

## Historique des reviews

### Review pub #1 — APPROUVÉ avec 5 notes (intégrées)

1. **Feedback loop** (LOW) : ajout de delta metrics dans le nudge A2
2. **Nudge anti-bruit** : max 1 nudge/prompt, cooldown 10 beats par type, budget 300 chars
3. **D1 supprimé** : wake aveugle toutes les 4h → absorbé par A2
4. **Contradiction D2/Section 6 clarifiée** : D2 est un ajout, pas une modification
5. **B2 tracking recall** : ajout `last_recall_beat` dans beat.json + écriture depuis handler MCP

### Review pub #2 — APPROUVÉ, 0 notes restantes
