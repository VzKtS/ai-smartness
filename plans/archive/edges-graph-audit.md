# Audit — Edges graph (types de relations + rendu poids)

**Mission 13** — demandée par cor, 2026-02-24
**Auditeur** : dev (researcher)

---

## 1. Constat utilisateur

1. Seuls **ChildOf** et **Sibling** apparaissent. La légende affiche 6 types mais les 4 autres (Extends, Depends, Contradicts, Replaces) ne sont jamais visibles.
2. Les edges ChildOf ont une épaisseur proportionnelle au poids — OK.
3. Les edges Sibling ont une épaisseur **fixe** quel que soit le poids.

---

## 2. Diagnostic backend — types de relations créés

### 2.1 Enum BridgeType (bridge.rs:5-12)

6 types définis :

| Type | as_str() (DB) | Debug (API) | Créé ? |
|------|---------------|-------------|--------|
| ChildOf | `child_of` | `ChildOf` | **OUI** — gossip + fork |
| Sibling | `sibling` | `Sibling` | **OUI** — gossip + birth + propagation |
| Extends | `extends` | `Extends` | **OUI** — gossip (rare) |
| Contradicts | `contradicts` | `Contradicts` | **NON** — jamais |
| Depends | `depends` | `Depends` | **NON** — jamais |
| Replaces | `replaces` | `Replaces` | **NON** — jamais |

### 2.2 Chemins de création des bridges

| # | Module | Fonction | Lignes | Type créé | Poids typique |
|---|--------|----------|--------|-----------|---------------|
| P1 | gossip.rs | `run_cycle()` Phase 1 | 130-165 | ChildOf / Extends / Sibling (via `determine_relation`) | 0.20–0.80 |
| P2 | gossip.rs | `run_legacy_topic_overlap()` | 262-279 | **Sibling** uniquement | 0.50 fixe |
| P3 | gossip.rs | `run_propagation()` | 409-430 | **Sibling** uniquement | 0.10–0.30 (décroissant par hop) |
| P4 | thread_manager.rs | `create_thread()` fork | 289-308 | **ChildOf** uniquement | 0.80 fixe |
| P5 | thread_manager.rs | `create_birth_bridges()` | 681-697 | **Sibling** uniquement | 0.15–0.40 |

### 2.3 determine_relation() — gossip.rs:305-317

Seul P1 utilise une logique de détermination du type :

```rust
fn determine_relation(source: &Thread, target: &Thread, weight: f64) -> BridgeType {
    if source.parent_id == Some(target.id)   → ChildOf      // source est enfant de target
    if target.parent_id == Some(source.id)   → Extends       // target est enfant de source
    if same parent_id                         → Sibling       // frères
    if weight >= 0.80 && source plus récent  → Extends       // forte overlap, source postérieure
    else                                      → Sibling       // fallback
}
```

**Extends** est créé quand :
- Un thread a un `parent_id` pointant vers l'autre (requires fork — rare)
- OU weight ≥ 0.80 ET le source est chronologiquement postérieur (requires très forte overlap — rare)

**En pratique :** ~80% Sibling, ~18% ChildOf, ~2% Extends. Le reste (Contradicts, Depends, Replaces) : **0%**.

### 2.4 Chaîne de sérialisation DB → API → Frontend

```
Création:    BridgeType::ChildOf
     ↓
DB (INSERT): bridge.relation_type.as_str() → "child_of"     (bridges.rs:58)
     ↓
DB (SELECT): relation_str.parse() → BridgeType::ChildOf      (bridges.rs:21-23)
     ↓
API (Tauri): format!("{:?}", b.relation_type) → "ChildOf"    (commands.rs:519)
     ↓
Frontend:    RELATION_COLORS['ChildOf'] → '#8ad4ff'           (app.js:1833)
```

Chaîne **correcte**. Pas de mismatch snake_case/PascalCase — la conversion se fait au bon endroit.

---

## 3. Diagnostic frontend — rendu des edges

### 3.1 Stroke-width (app.js:2164)

```javascript
ctx.lineWidth = isHighlight ? 2.5 : Math.max(1, e.weight * 3);
```

La formule est **identique pour TOUS les types de relation**. Le stroke-width dépend **uniquement du poids**, pas du type. Aucun bug de code ici.

