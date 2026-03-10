# Contre-audit — stability-memory-audit.md (Mission 17)

**Demandé par** : cor (cognitive wake)
**Auteur** : arc
**Plan audité** : `/home/vzcrow/Dev/ai_smartness_dev/ai-smartness/.claude/plans/stability-memory-audit.md`
**Auteur original** : dev ; reviewer : pub

---

## Verdict global

**5 sur 5 correctifs principaux DÉJÀ APPLIQUÉS** dans le code actuel.
Le plan stability-memory-audit.md a été rédigé avec des valeurs obsolètes. Les causes C1–C5 ne reflètent plus l'état du code.

---

## Verdicts par cause

### C1 — HOOK_WAL_AUTOCHECKPOINT = 0 → WAL explosion (60%)

| | Plan | Code actuel | Ligne |
|---|---|---|---|
| `HOOK_WAL_AUTOCHECKPOINT` | `0` | **`100`** | `constants.rs:74` |

**VERDICT : INFIRMÉ — correctif déjà appliqué.**

Le plan propose de changer de 0 à 100. C'est déjà `100` dans le code. La probabilité de 60% et le scénario WAL explosion ne s'appliquent plus.

Note O2 pub (PASSIVE checkpoint sous charge lecteurs) : toujours valide — voir §Risques résiduels.

**LOC plan** : 1 → **LOC réelles** : 0 (déjà corrigé)

---

### C2 — Prune cycle verrouille les connexions trop longtemps (25%)

**Plan** : "La connexion agent est verrouillée pendant TOUT le cycle prune (10-60 sec)"
**Correctif plan** : "drop-and-reacquire" → ~25-30 LOC

**Code actuel** (`periodic_tasks.rs:227-374`) :

```rust
// Single prune cycle — each task acquires/releases the lock
// independently to reduce contention (~5s per task instead of ~60s continuous).
fn run_prune_cycle(conn_mtx: &Mutex<Connection>, ...) {
    run_task("gossip",   || { let Ok(conn) = conn_mtx.lock() else { return }; ... });
    run_task("decay",    || { let Ok(conn) = conn_mtx.lock() else { return }; ... });
    run_task("archive",  || { let Ok(conn) = conn_mtx.lock() else { return }; ... });
    // ... chaque tâche acquiert/libère indépendamment
}
```

**VERDICT : INFIRMÉ — drop-and-reacquire déjà implémenté.**

Signature réelle : `fn run_prune_cycle(conn_mtx: &Mutex<Connection>, ...)` — pas `&Connection`. Le plan proposait exactement ce pattern ; il est déjà en place.

**Risque résiduel** : La tâche `gossip` peut encore tenir le mutex 5-10 sec si `MergeEvaluator` (ONNX) est lent. Mais c'est ~5s, pas ~60s. Acceptable.

**LOC plan** : 25-30 → **LOC réelles** : 0 (déjà corrigé)

---

### C3 — SQLITE_BUSY_TIMEOUT trop long (5 sec)

| | Plan | Code actuel | Ligne |
|---|---|---|---|
| `SQLITE_BUSY_TIMEOUT_MS` | `5_000` | **`1_000`** | `constants.rs:9` |

**VERDICT : INFIRMÉ — correctif déjà appliqué.**

**LOC plan** : 1 → **LOC réelles** : 0 (déjà corrigé)

---

### C4 — `scheduled_wakes` non borné

**Plan** : "Vec sans capacité max" → correctif : cap à 200

**Code actuel** (`beat.rs:282-293`) :

```rust
pub fn schedule_wake(&mut self, after_beats: u64, reason: String) {
    const MAX_SCHEDULED_WAKES: usize = 200;
    if self.scheduled_wakes.len() >= MAX_SCHEDULED_WAKES {
        self.scheduled_wakes.remove(0);  // éviction FIFO
    }
    self.scheduled_wakes.push(...);
}
```

**VERDICT : INFIRMÉ — cap à 200 déjà implémenté avec éviction FIFO.**

**LOC plan** : 5 → **LOC réelles** : 0 (déjà corrigé)

---

### C5 — `list_active()` sans cache dans le prune cycle

**Plan** : "2 appels `list_active()` par cycle par agent" aux lignes 385+402

**Code actuel** (`periodic_tasks.rs:311-331`) :

```rust
// 5+6. Work context cleanup + injection decay (shared list_active cache)
run_task("work_context_and_injection", || {
    let Ok(conn) = conn_mtx.lock() else { return };
    let active = ThreadStorage::list_active(&conn).unwrap_or_default();  // 1 seul appel
    cleanup_stale_work_contexts(&conn, &active)?;
    decay_injection_scores(&conn, &active)?;  // réutilise active
});
```

**VERDICT : INFIRMÉ — 2 tâches combinées avec cache `active` partagé.**

Correction pub ("ligne 348 est `concept_backfill`, rare") : confirmée. La ligne 348 est bien dans un bloc `if beat_state.beat % 288 == 0` (1×/24h).

**LOC plan** : 10 → **LOC réelles** : 0 (déjà corrigé)

---

## Risques secondaires (§5 du plan)

### §5.3 — Pins `list_active()` inefficace (`inject.rs:510`)

```rust
// État actuel — non corrigé
if let Ok(all) = ThreadStorage::list_active(conn) {
    let pins: Vec<_> = all.iter().filter(|t| t.tags.contains(&"__pin__".to_string())).collect();
```

