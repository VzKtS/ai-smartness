# Meta-Plan — Audit croisé et séquencement des 15 plans ouverts

**Auteur** : arc (architecte)
**Date** : 2026-02-24
**Commanditaire** : cor (coordinateur)
**Reviewer** : sub (triangulation)
**Version** : 2.0-final (post-triangulation sub + vérification code)

---

## 0. ALERTE CRITIQUE — Décalage d'état corrigé

> La v1.0 traitait les 15 plans comme "approuvés non implémentés". Après triangulation
> par sub et vérification directe du code source par arc, **8 plans sont FULLY IMPLEMENTED**,
> **4 partiellement**, et **3 restent non implémentés**.
>
> **LOC restant révisé : ~2660** (vs 6760+ dans la v1.0)
> **Plans à dispatcher : 7** (pas 15)

---

## 1. Inventaire complet — État vérifié contre le code

### Plans FULLY IMPLEMENTED (archivables)

| # | Plan | LOC plan | État code | Preuve |
|---|------|----------|-----------|--------|
| 2 | gossip-bridges-audit (decay) | ~30 | **FAIT** | decayer.rs:70 delta-based, bridges.rs:214 update_last_reinforced |
| 3 | thinkbridge-audit | ~81 | **FAIT** | constants.rs:46 MIN_PROMPT_LENGTH=50, inject.rs:129 reject <50, processor.rs:74 gate 51-150 |
| 4 | compact-wiring-audit | ~35 | **FAIT** | inject.rs:115 generate_synthesis() appelé, L191 Layer 1.6, beat.rs:318 compaction_suspected lu |
| 5 | context-tokens-hack | ~120 | **FAIT** | transcript.rs complet, beat.rs:41 context_tokens, inject.rs:350 update_context_from_transcript |
| 6 | max-quota-tracking | ~95 | **FAIT** | quota_probe.rs complet, credentials.rs:34 detect_plan(), rate_limit_tier parsing L45-51 |
| 7 | edges-graph-audit | ~44 | **FAIT** | gossip.rs:308 determine_relation() avec heuristiques, app.js:2203 sqrt stroke, L2420 dynamic legend |
| 8 | cognitive-proactivity-audit | ~265 | **FAIT** | inject.rs:203 Layer 1.7, L828 build_cognitive_nudge(), beat.rs:56-62 les 4 champs, periodic_tasks.rs:333 backfill |
| 11 | gui-agent-form-custom-fields | ~52 | **FAIT** | app.js:1041 ae-custom-role, L1007 report_to dropdown, index.html:995-1001 |
| | **Sous-total archivable** | **~722** | | |

### Plans PARTIELLEMENT IMPLEMENTED (résiduel)

| # | Plan | LOC plan | LOC restant | Détail résiduel |
|---|------|----------|-------------|-----------------|
| 1 | stability-memory-audit | ~45 | **~5** | C1 ✓ (WAL=100 hook/1000 daemon), C2 ✓ (prune chunked), C4 ✓ (wakes cap=200), C5 ✓ (cache). **RESTE** : C3 busy_timeout — constants.rs:9 a 1000ms mais database.rs:47 hardcode 5000ms (discordance) |
| 2b | gossip-bridges (graph) | ~24 | **~10** | Decay ✓. **RESTE** : list_all() ne filtre pas bridges mortes (weight=0) — bridges.rs:181 query sans WHERE |
| 10 | cli-first-daemon-standalone | ~805 | **~700** | Layer 1.8 ✓ (inject.rs:220), consume_inject_queue ✓ (controller.rs:434). **RESTE** : controller subcommand CLI absent de main.rs |
| 12 | gui-charte-graphique | ~270 | **~20** | CSS 5 thèmes ✓ (style.css). getThemeColor() ✓ (app.js:1853). **RESTE** : 6 RELATION_COLORS partiellement migrés dans refreshGraphColors (seuls Extends/Contradicts, manque 4 types) |
| | **Sous-total résiduel** | | **~735** | |

### Plans NON IMPLEMENTED

| # | Plan | LOC | Dépendances satisfaites ? |
|---|------|-----|--------------------------|
| 9 | model-per-agent-audit | ~117 | Oui (gui-agent-form fait, max-quota fait) |
| 13 | dag-graph-features | ~1175 | Oui (edges-graph fait, gui-charte CSS fait) |
| 14 | tui-headless-audit (B1-B3) | ~30 | Indépendant |
| 15 | arborescence-refactor | ~600+ | À reporter (dernier) |
| | **Sous-total non impl.** | **~1922** | |

### TOTAL LOC RESTANT : ~2660

---

## 2. Dépendances — État actualisé

### Dépendances SATISFAITES (plans amont implémentés)