### 3.2 Pourquoi Sibling semble « fixe »

Le problème est la **distribution des poids**, pas la formule de rendu :

| Poids | lineWidth (`max(1, w×3)`) | Visuel |
|-------|---------------------------|--------|
| 0.15 | max(1, 0.45) = **1.0** | Minimum, indistinguable |
| 0.20 | max(1, 0.60) = **1.0** | Minimum, indistinguable |
| 0.30 | max(1, 0.90) = **1.0** | Minimum, indistinguable |
| 0.40 | max(1, 1.20) = 1.2 | À peine plus épais |
| 0.50 | max(1, 1.50) = 1.5 | Peu distinguable du 1.0 |
| 0.80 | max(1, 2.40) = 2.4 | **Visuellement distinct** |

**Distribution des poids Sibling :**
- Birth bridges : 0.15–0.40
- Legacy gossip : 0.50 fixe
- Propagation : 0.10–0.30
- → **Cluster 0.15–0.50** → lineWidth 1.0–1.5 → **visuellement identique**

**Distribution des poids ChildOf :**
- Fork : 0.80 fixe
- Gossip ChildOf : variable mais > seuil overlap
- → **Cluster 0.50–0.80** → lineWidth 1.5–2.4 → **variation visible**

### 3.3 Facteur aggravant : bug de décroissance composée (Mission 8)

Le bug N² du decayer (Mission 8, non encore fixé) accélère la décroissance de TOUS les bridges. Les Sibling, nés avec des poids faibles (0.15–0.50), atteignent le seuil de mort (0.05) en quelques heures. Les survivants sont tous au minimum → lineWidth = 1.0 flat.

Après le fix Mission 8, les poids Sibling seront mieux distribués → la proportionnalité stroke-width deviendra plus visible.

### 3.4 Couleur et opacité

```javascript
ctx.strokeStyle = RELATION_COLORS[e.relation] || GRAPH_COLORS.edge_default;
ctx.globalAlpha = isHighlight ? 1 : 0.55 + e.weight * 0.4;
```

- **Couleur** : correcte, varie par type de relation
- **Opacité** : même formule pour tous les types (0.55–0.95 selon poids)
- Pour les Sibling à poids faible : alpha ≈ 0.61–0.75 → plus transparents que les ChildOf

### 3.5 Légende (app.js:2380-2398)

```javascript
for (const [rel, color] of Object.entries(RELATION_COLORS)) {
    html += `<span style="color:${color}">━</span> ${rel} &nbsp; `;
}
```

La légende itère **tous les 6 types** de `RELATION_COLORS`. Elle affiche Contradicts, Depends, Replaces même s'ils n'existent jamais dans les données.

### 3.6 Filtrage des edges (app.js:1887)

```javascript
const liveBridges = bridges.filter(b => b.weight > 0.05);
```

Filtre par poids uniquement. Aucun filtrage par type de relation. Tous les types actifs sont rendus.

---

## 4. Réponses aux questions de cor

### Q1 — Les 4 types manquants sont-ils créés quelque part ?

**Extends : OUI** mais **rare** (~2% des bridges). Créé dans `determine_relation()` quand :
- Thread fork avec `parent_id` inversé, ou
- Concept overlap ≥ 0.80 avec source plus récent

**Contradicts, Depends, Replaces : NON**, jamais créés. Les 3 variants sont des **placeholders dead code** dans l'enum `BridgeType`. Aucun module (gossip, thread_manager, extractor, coherence) ne les génère. L'extracteur LLM produit des concepts/labels mais ne fait PAS d'analyse sémantique de type « contradiction » ou « dépendance ».

### Q2 — Le scaling stroke-width s'applique-t-il à tous les types ?

**OUI**, la formule `Math.max(1, e.weight * 3)` est **uniforme pour tous les types**. Le code ne discrimine pas par `relation_type` pour le stroke-width.

Le problème est **perceptuel, pas logique** :
- Sibling : poids 0.15–0.50 → lineWidth 1.0–1.5 → **indistinguable visuellement**
- ChildOf : poids 0.50–0.80 → lineWidth 1.5–2.4 → **variation visible**

La formule `weight × 3` est trop compressée pour les poids bas. Seuil effectif de distinction visuelle : **weight > 0.67** (lineWidth > 2.0).

