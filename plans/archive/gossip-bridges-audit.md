# Audit — Gossip + ThinkBridges : pourquoi presque pas d'edges

**Mission 8** — demandée par cor, 2026-02-24
**Auditeur** : dev (researcher)

---

## 1. Résumé exécutif

**Cause racine identifiée : BUG CRITIQUE dans le décayeur de bridges** (`decayer.rs:80-81`).

Le decay applique `current_weight × 0.5^(total_age / half_life)` à chaque cycle (toutes les 5 min), où `current_weight` est la valeur **déjà décayée** en DB et `total_age` est l'âge absolu depuis création. Ceci crée un **decay composé quadratique** (exposant N²) au lieu d'un decay exponentiel linéaire.

**Impact** : Un bridge avec poids initial 0.2 (birth bridge typique) atteint le seuil de mort (0.05) en **~5.6 heures** au lieu des **~8 jours** prévus. Tous les bridges meurent en 5-8 heures quel que soit leur poids initial.

Causes secondaires identifiées :
- Le graph ne montre que les threads actifs (74% sont suspendus → bridges filtrés)
- `get_bridges()` retourne les bridges morts (weight=0) → pollution visuelle

---

## 2. Cause 1 — Decay composé (CRITIQUE)

### 2.1 Le bug

**Fichier** : `src/intelligence/decayer.rs:70-91`

```rust
// Code actuel (BUGUÉ)
let reference_time = bridge.last_reinforced.unwrap_or(bridge.created_at);
let age_days = (now - reference_time).num_hours() as f64 / 24.0;
let decay_factor = 0.5f64.powf(age_days / cfg.bridge_half_life);
let new_weight = bridge.weight * decay_factor;  // ← BUG ICI
```

Le problème : `bridge.weight` est lu depuis la DB (déjà décayé par les cycles précédents), puis multiplié par un facteur basé sur l'âge TOTAL. Le résultat est réécrit en DB (lignes 84-91). Au cycle suivant, le même processus se répète sur la valeur déjà réduite.

### 2.2 Démonstration mathématique

Soit W₀ le poids initial, h la demi-vie (4 jours), Δ l'intervalle entre cycles (5 min = 0.00347 jours).

**Formule correcte** (decay exponentiel simple) :
```
w(t) = W₀ × 0.5^(t/h)
```

**Formule actuelle** (decay composé) — après N cycles :
```
w_N = W₀ × 0.5^(Δ/h × Σ_{k=1}^{N} k) = W₀ × 0.5^(Δ × N(N+1) / (2h))
```

L'exposant croît en **N²** (quadratique) au lieu de **N** (linéaire).

### 2.3 Impact chiffré

| Poids initial | Mort attendue (correct) | Mort réelle (bug) | Ratio |
|:---:|:---:|:---:|:---:|
| 0.20 (birth bridge) | ~8 jours | **~5.6 heures** | 34× plus rapide |
| 0.50 (bon gossip) | ~13.3 jours | **~7.3 heures** | 44× plus rapide |
| 1.00 (max théorique) | ~17.3 jours | **~8.3 heures** | 50× plus rapide |

**Vérification après 1 jour** (N=288 cycles) pour W₀=0.5 :
- Exposant réel : Δ/h × 288×289/2 = 0.000868 × 41616 = **36.12**
- Exposant correct : 1/4 = **0.25**
- Poids réel : 0.5 × 2^(-36.12) ≈ **6.9 × 10⁻¹²** (mort depuis des heures)
- Poids correct : 0.5 × 2^(-0.25) ≈ **0.420**

### 2.4 Le même bug affecte les threads (impact moindre)

`decayer.rs:33-53` — même pattern pour les threads :
```rust
let age_days = (now - thread.last_active).num_hours() as f64 / 24.0;
let decay_factor = 0.5f64.powf(age_days / half_life);
let new_weight = (thread.weight * decay_factor).max(0.0);
```

**Mais impact moindre car** : `thread.last_active` est réinitialisé à chaque interaction (prompt, injection, modification). Pour un thread activement utilisé, `age_days` reste petit (~minutes/heures). Le decay composé a peu d'effet sur les petits `age_days`.