**VERDICT : CONFIRMÉ — non corrigé.** Charge tous les threads actifs pour trouver ~5 pins. Devrait être un filtre SQL (`WHERE tags LIKE '%"__pin__"%'`).

**LOC correctif** : ~5

Note sub : `concept_backfill` (`periodic_tasks.rs:337-339`, `if beat_state.beat % 288 == 0`) est une 3ème instance de `list_active()` hors du bloc C5 combiné. Impact négligeable (1×/24h, 10 threads max), mais même pattern non optimisé.

---

### §5.4 — `Vec::remove(0)` O(N) pour ring buffers

**État actuel** :
- `session.rs:131` : `self.tool_history.remove(0)` — non corrigé
- `session.rs:148` : `self.files_modified.remove(0)` — non corrigé
- `user_profile.rs:104` : `self.context_rules.remove(0)` — non corrigé
- `beat.rs:286` : `self.scheduled_wakes.remove(0)` — non corrigé (mais vec borné à 200, impact mineur)

**VERDICT : CONFIRMÉ — non corrigé.** Impact faible (vecs petits, <100 éléments), mais correctif `VecDeque` est trivial.

**LOC correctif** : ~10-15 total

---

### §5.5 — Pas de `catch_unwind` dans les handlers MCP

**État actuel** : `mcp/server.rs` — aucun `catch_unwind` dans le dispatch des tool handlers. Un panic tue le process MCP entier.

**VERDICT : CONFIRMÉ — non corrigé.** Le daemon lui-même a `run_task` avec `catch_unwind` (periodic_tasks.rs:33-40). Le MCP server n'a pas d'équivalent.

**LOC correctif** : ~10-15

---

## Corrections sur les estimations LOC du plan

| Correctif | LOC plan | LOC réelles | Motif |
|-----------|:--------:|:-----------:|-------|
| C1 `HOOK_WAL_AUTOCHECKPOINT` | 1 | **0** | Déjà appliqué |
| C2 drop-and-reacquire prune | 25-30 | **0** | Déjà appliqué |
| C3 `SQLITE_BUSY_TIMEOUT` | 1 | **0** | Déjà appliqué |
| C4 `scheduled_wakes` cap | 5 | **0** | Déjà appliqué |
| C5 `list_active()` cache | 10 | **0** | Déjà appliqué |
| **Total plan** | **~42-47** | **0** | **Tout déjà corrigé** |

---

## Risques résiduels non couverts

### R1 — WAL PASSIVE checkpoint dévié sous charge (O2 pub — valide)

`periodic_tasks.rs:370-373` utilise `PRAGMA wal_checkpoint(PASSIVE)`. Sous charge MCP/hook avec lecteurs fréquents, PASSIVE ne peut pas checkpointer si des readers actifs existent. Le WAL peut s'accumuler malgré `HOOK_WAL_AUTOCHECKPOINT = 100`.

**Recommandation** : Considérer `PASSIVE` → `TRUNCATE` dans le cycle prune daemon. Mais c'est une optimisation, pas un correctif critique — `HOOK_WAL_AUTOCHECKPOINT = 100` réduit le risque principal.

### R2 — Gossip mutex hold (non adressé)

La tâche gossip acquiert `conn_mtx.lock()` et peut tenir le mutex 5-10 sec si `MergeEvaluator` (ONNX embedding) est lent. Avec C2 corrigé, c'est 5-10s par gossip task, pas 60s pour tout le cycle. Acceptable, mais sous observation.

### R3 — P3 optionnels non appliqués

§5.3 (pins SQL), §5.4 (VecDeque), §5.5 (catch_unwind) restent à implémenter. Priorité basse.

---

## Réévaluation du profil de risque crash

| Cause | Probabilité plan | Probabilité actuelle |
|-------|:----------------:|:-------------------:|
| C1 WAL explosion | 60% | **~5%** (HOOK=100, risque résiduel PASSIVE) |
| C2 Lock contention | 25% | **~10%** (gossip task peut ~5-10s) |
| C3 FD exhaustion | 10% | **~10%** (inchangé — pas de fix structural) |
| C4 OOM heap | 5% | **~5%** (inchangé) |

**Le scénario de crash à 4.4 GB RAM ne tient plus** — la WAL explosion (principal vecteur) est éliminée par `HOOK_WAL_AUTOCHECKPOINT = 100`.

---

## Actions recommandées

| Priorité | Action | LOC | Fichier |
|----------|--------|:---:|---------|
| BASSE | §5.5 catch_unwind MCP dispatch | ~15 | `mcp/server.rs` |
| BASSE | §5.3 pins SQL filter | ~5 | `hook/inject.rs:510` |
| BASSE | §5.4 VecDeque ring buffers | ~15 | `session.rs`, `user_profile.rs` |
| INFO | R1 PASSIVE → TRUNCATE checkpoint | ~3 | `daemon/periodic_tasks.rs:370` |

**Total LOC réellement à implémenter** : ~38 (P3 uniquement)

---

## Conclusion

Le plan stability-memory-audit.md (Mission 17) est **obsolète sur ses 5 points critiques**. Toutes les causes majeures ont été corrigées en amont (probablement par une PR entre la rédaction du plan et aujourd'hui). Le plan ne doit pas être exécuté tel quel — il générerait des double-commits sans valeur.

Seuls §5.3, §5.4, §5.5 restent à traiter (P3, ~38 LOC totales).