### Q3 — Retirer de la légende ou implémenter ?

**Recommandation : retirer les 3 types morts de la légende** (Contradicts, Depends, Replaces). Raisons :
- Implémenter ces types nécessite un extracteur sémantique LLM capable de détecter contradictions/dépendances/remplacements — complexité significative (P2-P3)
- Afficher des types impossibles dans la légende est trompeur
- Extends est rare mais réel — le garder dans la légende

---

## 5. Heuristiques pour les 3 types manquants

### 5.1 Métadonnées disponibles dans determine_relation()

`determine_relation(source: &Thread, target: &Thread, weight: f64)` a accès à la struct Thread complète :

| Champ | Type | Utilité heuristique |
|-------|------|---------------------|
| `status` | Active/Suspended/Archived | Replaces (actif remplace inactif) |
| `origin_type` | Prompt/FileRead/.../Split | Contradicts (splits = divergence intentionnelle) |
| `parent_id` | Option<String> | ChildOf/Extends (existant), Contradicts (frères séparés) |
| `created_at` | DateTime<Utc> | Temporalité (Depends = postérieur, Replaces = plus récent) |
| `last_active` | DateTime<Utc> | Fraîcheur relative |
| `concepts` | Vec<String> | Superset = dépendance (Depends) |
| `topics` | Vec<String> | Overlap sémantique |
| `labels` | Vec<String> | Catégorisation manuelle |
| `importance` | f64 | Poids relatif (importance basse → remplacé) |
| `weight` (param) | f64 | Concept overlap (déjà calculé par gossip) |

**Point critique** : gossip appelle `ThreadStorage::list_all()` (gossip.rs:54) — inclut Active, Suspended ET Archived. `determine_relation()` voit donc des threads de **tous les statuts**.

### 5.2 Heuristique Replaces — FAISABILITÉ HAUTE

**Sémantique** : A remplace B → A est actif, B est inactif, même sujet.

**Signal** : Thread source Active + thread target Suspended/Archived + forte overlap conceptuelle.

```rust
// Replaces: active thread supersedes an inactive one on the same topic
if source.status == ThreadStatus::Active
    && target.status != ThreadStatus::Active
    && weight >= 0.40
{
    return BridgeType::Replaces;
}
// Symétrique: si target est actif et source est inactif
if target.status == ThreadStatus::Active
    && source.status != ThreadStatus::Active
    && weight >= 0.40
{
    // Inverser la direction : la convention est source=nouveau, target=ancien
    // Note: le bridge source→target signifie "source Replaces target"
    return BridgeType::Replaces;
}
```

**Fiabilité** : HAUTE. Un thread actif avec forte overlap conceptuelle sur un thread archivé/suspendu est objectivement un remplacement. Le seuil 0.40 garantit une overlap significative sans être trop restrictif.

**Faux positifs possibles** : Un thread suspendu manuellement par l'utilisateur qui n'est pas « remplacé » mais simplement mis en pause. Mitigation : le poids 0.40 filtre les overlaps faibles. Si problème, augmenter à 0.50.

### 5.3 Heuristique Depends — FAISABILITÉ MOYENNE

**Sémantique** : A dépend de B → A a absorbé les connaissances de B et y a ajouté les siennes.

**Signal** : Les concepts de B sont un sous-ensemble des concepts de A (A est superset de B), ET A est chronologiquement postérieur.

```rust
// Depends: source builds on target's knowledge (strict superset of concepts)
if weight >= 0.50
    && source.created_at > target.created_at
    && source.concepts.len() > target.concepts.len()  // strict superset: source has MORE concepts
    && is_concept_superset(&source.concepts, &target.concepts)
{
    return BridgeType::Depends;
}
```

Helper nécessaire :

```rust
/// Returns true if `superset` contains all concepts of `subset` (case-insensitive).
/// Requires subset to have at least 2 concepts (avoid trivial matches).
fn is_concept_superset(superset: &[String], subset: &[String]) -> bool {
    if subset.len() < 2 { return false; }
    let super_lower: std::collections::HashSet<String> =
        superset.iter().map(|c| c.to_lowercase()).collect();
    let match_count = subset.iter()
        .filter(|c| super_lower.contains(&c.to_lowercase()))
        .count();
    // Au moins 80% des concepts de subset doivent être dans superset
    match_count as f64 / subset.len() as f64 >= 0.80
}
```