**Pour les bridges** : `reference_time = bridge.last_reinforced.unwrap_or(bridge.created_at)`. Les bridges sont rarement "reinforced" (uniquement sur traversée explicite). La plupart ont `last_reinforced = None`, donc `reference_time = created_at` et `age_days` croît continuellement.

### 2.5 Plan de correction

**Approche recommandée** : decay delta (pas de changement de schéma, impact minimal).

**Fichier** : `src/intelligence/decayer.rs`

Remplacer le calcul de decay pour les bridges (lignes 70-91) :

```rust
// BRIDGES — delta-based decay (corrige le bug de decay composé)
let mut bridges = BridgeStorage::list_active(conn)?;
bridges.extend(BridgeStorage::list_by_status(conn, BridgeStatus::Weak)?);

for bridge in &bridges {
    // Utiliser last_reinforced comme point de référence pour le delta.
    // Si jamais reinforced, utiliser created_at.
    let reference = bridge.last_reinforced.unwrap_or(bridge.created_at);
    let delta_days = (now - reference).num_hours() as f64 / 24.0;
    if delta_days <= 0.0 {
        continue;
    }

    let decay_factor = 0.5f64.powf(delta_days / cfg.bridge_half_life);
    let new_weight = bridge.weight * decay_factor;

    if new_weight < cfg.bridge_death_threshold {
        BridgeStorage::update_weight(conn, &bridge.id, 0.0)?;
        BridgeStorage::update_status(conn, &bridge.id, BridgeStatus::Invalid)?;
    } else if new_weight < crate::constants::BRIDGE_WEAK_THRESHOLD {
        BridgeStorage::update_status(conn, &bridge.id, BridgeStatus::Weak)?;
        BridgeStorage::update_weight(conn, &bridge.id, new_weight)?;
    } else {
        BridgeStorage::update_weight(conn, &bridge.id, new_weight)?;
    }

    // Mettre à jour last_reinforced pour que le prochain cycle
    // calcule le delta depuis CE cycle, pas depuis la création.
    // Note: skip pour les bridges qui viennent de mourir (wasteful).
    if new_weight >= cfg.bridge_death_threshold {
        BridgeStorage::update_last_reinforced(conn, &bridge.id, now)?;
    }
}
```

**Même correction pour les threads** (lignes 25-68) :

```rust
// THREADS — delta-based decay
let age_since_ref = (now - thread.last_active).num_hours() as f64 / 24.0;
// ...compute half_life with orphan factor...
let decay_factor = 0.5f64.powf(age_since_ref / half_life);
let new_weight = (thread.weight * decay_factor).max(0.0);

// Mettre à jour last_active pour le prochain delta
// NOTE: NE PAS faire ça — last_active sert aussi à l'orphan detection.
// Pour les threads, utiliser un champ séparé OU accepter le bug mineur
// (impact faible car last_active se réinitialise fréquemment).
```

**Alternative pour les threads** : le bug est mineur pour les threads actifs (car `last_active` se réinitialise). Corriger uniquement les bridges en priorité.

**Contrainte `last_active` threads** : `last_active` ne peut PAS servir de delta checkpoint pour les threads — il est utilisé comme fallback `orphan_since` pour la détection orphan (`decayer.rs:46`). Updater `last_active` à chaque cycle de decay casserait la détection orphan pour les threads sans historique d'injection (`last_injected_at` absent). Un fix propre nécessiterait une colonne `last_decayed_at` (schema change). P3 — monitorer avant d'intervenir.

### 2.6 Migration des données existantes

Les bridges déjà décayés (Invalid/Weak) sont irrécupérables — le poids initial n'est pas stocké.

```rust
// Migration one-shot : supprimer les bridges morts et relancer gossip
BridgeStorage::delete_by_status(conn, BridgeStatus::Invalid)?;
BridgeStorage::delete_by_status(conn, BridgeStatus::Weak)?;
// Le prochain cycle gossip recréera les bridges manquants.
```

### 2.7 Fonction manquante : update_last_reinforced

