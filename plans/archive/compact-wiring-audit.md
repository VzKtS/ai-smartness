# Audit — compact.rs dead code + câblage synthèse automatique

**Mission 11** — demandée par cor, 2026-02-24
**Auditeur** : dev (researcher)

---

## 1. Inventaire dead code

### 1.1 compact.rs — 100% dead code

| Élément | Type | Lignes | Appelé par |
|---------|------|--------|-----------|
| `SynthesisReport` | struct | 14-21 | Rien (hors compact.rs) |
| `WorkItem` | struct | 23-28 | Rien (hors compact.rs) |
| `generate_synthesis()` | pub fn | 32-111 | **JAMAIS** |
| `save_synthesis()` | fn | 113-126 | generate_synthesis (dead) |
| `format_for_injection()` | fn | 128-157 | generate_synthesis + load_latest_synthesis (dead) |
| `load_latest_synthesis()` | pub fn | 160-194 | **JAMAIS** |

Le module est déclaré (`hook/mod.rs:2: pub mod compact;`) mais aucune de ses fonctions n'est appelée.

### 1.2 beat.rs — `compaction_suspected` flag dead

| Élément | Fichier | Ligne | Usage |
|---------|---------|-------|-------|
| `compaction_suspected: bool` | beat.rs | 53 | Déclaration |
| `compaction_suspected: false` | beat.rs | 129 | Default |
| `self.compaction_suspected = true` | beat.rs | 314 | Set quand tokens drop >40% |
| `self.compaction_suspected = false` | beat.rs | 321 | Reset quand pas de drop |

**Le flag est SET mais JAMAIS LU** — aucune référence à `compaction_suspected` en dehors de beat.rs. La détection fonctionne, mais personne ne consomme le résultat.

### 1.3 inject.rs — aucune référence

Grep `compact|synthesis|compaction` dans inject.rs → **0 résultats**. Aucune couche d'injection ne référence la synthèse post-compaction.

### 1.4 SynthesisConfig — existant mais non connecté

`config.rs:388-419` — `SynthesisConfig` avec `llm`, `max_messages`, `language`, etc. Exposé dans la GUI Tauri (index.html:405-415). Inclus dans `GuardianConfig` (config.rs:1010). **Mais compact.rs ne l'utilise pas** — la génération est purement heuristique (DB read + format), pas LLM.

### 1.5 intelligence/synthesis.rs — module DIFFÉRENT

`intelligence/synthesis.rs` est un summarizer de **messages de threads** (heuristique, 36 lignes). Ce n'est PAS la synthèse de session/compaction. Modules distincts, pas de confusion.

---

## 2. Analyse du code existant (compact.rs)

### 2.1 Évaluation

Le code est **structurellement correct** et **fonctionnellement complet**. Il implémente exactement ce qui est nécessaire :

1. `generate_synthesis()` : lit les threads actifs, construit un rapport (active_work, key_insights, open_questions), sauvegarde en fichier JSON, retourne le texte formaté
2. `save_synthesis()` : écrit dans `{agent_data}/synthesis/synthesis_{timestamp}.json`
3. `format_for_injection()` : formate en texte lisible pour injection dans le prompt
4. `load_latest_synthesis()` : charge le dernier fichier, vérifie la fraîcheur (< 1 heure), retourne formaté

### 2.2 Bugs mineurs à corriger

| # | Sévérité | Ligne | Bug |
|---|----------|-------|-----|
| B1 | BASSE | 136 | `&s[..s.len().min(100)]` — pas UTF-8 safe. Utiliser `truncate_safe(s, 100)` |
| B2 | BASSE | 41-49 | `list_active()` ne trie pas par pertinence. Ajouter `.sort_by()` sur importance ou last_active |
| B3 | BASSE | 38 | Ouvre sa propre connexion DB alors que inject.rs en a déjà une. Refactorer pour accepter `&Connection` |

### 2.3 Ce qui est BIEN

- **Pas d'appel LLM** — purement heuristique (DB + fichiers). Zéro latence ajoutée au hook synchrone.
- **TTL 1 heure** — la synthèse expire automatiquement, pas de staleness
- **Sauvegarde fichier** — persisté sur disque, survit au restart du daemon
- **Pins inclus** — les pins utilisateur sont inclus dans key_insights (ligne 70-83)
- **Focus tags** — les threads `__focus__` sont inclus dans open_questions (ligne 86-95)

---

## 3. Réponses aux questions de cor

### Q1 : Où brancher generate_synthesis() ?

**Réponse : `inject.rs`**, dans le hook synchrone, immédiatement après le chargement de beat_state (ligne 107).

**Pourquoi pas beat.rs ?** beat.rs est un module de stockage (load/save). Il ne doit pas déclencher d'actions.