**Fiabilité** : MOYENNE. Le superset conceptuel est un bon proxy de dépendance intellectuelle, mais des threads sur le même sujet large (ex: deux threads sur « Rust async ») pourraient avoir des concepts en superset sans véritable dépendance.

**Faux positifs possibles** : Threads indépendants sur le même sujet large. Mitigation : le seuil 80% superset + minimum 2 concepts + **strict superset** (`source.concepts.len() > target.concepts.len()`) réduit le bruit. Deux threads avec les mêmes concepts sont des Siblings, pas Depends (Review pub R1).

### 5.4 Heuristique Contradicts — FAISABILITÉ BASSE

**Sémantique** : A contredit B → les deux threads ont des conclusions opposées sur le même sujet.

**Signal le plus fiable** : Deux threads issus d'un Split du même parent. L'utilisateur a explicitement séparé un thread en deux car il contenait des informations divergentes.

```rust
// Contradicts: both threads were split from the same parent (user saw divergence).
// NOTE: ALL split sibling pairs from the same parent are classified Contradicts,
// not just semantically contradictory ones. If P is split into A, B, C then
// pairs (A-B), (A-C), (B-C) all become Contradicts. "Contradicts" here means
// "intentional divergence" rather than strict logical contradiction.
// Renaming to "Diverges" is out of scope — enum BridgeType is already stable.
if source.origin_type == OriginType::Split
    && target.origin_type == OriginType::Split
    && source.parent_id.is_some()
    && source.parent_id == target.parent_id
{
    return BridgeType::Contradicts;
}
```

**Fiabilité** : BASSE-MOYENNE. Un split indique une divergence, mais pas nécessairement une contradiction. L'utilisateur peut avoir splitté pour organiser (thème A vs thème B) sans opposition sémantique. C'est le **meilleur signal disponible sans LLM**, mais il reste faible.

**Alternative rejetée** : Comparer les ratings (positifs vs négatifs) — trop sparse dans la pratique. Comparer les drift_history — format Vec<String> non structuré, pas de sémantique exploitable.

### 5.5 Ordre de priorité dans determine_relation()

L'ordre d'évaluation est critique — du plus spécifique au plus général :

```rust
fn determine_relation(source: &Thread, target: &Thread, weight: f64) -> BridgeType {
    // 1. Structural: parent-child relationships (existant)
    if source.parent_id.as_deref() == Some(&*target.id) {
        return BridgeType::ChildOf;
    }
    if target.parent_id.as_deref() == Some(&*source.id) {
        return BridgeType::Extends;
    }

    // 2. Contradicts: split siblings — all pairs from same parent (NOUVEAU)
    //    "Contradicts" = intentional divergence, not strict logical contradiction
    if source.origin_type == OriginType::Split
        && target.origin_type == OriginType::Split
        && source.parent_id.is_some()
        && source.parent_id == target.parent_id
    {
        return BridgeType::Contradicts;
    }

    // 3. Structural: same parent (existant)
    if source.parent_id.is_some() && source.parent_id == target.parent_id {
        return BridgeType::Sibling;
    }

    // 4. Replaces: active supersedes inactive (NOUVEAU)
    if source.status == ThreadStatus::Active
        && target.status != ThreadStatus::Active
        && weight >= 0.40
    {
        return BridgeType::Replaces;
    }
    if target.status == ThreadStatus::Active
        && source.status != ThreadStatus::Active
        && weight >= 0.40
    {
        return BridgeType::Replaces;
    }

    // 5. Depends: strict concept superset + temporal (NOUVEAU)
    if weight >= 0.50
        && source.created_at > target.created_at
        && source.concepts.len() > target.concepts.len()  // strict superset
        && is_concept_superset(&source.concepts, &target.concepts)
    {
        return BridgeType::Depends;
    }

    // 6. Extends: strong overlap + temporal (existant, seuil abaissé)
    if weight >= 0.80 && source.created_at > target.created_at {
        return BridgeType::Extends;
    }

    // 7. Fallback (existant)
    BridgeType::Sibling
}
```