**Fichier** : `src/storage/bridges.rs` — ajouter :

```rust
pub fn update_last_reinforced(
    conn: &Connection,
    bridge_id: &str,
    when: chrono::DateTime<chrono::Utc>,
) -> AiResult<()> {
    conn.execute(
        "UPDATE bridges SET last_reinforced = ?1 WHERE id = ?2",
        params![time_utils::to_sqlite(&when), bridge_id],
    )?;
    Ok(())
}
```

---

## 3. Cause 2 — Graph filtre active-only (HAUTE)

### 3.1 Le problème

**Fichier** : `src/gui/frontend/app.js:1862`

```javascript
const [threads, bridges] = await Promise.all([
    invoke('get_threads', { projectHash, agentId: aid, statusFilter: 'active' }),
    invoke('get_bridges', { projectHash, agentId: aid }),
]);
```

Puis dans `buildGraph()` (app.js:1892) :

```javascript
graphEdges = bridges
    .filter(b => idSet.has(b.source_id) && idSet.has(b.target_id))
    .map(b => ({ ... }));
```

Seuls les threads **actifs** sont chargés. Les bridges dont un endpoint est suspendu/archivé sont silencieusement éliminés.

**État actuel** : 19 actifs / 63 suspendus / 3 archivés = **22% actifs**.
Probabilité qu'un bridge ait ses DEUX endpoints actifs : ~22% × 22% = **~5%**.
→ **~95% des bridges sont masqués** même s'ils existent en DB.

### 3.2 Plan de correction

**Fichier** : `src/gui/frontend/app.js`

Option A (recommandée) : charger tous les threads connectés par un bridge.

```javascript
// Charger les threads actifs + les threads référencés par des bridges
const [activeThreads, bridges] = await Promise.all([
    invoke('get_threads', { projectHash, agentId: aid, statusFilter: 'active' }),
    invoke('get_bridges', { projectHash, agentId: aid }),
]);

// Collecter les IDs d'endpoints de bridges non-actifs
const activeIds = new Set(activeThreads.map(t => t.id));
const missingIds = new Set();
for (const b of bridges) {
    if (!activeIds.has(b.source_id)) missingIds.add(b.source_id);
    if (!activeIds.has(b.target_id)) missingIds.add(b.target_id);
}

// Charger les threads manquants (suspended/archived) si nécessaire
let allThreads = activeThreads;
if (missingIds.size > 0) {
    const otherThreads = await invoke('get_threads', {
        projectHash, agentId: aid, statusFilter: 'all'
    });
    const missing = otherThreads.filter(t => missingIds.has(t.id));
    allThreads = [...activeThreads, ...missing];
}

buildGraph(allThreads, bridges);
```

Option B (simple) : ajouter un sélecteur de filtre au graph.

```javascript
// Dans loadGraph(), utiliser le filtre du graph, pas hardcoded 'active'
const graphStatusFilter = document.getElementById('graph-status-filter')?.value || 'all';
invoke('get_threads', { projectHash, agentId: aid, statusFilter: graphStatusFilter })
```

---

## 4. Cause 3 — list_all retourne les bridges morts (MOYENNE)

### 4.1 Le problème

**Fichier** : `src/gui/commands.rs:518`

```rust
let bridges = BridgeStorage::list_all(&conn).map_err(|e| e.to_string())?;
```

`list_all()` (bridges.rs:181-193) retourne TOUS les bridges, y compris Invalid (weight=0). Dans le graph, ces bridges ont `lineWidth = weight * 3 = 0` → invisibles mais polluent le compteur d'edges affiché.

### 4.2 Plan de correction

**Fichier** : `src/gui/commands.rs` — remplacer `list_all` par `list_active` :

```rust
let bridges = BridgeStorage::list_active(&conn).map_err(|e| e.to_string())?;
```

Ou filtrer côté frontend dans `buildGraph()` :

```javascript
graphEdges = bridges
    .filter(b => idSet.has(b.source_id) && idSet.has(b.target_id))
    .filter(b => b.weight > 0.05)  // Exclure bridges morts
    .map(b => ({ ... }));
```

---