**Pourquoi pas periodic_tasks ?** Le daemon n'a pas accès au beat.json par agent (il faudrait scanner tous les agents). Et problème de timing — la synthèse doit être prête pour le PREMIER prompt après compaction.

**Pourquoi pas un hook dédié ?** Claude Code n'a pas de hook "PreCompact" ou "PostCompact". La seule façon de détecter la compaction est via le drop de tokens, détecté dans beat.rs lors du hook UserPromptSubmit (via `update_context_from_transcript()`).

**Timing** : La compaction est détectée quand le prompt N+1 arrive et que les tokens ont chuté de >40% par rapport au prompt N. Le flag `compaction_suspected` est mis à jour dans `update_context()` (beat.rs:310-328), qui est appelé par `update_context_from_transcript()` (inject.rs:104). Donc le flag est disponible PENDANT l'exécution du hook inject du prompt N+1 — timing parfait pour générer + injecter.

### Q2 : Où brancher load_latest_synthesis() pour l'injection ?

**Réponse : Nouvelle couche 1.6** dans inject.rs, entre Layer 1.5 (Session state) et Layer 1.7 (Cognitive nudge).

Logique en deux temps :
- **Premier prompt après compaction** : `compaction_suspected = true` → appeler `generate_synthesis()` → injecter le résultat → reset flag
- **Prompts suivants (dans l'heure)** : `compaction_suspected = false` → appeler `load_latest_synthesis()` → injecter si frais

### Q3 : Le code existant est-il correct et complet ?

**OUI**, à 3 bugs mineurs près (§2.2). Pas besoin de réécriture. Le code est prêt à être câblé.

### Q4 : Risque de double appel LLM ?

**AUCUN.** `generate_synthesis()` est 100% heuristique — DB read + format texte. Zéro appel LLM, zéro coût API, zéro latence réseau. Le seul I/O est une lecture SQLite (~1ms) et une écriture fichier JSON (~1ms).

La `SynthesisConfig.llm` existe dans le config mais n'est PAS utilisée par compact.rs. Elle est prévue pour un futur mode LLM-enhanced mais le mode heuristique actuel est suffisant.

### Q5 : Plan de câblage

Voir §4 ci-dessous.

---

## 4. Plan de câblage

### 4.1 Phase 1 — Refactorer generate_synthesis() pour accepter &Connection (~5 LOC)

**Fichier** : `src/hook/compact.rs`

Modifier la signature pour éviter la double ouverture de connexion :

```rust
/// Generate a synthesis of current work context.
/// Called when context compaction is detected.
pub fn generate_synthesis(
    conn: &rusqlite::Connection,
    project_hash: &str,
    agent_id: &str,
) -> Option<String> {
    let threads = ThreadStorage::list_active(conn).ok()?;
    if threads.is_empty() {
        return None;
    }

    // Trier par importance (meilleure synthèse)
    let mut sorted = threads;
    sorted.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));

    // ... reste du code inchangé, en utilisant `sorted` au lieu de `threads` ...
```

Supprimer les lignes 33-39 (ouverture DB interne).

### 4.2 Phase 2 — Câbler dans inject.rs (~25 LOC)

**Fichier** : `src/hook/inject.rs`

**Ajout 1** — Import en haut du fichier :

```rust
use super::compact;
```

**Ajout 2** — Après `beat_state.save(&agent_data);` (ligne 107), avant Layer 1 :

```rust
    // E2: Compaction synthesis — generate/load synthesis when compaction detected
    let compaction_synthesis = if beat_state.compaction_suspected {
        // First prompt after compaction: generate fresh synthesis
        tracing::info!("Compaction detected — generating session synthesis");
        let synthesis = compact::generate_synthesis(&conn, project_hash, agent_id);
        // Reset flag so we don't regenerate on every subsequent prompt
        beat_state.compaction_suspected = false;
        beat_state.save(&agent_data);
        synthesis
    } else {
        // Subsequent prompts: load from file if fresh (< 1 hour)
        compact::load_latest_synthesis(project_hash, agent_id)
    };
```

**Ajout 3** — Nouvelle Layer 1.6 entre Layer 1.5 (ligne 166) et Layer 1.7 (ligne 168) :

```rust
    // Layer 1.6: Post-compaction synthesis (when context was recently compacted)
    if let Some(ref synthesis) = compaction_synthesis {
        let layer = format!("<system-reminder>\n{}\n</system-reminder>", synthesis);
        if layer.len() < budget {
            budget -= layer.len();
            injections.push(layer);
            tracing::info!(size = layer.len(), "Layer 1.6: post-compaction synthesis injected");
        } else {
            tracing::debug!("Layer 1.6: exceeds budget, skipped");
        }
    }
```

### 4.3 Phase 3 — Fix bugs mineurs (~5 LOC)

**B1** — `compact.rs:136` — UTF-8 safe truncation :

```rust
// Avant (bugué)
out.push_str(&format!(": {}", &s[..s.len().min(100)]));

// Après
out.push_str(&format!(": {}", ai_smartness::constants::truncate_safe(s, 100)));
```

**B2** — `compact.rs:47-55` — Tri par importance (déjà inclus dans Phase 1).

**B3** — `compact.rs:38` — Suppression ouverture DB (déjà inclus dans Phase 1).

### 4.4 Mise à jour doc layers inject.rs

Mettre à jour le commentaire en-tête (lignes 1-16) pour ajouter :

```rust
//!   1.6  Post-compaction synthesis (when context was compacted — session snapshot)
```

---

## 5. Flux complet après câblage

```
PROMPT N: user envoie un message
    │
    ▼
inject.rs: update_context_from_transcript()
    │
    ▼
beat.rs:310: détecte tokens drop >40%
    │  → compaction_suspected = true
    │  → beat_state.save()
    │
    ▼
inject.rs: if compaction_suspected
    │  → compact::generate_synthesis(&conn, ...)
    │     → lit threads actifs (top 10 par importance)
    │     → lit pins.json
    │     → format SynthesisReport
    │     → save synthesis/{timestamp}.json
    │     → retourne texte formaté
    │  → reset compaction_suspected = false
    │
    ▼
inject.rs: Layer 1.6
    │  → injecte synthèse dans prompt
    │
    ▼
Claude Code reçoit :
    <system-reminder>
    Context synthesis (pre-compaction snapshot):
    Active work:
    - Thread title 1: summary...
    - Thread title 2: summary...
    Key insights:
    - [importance=0.9] Critical finding: ...
    - [pin] User-pinned note: ...
    Open investigations:
    - Focus thread: ongoing research...
    </system-reminder>

    [message utilisateur]

PROMPTS N+1, N+2, ... (dans l'heure) :
    │
    ▼
inject.rs: compaction_suspected = false
    │  → compact::load_latest_synthesis(...)
    │     → lit synthesis/{latest}.json
    │     → vérifie TTL < 1 heure
    │     → retourne texte formaté
    │
    ▼
inject.rs: Layer 1.6
    │  → injecte synthèse (tant que < 1 heure)

APRÈS 1 HEURE :
    │
    ▼
load_latest_synthesis() → None (TTL expiré)
    │  → pas d'injection
```

---

## 6. Fichiers modifiés

| Fichier | Phase | Action | LOC estimées |
|---------|:-----:|--------|:------------:|
| `src/hook/compact.rs` | 1, 3 | Refactor signature + fix UTF-8 + tri importance | ~10 |
| `src/hook/inject.rs` | 2 | Import + détection compaction + Layer 1.6 | ~25 |
| **Total** | | | **~35** |

### Fichiers NON modifiés
- `beat.rs` — `compaction_suspected` et `update_context()` déjà corrects
- `config.rs` — SynthesisConfig existe déjà
- `intelligence/synthesis.rs` — module distinct (thread summarization)
- `daemon/` — aucun changement côté daemon

---

## 7. Risques et mitigations

| # | Risque | Sévérité | Mitigation |
|---|--------|----------|------------|
| R1 | Latence hook synchrone | NULLE | generate_synthesis est heuristique : ~2ms (DB read + file write) |
| R2 | Budget injection dépassé | BASSE | Layer 1.6 respecte le budget comme toutes les autres layers |
| R3 | Faux positifs compaction (tokens varient sans compaction) | BASSE | Seuil 40% est conservateur. Si problème, augmenter à 50% dans beat.rs |
| R4 | Fichiers synthesis non nettoyés | BASSE | TTL 1h pour injection, fichiers anciens ignorés. Cleanup possible via periodic_tasks (P3) |
| R5 | Double save beat_state (compaction reset + fin de hook) | NULLE | beat_state est déjà saved deux fois dans inject.rs (ligne 107 et 180). Un troisième save est négligeable (~1ms) |

---

## 8. Historique reviews

### Review pub R1 — 2026-02-24

**VERDICT : APPROUVÉ** sans condition. Livrable directement à cor.

**Dead code** : 100% confirmé. compact.rs jamais appelé, `compaction_suspected` jamais lu.

**Code existant** : structurellement correct, 3 bugs mineurs (B1-B3) correctement identifiés.

**Q1 (inject.rs)** : BON ENDROIT — timing parfait, flag disponible pendant le hook.
**Q2 (Layer 1.6)** : POSITION CORRECTE — gap propre entre 1.5 et 1.7, ordre sémantique logique.
**Q3 (double save)** : ACCEPTABLE — protection défensive, ~1ms.
**Q4 (tri importance)** : SUFFISANT — 3 dimensions couvrent quoi/pourquoi/quoi ensuite.

**Observations mineures** : O1 — factoriser `agent_data` (double compute lignes 70/106). O2 — `load_latest_synthesis` ne touche pas la DB, correctement non refactoré.