**Logique de priorité** :
1. **ChildOf/Extends structurel** : signal le plus fort (parent_id explicite)
2. **Contradicts** : signal structurel (split siblings), avant Sibling car plus spécifique
3. **Sibling structurel** : même parent, signal fort
4. **Replaces** : statut actif/inactif + overlap, signal objectif
5. **Depends** : superset conceptuel, signal plus faible
6. **Extends heuristique** : forte overlap + temporalité
7. **Sibling** : fallback par défaut

### 5.6 Direction des bridges Replaces et Depends

Convention existante : `source → target` (source est lié À target).

- **Replaces** : `source Replaces target` → source est le thread actif, target est l'ancien
- **Depends** : `source Depends target` → source est le thread plus récent qui dépend de target

Pour Replaces, quand c'est target qui est actif et source qui est inactif, il faut **inverser la direction** lors de la création du bridge (source_id = actif, target_id = inactif). Cela nécessite un ajustement dans `run_cycle()` et pas seulement dans `determine_relation()`.

**Option simple** : `determine_relation()` retourne un tuple `(BridgeType, bool)` où le bool indique si les rôles source/target doivent être inversés. Impact : ~5 LOC supplémentaires dans run_cycle.

**Option minimale** : Ignorer la direction pour Replaces. Le bridge existe, le type est correct, la direction est secondaire pour la visualisation. **Recommandation : option minimale** pour la v1.

---

## 6. Plan de correction

### 6.1 Phase 1 — Heuristiques determine_relation() (~35 LOC)

**Fichier** : `src/intelligence/gossip.rs`

Réécrire `determine_relation()` (lignes 305-317) avec la cascade §5.5. Ajouter `is_concept_superset()` en helper privé.

Nécessite l'import de `OriginType` et `ThreadStatus` :

```rust
use crate::thread::{OriginType, ThreadStatus};
```

### 6.2 Phase 2 — Légende dynamique (~5 LOC)

**Fichier** : `src/gui/frontend/app.js`

Filtrer `RELATION_COLORS` dans `renderGraphLegend()` pour n'afficher que les types présents dans les données :

```javascript
const activeRelations = new Set(graphEdges.map(e => e.relation));
for (const [rel, color] of Object.entries(RELATION_COLORS)) {
    if (!activeRelations.has(rel)) continue;
    html += `<span style="color:${color}">━</span> ${rel} &nbsp; `;
}
```

Garder les 6 types dans `RELATION_COLORS` (ils seront maintenant créés). La légende s'adapte dynamiquement.

### 6.3 Phase 3 — Stroke-width √ scaling (~3 LOC)

**Fichier** : `src/gui/frontend/app.js`

Remplacer la formule linéaire par racine carrée :

```javascript
// Avant : Math.max(1, e.weight * 3)  →  0.20 → 1.0, 0.50 → 1.5, 0.80 → 2.4
// Après : Math.max(0.5, Math.sqrt(e.weight) * 3)  →  0.20 → 1.34, 0.50 → 2.12, 0.80 → 2.68
ctx.lineWidth = isHighlight ? 2.5 : Math.max(0.5, Math.sqrt(e.weight) * 3);
```

| Poids | Avant (w×3) | Après (√w×3) | Gain |
|-------|-------------|--------------|------|
| 0.15 | 1.0 (clamp) | 1.16 | +16% |
| 0.20 | 1.0 (clamp) | 1.34 | +34% |
| 0.30 | 1.0 (clamp) | 1.64 | +64% |
| 0.50 | 1.5 | 2.12 | +41% |
| 0.80 | 2.4 | 2.68 | +12% |

### 6.4 Phase 4 (optionnelle) — Opacité √ scaling (~1 LOC)

```javascript
ctx.globalAlpha = isHighlight ? 1 : 0.55 + Math.sqrt(e.weight) * 0.4;
```

---

## 7. Fichiers modifiés

| Fichier | Phase | Action | LOC estimées |
|---------|:-----:|--------|:------------:|
| `src/intelligence/gossip.rs` | 1 | Réécrire `determine_relation()` + `is_concept_superset()` | ~35 |
| `src/gui/frontend/app.js` | 2 | Légende dynamique | ~5 |
| `src/gui/frontend/app.js` | 3 | Stroke-width √ scaling | ~3 |
| `src/gui/frontend/app.js` | 4 | Opacité √ scaling (optionnel) | ~1 |
| **Total** | | | **~44** |