## 5. Cause 4 — RELATION_COLORS incomplet (BASSE)

### 5.1 Le problème

**Fichier** : `src/gui/frontend/app.js:1828-1833`

```javascript
const RELATION_COLORS = {
    'ChildOf': '#8ad4ff',
    'SiblingOf': '#b5e86c',      // ← devrait être 'Sibling' (nom Rust)
    'RelatedTo': '#ffd56c',      // ← n'existe pas dans BridgeType
    'Supersedes': '#d68cff',     // ← devrait être 'Replaces' (nom Rust)
};
```

BridgeTypes Rust : `Extends`, `Contradicts`, `Depends`, `Replaces`, `ChildOf`, `Sibling`.
Tauri `get_bridges` utilise `format!("{:?}", b.relation_type)` → noms Debug (PascalCase).

Discordances :
- `SiblingOf` (JS) vs `Sibling` (Rust Debug) → pas de couleur pour Sibling
- `RelatedTo` (JS) → n'existe pas dans BridgeType
- `Supersedes` (JS) vs `Replaces` (Rust Debug) → pas de couleur pour Replaces
- `Extends`, `Contradicts`, `Depends` → pas de couleur du tout

### 5.2 Plan de correction

```javascript
const RELATION_COLORS = {
    'ChildOf': '#8ad4ff',
    'Sibling': '#b5e86c',
    'Extends': '#ffd56c',
    'Depends': '#ff9f6c',
    'Contradicts': '#ff6c6c',
    'Replaces': '#d68cff',
};
```

---

## 6. Pipeline gossip — État actuel (pas de bug)

### 6.1 Gossip fonctionne correctement

Le gossip tourne bien :
- **Fréquence** : toutes les 5 min via `periodic_tasks.rs:245`
- **Phase 1** : découverte concept overlap via ConceptIndex inversé (gossip.rs:74-187)
- **Phase 2** : fallback topic overlap pour threads sans concepts (gossip.rs:222-293)
- **Phase 3** : propagation transitive (gossip.rs:332-447)
- **Seuil min** : 2 concepts partagés, weight ≥ 0.20

### 6.2 Birth bridges fonctionnent correctement

- Créés à la création de chaque thread (thread_manager.rs:621-712)
- Basés sur concept overlap DB (pas embedding)
- Seuil min : weight ≥ 0.20, relation_type = Sibling

### 6.3 Le problème n'est PAS la création

Les bridges SONT créés. Ils sont ensuite **tués par le decay composé en 5-8 heures**. Le gossip en recrée à chaque cycle, mais le decay les détruit entre les cycles. Le résultat net est un très faible nombre de bridges vivants à tout instant.

---

## 7. Réponses aux questions de cor

### Q1 : Les bridges existent-ils en DB ?
**Oui, mais presque tous sont Invalid/Weak** à cause du decay composé. Les bridges naissent (birth + gossip) puis meurent en 5-8 heures. À tout instant, seuls les bridges créés dans les dernières heures survivent.

### Q2 : Le gossip tourne-t-il ?
**Oui.** Toutes les 5 min via le daemon. Création correcte. Le problème est côté decay, pas côté création.

### Q3 : Différence gossip vs ThinkBridges ?
**Même entité.** `ThinkBridge` est le struct Rust (bridge.rs:83-100). Les bridges sont créés par : birth (thread creation), gossip_v2 (periodic discovery), thread_manager (fork). Tous stockés dans la même table `bridges`.

### Q4 : API graph → bridges ?
`get_bridges` (Tauri) appelle `list_all()` → retourne tout y compris les morts. Le graph filtre ensuite côté client (endpoints doivent exister dans les threads actifs).

### Q5 : Le graph masque-t-il des bridges ?
**Oui, massivement** (Cause 2). Le graph charge uniquement les threads actifs (22% du total). Les bridges vers des threads suspendus sont éliminés (~95% des bridges).

---

## 8. Fichiers modifiés