| Dépendance | Statut |
|-----------|--------|
| stability-memory C1+C2 → tout | ✅ WAL + prune chunked faits |
| gossip-bridges decay → edges-graph | ✅ Delta-based fait → edges fait aussi |
| gossip-bridges → dag-graph | ✅ Bridges survivent, mais list_all résiduel |
| gui-charte CSS → dag-graph | ✅ CSS fait, CSS vars disponibles |
| context-tokens → max-quota | ✅ Les deux faits |
| thinkbridge → cognitive-proactivity | ✅ Les deux faits |
| compact-wiring L1.6 → cognitive L1.7 → cli-first L1.8 | ✅ Les 3 layers existent |
| gui-agent-form → model-per-agent GUI | ✅ Patterns établis |

### Dépendances ACTIVES restantes

| Dépendance | Raison |
|-----------|--------|
| stability-memory C3 (busy_timeout) → cli-first controller | Controller ouvre beaucoup de connexions DB — timeout doit être corrigé avant |
| gossip-bridges list_all fix → dag-graph | Graph ne doit pas afficher bridges mortes |
| gui-charte RELATION_COLORS → dag-graph | Couleurs graph doivent utiliser CSS vars avant ajout features |
| arborescence-refactor → RIEN | Toujours en dernier |

### Dépendance découverte par sub (nouvelle)

| Dépendance | Raison |
|-----------|--------|
| stability-memory C3 → cli-first controller | database.rs:47 hardcode 5000ms ignorant la constante 1000ms. Le controller multi-connexion amplifiera les locks longs. |

---

## 3. Plans à archiver

Les 8 plans FULLY IMPLEMENTED doivent être marqués comme archivés :

```
gossip-bridges-audit (decay)     → ARCHIVER
thinkbridge-audit                → ARCHIVER
compact-wiring-audit             → ARCHIVER
context-tokens-hack              → ARCHIVER
max-quota-tracking               → ARCHIVER
edges-graph-audit                → ARCHIVER
cognitive-proactivity-audit      → ARCHIVER
gui-agent-form-custom-fields     → ARCHIVER
```

**Action recommandée** : Soit renommer en `*.done.md`, soit déplacer dans `.claude/plans/archive/`.

---

## 4. Fusions — Révision post-triangulation

### ~~Fusion A : gossip-bridges + edges-graph~~ — CADUQUE

Les deux sont implémentés. Seul résiduel gossip (list_all filter, ~10 LOC) — trop petit pour un plan, dispatch direct.

### ~~Fusion B : context-tokens + max-quota~~ — CADUQUE

Les deux sont implémentés. Plus rien à fusionner.

### Nouvelle fusion possible : résiduel stability-memory + résiduel gossip-bridges

~5 LOC busy_timeout fix + ~10 LOC list_all filter = ~15 LOC total. Dispatch en un seul micro-ticket "Storage Residual Fixes".

---

## 5. Proposition de séquencement révisé

### Sprint 1 — Micro-résiduel (rapide, parallélisable)

```
Voie A (Rust storage)                    Voie B (GUI)
─────────────────────                    ──────────
stability-memory C3:                     gui-charte RELATION_COLORS:
  database.rs:47 → utiliser constante      refreshGraphColors() compléter
  (5000 → SQLITE_BUSY_TIMEOUT_MS=1000)    4 types relation manquants
                                           (~20 LOC)
gossip-bridges list_all:
  bridges.rs:181 → WHERE weight > 0
  (~10 LOC)

Total : ~15 LOC                          Total : ~20 LOC
```

**Critère de sortie** : `cargo test` + graph ne montre que bridges vivantes + couleurs thème-aware.

### Sprint 2 — Model per agent (~117 LOC)

**Dépendances satisfaites** : gui-agent-form ✓, max-quota ✓

```
model-per-agent-audit
  Phase 1: Transcript model detection (find_last_json_string)
  Phase 2: expected_model field + DB migration V7
  Phase 3: Hook model mismatch warning in inject.rs
  Phase 4: Wrapper scripts
  Phase 5: GUI selector
```

**Critère de sortie** : Chaque agent utilise son modèle attendu, mismatch logué.

### Sprint 3 — CLI Controller standalone (~700 LOC)

**Dépend de** : Sprint 1 Voie A (busy_timeout corrigé)

```
cli-first-daemon-standalone (résiduel)
  controller start/stop/status subcommand
  PID registry dans beat.json
  Process discovery via /proc/{pid}/fd/0
  Fallback inject_queue (Layer 1.8 déjà fait)
```

**Critère de sortie** : `ai-smartness controller start` démarre le daemon sans VS Code.

### Sprint 4 — DAG Graph features (~1175 LOC)

**Dépend de** : Sprint 1 (bridges filtrées, couleurs CSS)