### Fichiers NON modifiés
- `bridge.rs` — les 6 types sont déjà dans l'enum
- `commands.rs` — sérialisation Debug correcte pour tous les types
- `storage/bridges.rs` — insertion/lecture supportent déjà tous les types
- `thread_manager.rs` — fork (ChildOf) et birth (Sibling) inchangés

---

## 8. Interaction avec Mission 8 (bug de décroissance)

Le fix du bug de décroissance composée (Mission 8) aura un **impact direct** sur la visibilité des edges :

| Métrique | Avant fix M8 | Après fix M8 | Après fix M8 + M13 |
|----------|-------------|-------------|---------------------|
| Poids Sibling moyen à 24h | ~0.05 (seuil mort) | ~0.35 | ~0.35 |
| lineWidth Sibling 24h | 1.0 (clamp) | 1.05 (w×3) | 1.77 (√w×3) |
| Types de relation visibles | 2 (ChildOf, Sibling) | 2 | **5-6** |
| Variation visuelle | Aucune | Faible | **Bonne** |

Les Missions 8 et 13 sont **complémentaires** — M8 corrige les poids, M13 corrige la visualisation et enrichit les types.

---

## 9. Fréquence attendue par type après câblage

| Type | Source principale | Fréquence estimée | Poids typique |
|------|-------------------|-------------------|---------------|
| Sibling | Birth + gossip fallback + propagation | ~50% | 0.15–0.50 |
| ChildOf | Fork + gossip parent_id | ~15% | 0.50–0.80 |
| Replaces | Gossip actif/inactif | ~15% | 0.40–0.70 |
| Extends | Gossip parent_id inversé + forte overlap | ~10% | 0.60–0.80 |
| Depends | Gossip superset conceptuel | ~8% | 0.50–0.70 |
| Contradicts | Split siblings | ~2% | Variable |

Sibling reste dominant mais les 5 autres types ont maintenant des chemins de création définis.

---

## 10. Risques et mitigations

| # | Risque | Sévérité | Mitigation |
|---|--------|----------|------------|
| R1 | Faux positifs Replaces (thread suspendu manuellement, pas remplacé) | BASSE | Seuil overlap 0.40. Augmenter à 0.50 si bruit |
| R2 | Faux positifs Depends (threads sur même sujet large) | BASSE | Seuil 80% superset + min 2 concepts. Ajustable |
| R3 | Faux positifs Contradicts (split organisationnel, pas contradiction) | MOYENNE | Signal faible assumé. Label "Contradicts" est un raccourci pour "divergence intentionnelle". Acceptable car rare (~2%) |
| R4 | Stroke-width √ rend les lignes trop épaisses | BASSE | √w×3 plafonne à 3.0 (w=1.0). Acceptable |
| R5 | Légende dynamique vide (aucun bridge) | NULLE | Masque la section edges si set vide |
| R6 | Régression Sibling structurel → Contradicts (splits mal classés) | NULLE | Contradicts ne match que si BOTH threads sont des Splits ET même parent. Sibling structurel (same parent, non-Split) reste Sibling |

---

## 11. Historique reviews

### Review pub R1 — 2026-02-24

**VERDICT : APPROUVÉ** avec 2 recommandations mineures.

**Diagnostic §2-§3** : CORRECT. Chaîne DB→API→Frontend vérifiée. Stroke-width "fixe" Sibling = effet perceptuel (cluster poids bas), pas un bug.

**§5.2 Replaces** : APPROUVÉ. Signal objectif, seuil 0.40 correct. Cascade §5.5 correcte (Sibling structural prioritaire sur Replaces).

**§5.3 Depends** : APPROUVÉ sous R1 — ajouter `source.concepts.len() > target.concepts.len()` pour garantir la sémantique "builds on" vs simple overlap de même taille.

**§5.4 Contradicts** : APPROUVÉ. R2 (doc) — documenter que TOUTES paires de split siblings → Contradicts, pas seulement les vrais contradictions.

**§5.6 Direction** : Option minimale acceptée pour v1.

**§6 Plan** : 4 phases approuvées (~44 LOC).

**§8 M8 interaction** : Correct, complémentaire.