| Fichier | Cause | Action | LOC estimées |
|---------|:-----:|--------|:------------:|
| `src/intelligence/decayer.rs` | 1 | Fix decay composé → delta-based | ~20 |
| `src/storage/bridges.rs` | 1 | Ajouter `update_last_reinforced()` | ~10 |
| `src/gui/frontend/app.js` | 2 | Charger threads endpoints bridges | ~15 |
| `src/gui/frontend/app.js` | 3 | Filtrer bridges morts | ~2 |
| `src/gui/frontend/app.js` | 4 | Corriger RELATION_COLORS | ~6 |
| `src/gui/commands.rs` | 3 | `list_active` au lieu de `list_all` | ~1 |
| **Total** | | | **~54** |

### Fichiers NON modifiés
- `gossip.rs` — création bridges correcte
- `thread_manager.rs` — birth bridges corrects
- `coherence.rs`, `extractor.rs` — pas impactés
- `ipc_server.rs`, `capture_queue.rs` — pas impactés
- `bridges.rs` (schéma) — pas de changement de schéma

---

## 9. Risques et mitigations

| # | Risque | Sévérité | Mitigation |
|---|--------|----------|------------|
| R1 | Sémantique `last_reinforced` changée (decay l'utilise maintenant) | ~~MOYENNE~~ BASSE | Confirmé par pub : sémantique déjà "reset du clock decay" (cf. engram_retriever.rs:225) |
| R2 | Migration données : bridges existants irrécupérables | BASSE | Purge Invalid/Weak + laisser gossip recréer. Perte minimale. |
| R3 | Threads decay composé (même bug, impact moindre) | MOYENNE | Priorité bridges. Pour threads, le bug est atténué par le reset fréquent de `last_active`. Correction threads en P2 si nécessaire. |
| R4 | Graph all-threads plus lourd (Cause 2 fix) | BASSE | Lazy-load uniquement les threads référencés par bridges, pas tous |
| R5 | `update_last_reinforced` appelé à chaque cycle = 1 UPDATE/bridge/5min | BASSE | Batch update possible. ~100 bridges × 1 UPDATE = négligeable |

---

## 10. Priorité d'implémentation

1. **P0 (bloquant)** : Fix decay composé bridges (Cause 1) — sans ça, aucun bridge ne survit
2. **P1 (important)** : Purge bridges morts + relancer gossip (migration)
3. **P1 (important)** : Graph → charger endpoints bridges (Cause 2)
4. **P2 (cosmétique)** : `list_active` dans get_bridges Tauri (Cause 3)
5. **P2 (cosmétique)** : RELATION_COLORS (Cause 4)
6. **P3 (surveillance)** : Thread decay composé — monitorer avant de corriger

---

## 11. Historique reviews

### Review pub R1 — 2026-02-24

**VERDICT : APPROUVÉ** avec 2 précisions à intégrer.

**Bug confirmé** : `bridges.rs` distingue `update_weight()` (weight seul) et `reinforce_weight()` (weight + last_reinforced). Le decay utilise `update_weight()` → `last_reinforced` jamais mis à jour → bug quadratique confirmé chaque cycle.

**Math §2.2-2.3** : CORRECTE. Vérifiée indépendamment.

**Q1 (`last_reinforced` sémantique)** : ACCEPTÉ sans schema change. `engram_retriever.rs:225` confirme que `last_reinforced` est déjà le "decay clock reset point" par design. `update_last_reinforced()` (§2.7) = bonne approche. Optimisation : skip l'update pour les bridges qui viennent de mourir (weight < death_threshold).

**Q2 (threads P3 ou immédiat)** : MAINTENIR P3. Contrainte à documenter dans §2.5 : `last_active` ne peut pas servir de delta checkpoint — utilisé comme fallback `orphan_since` pour la détection orphan (`decayer.rs:46`). Fix threads nécessiterait colonne `last_decayed_at`.

**Q3 (math)** : CORRECTE.

**Q4 (Option A vs B)** : OPTION A confirmée. Implémentation du plan correcte (lazy-load endpoints manquants uniquement).

**Note migration** : après déploiement, le 1er cycle catch-up les bridges Active (decay massif → mort probable). Comportement souhaité — nettoyage automatique. §2.6 suffisant.