```
dag-graph-features
  Wave 1: F0 canvas layering + F1 search + F3 detail panel (~225 LOC)
  Wave 2: F4 multi-criteria filtering + F6 tooltips (~180 LOC)
  Wave 3: F7 minimap + F8 export (~140 LOC)
  Wave 4-5: F9-F16 avancé (~630 LOC)
```

**Critère de sortie** : Search + filter + tooltips fonctionnels.

### Sprint 5 — Isolation bugs + Headless (~30 LOC)

```
tui-headless B1-B3
  B1: agent_query → ajouter filtre project_hash
  B2: agent_configure → corriger project_hash
  B3: task_status → ajouter filtre project_hash
```

**Parallélisable** avec Sprints 2-4 (fichiers indépendants).

### Sprint final — Arborescence refactor (~600+ LOC)

**Après stabilisation de tous les sprints précédents.**

```
arborescence-refactor
  Phase 1: config.rs split → config/ (7 modules)
  Phases 2-6: src/ restructuration → core/config/adapters/testing
  §9: DQE (Dynamic Quota Engine)
```

---

## 6. Graphe de dépendances révisé

```
     ┌──────────────────────────────┐
     │  Sprint 1 — Micro-résiduel   │
     │  stability C3 + list_all     │
     │  gui-charte RELATION_COLORS  │
     └─────┬────────────┬───────────┘
           │            │
           ▼            ▼
   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
   │ Sprint 2     │  │ Sprint 4     │  │ Sprint 5     │
   │ model-per-   │  │ dag-graph    │  │ tui-headless │
   │ agent        │  │ features     │  │ B1-B3        │
   │ (~117 LOC)   │  │ (~1175 LOC)  │  │ (~30 LOC)    │
   └──────────────┘  └──────────────┘  └──────────────┘
           │                                    │
           ▼                                    │
   ┌──────────────┐                             │
   │ Sprint 3     │     ← peut aussi //         │
   │ cli-first    │        avec Sprint 4        │
   │ controller   │                             │
   │ (~700 LOC)   │                             │
   └──────┬───────┘                             │
          │                                     │
          └──────────────┬──────────────────────┘
                         ▼
               ┌──────────────────┐
               │  Sprint final    │
               │  arborescence-   │
               │  refactor        │
               │  (~600+ LOC)     │
               └──────────────────┘
```

**Parallélisation** : Sprints 2, 3, 4, et 5 sont tous parallélisables entre eux (fichiers disjoints), à condition que Sprint 1 soit terminé.

---

## 7. Résumé exécutif pour cor

### Constats révisés

1. **15 plans** audités. **8 sont FULLY IMPLEMENTED** et archivables.
2. **4 plans partiellement implémentés** avec ~735 LOC résiduel (dont 2 micro-fixes de ~15+20 LOC).
3. **3 plans non implémentés** représentant ~1922 LOC (model-per-agent, dag-graph, tui-headless B1-B3).
4. **1 plan reporté** : arborescence-refactor (~600+ LOC, sprint final).
5. **LOC total restant : ~2660** (vs ~6760+ dans l'analyse initiale).
6. **Les 2 fusions proposées en v1.0 sont CADUQUES** — les plans source sont déjà implémentés.
7. **Toutes les dépendances critiques sont satisfaites** sauf 2 micro-résiduels (busy_timeout, list_all).

### Correction sur la triangulation sub

Sub avait marqué edges-graph comme "probablement NON implémenté". Vérification code : **FULLY IMPLEMENTED** (determine_relation() L308, sqrt stroke L2203, dynamic legend L2420). Corrigé dans cette version.

### Décisions requises de cor

1. **Archiver les 8 plans faits** ? Méthode préférée : `*.done.md` ou dossier `archive/` ?
2. **Valider Sprint 1 micro-résiduel** (~35 LOC) comme dispatch immédiat ?
3. **Parallélisation Sprints 2-5** : combien de voies simultanées réalistes ?
4. **Sprint final arborescence** : confirmer le report ou avancer le config.rs split séparément ?

### Dispatch recommandé (séquentiel si 1 développeur)

```
 1. stability-memory C3 + gossip list_all   (~15 LOC)  ← immédiat
 2. gui-charte RELATION_COLORS              (~20 LOC)  ← immédiat
 3. model-per-agent                         (~117 LOC)
 4. cli-first controller                    (~700 LOC)
 5. dag-graph-features                      (~1175 LOC)
 6. tui-headless B1-B3                      (~30 LOC)
    ─── PAUSE — stabilisation ───
 7. arborescence-refactor                   (~600+ LOC)
    TOTAL : ~2660 LOC en 7 dispatches
```

---

*v2.0-final — Post triangulation sub + vérification code source directe par arc.*
*Prêt pour envoi à cor.*
