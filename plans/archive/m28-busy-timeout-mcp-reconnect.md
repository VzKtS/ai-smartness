# Plan M28 — busy_timeout execute() bug + MCP reconnect

**Auteur** : arc (architecte)
**Date** : 2026-02-24
**Source** : Audit M28 (transmission manuelle via hook — MCP déconnecté)
**Reviewer** : sub (à faire)
**Priorité** : CRITIQUE (Bug #1) / INFO (Bug #2)

---

## Bug #1 — CRITIQUE : busy_timeout via execute() fait crasher tous les DB opens

### Symptôme observé

```
ERROR: Failed to open DB path=.../agents/coder2.db
  error=Storage error: Failed to set busy_timeout: Execute returned results
  — did you mean to call query?
```

Tous les DB agents échouent à l'ouverture. Daemon démarre en fallback, GUI voit 0 agents/0 threads.

### Diagnostic

**Fichier** : `src/storage/database.rs:54`

```rust
// BUGUÉ
conn.execute(&format!("PRAGMA busy_timeout = {}", SQLITE_BUSY_TIMEOUT_MS), [])
    .map_err(|e| AiError::Storage(format!("Failed to set busy_timeout: {}", e)))?;
```

**Cause racine** : `rusqlite::Connection::execute()` mappe sur `sqlite3_prepare_v2()` + `sqlite3_step()`. Lorsque le step retourne `SQLITE_ROW` (ligne de résultat), rusqlite lève une erreur "Execute returned results". `PRAGMA busy_timeout = N` retourne exactement 1 ligne (la valeur effective du timeout) — d'où l'erreur.

**Pourquoi execute_batch() passe sans erreur pour les autres PRAGMAs** : `execute_batch()` mappe sur `sqlite3_exec()` qui est explicitement conçu pour ignorer les lignes de résultat. `PRAGMA journal_mode = WAL` retourne aussi une ligne ("wal") mais passe silencieusement dans le batch.

**Vérification grep** : UN SEUL `conn.execute()` sur PRAGMA dans tout le codebase — `database.rs:54`. Tous les autres usages (test_helpers, migrations, mcp/tools, daemon) utilisent `execute_batch()`. Le bug est isolé.

**Note** : Le log actuel daemon montre arc.db s'ouvrant correctement — le bug se manifeste sur certains agents (coder2.db, etc.) selon l'ordre d'initialisation ou la version du DB. Potentiellement conditionnel à l'état du WAL ou à un verrou SQLite actif au moment de l'ouverture.

**Relation avec meta-plan stability-memory C3** : La discordance originale (hardcode 5000ms vs constante 1000ms) a déjà été corrigée — `database.rs` utilise bien `SQLITE_BUSY_TIMEOUT_MS`. Mais le correcteur a utilisé `execute()` au lieu de `execute_batch()`, introduisant ce nouveau bug.

### Fix — ~3 LOC

**Fichier** : `src/storage/database.rs`

Intégrer `busy_timeout` dans le batch existant L46-52 :

```rust
fn configure_common(conn: &Connection) -> AiResult<()> {
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = {};
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -2000;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
        SQLITE_BUSY_TIMEOUT_MS,
    ))
    .map_err(|e| AiError::Storage(format!("Failed to configure pragmas: {}", e)))?;
    Ok(())
}
```

Supprimer les lignes 54-55 (le `conn.execute()` séparé).

**Scope** : `database.rs` uniquement. Aucun autre caller à modifier.

### Réponses aux questions de l'audit

| Question | Réponse |
|---------|---------|
| execute_batch() silencieux sur PRAGMAs qui retournent des lignes ? | OUI — sqlite3_exec() ignore SQLITE_ROW. journal_mode="wal" retourne une ligne mais passe car dans batch. |
| Autres conn.execute() sur PRAGMAs ? | NON — seul database.rs:54. Tous les autres usages sont execute_batch(). |
| Fallback daemon intentionnel ? | NON — c'est une conséquence non intentionnelle du lazy pool. Le daemon démarre car il n'ouvre pas les DBs à l'init — seulement sur première requête IPC. L'erreur est silencieuse côté daemon startup. À durcir si souhaité (v2). |
| Scope du fix ? | database.rs uniquement, ~3 LOC. |

---

## Bug #2 — INFO : MCP non reconnecté après kill -9 daemon

### Symptôme observé

Après `kill -9` des deux process daemon, le MCP server VS Code (`mcp__ai-smartness__*`) se déconnecte. Reload VS Code requis. MCP tools absents quelques minutes après reload.

### Diagnostic architectural

**Architecture MCP** (vérifiée dans `src/mcp/server.rs`) :

Le MCP server est **entièrement indépendant du daemon** :
- `McpServer::new()` ouvre ses propres connexions SQLite directement (3 connexions : agent, registry, shared)
- Il ne se connecte PAS au socket IPC `processor.sock`
- Il n'a aucune dépendance runtime sur le daemon

```
VS Code extension
    └── MCP server process (stdin/stdout JSON-RPC)
         └── Direct SQLite connections (agent.db, registry.db, shared.db)
              (pas de connexion à processor.sock)
```

Le daemon (processor.sock) est utilisé par :
- `hook/inject.rs` — pour lire les injections IPC
- `processing/daemon_ipc_client.rs` — pour les captures async

**Cause du disconnect** : VS Code extension tue le process MCP server quand elle détecte que le daemon est down (monitoring de santé côté extension). Quand le daemon redémarre, VS Code doit relancer le MCP server.

**Pourquoi "quelques minutes" après reload** : Deux causes possibles :
1. **Bug #1 actif** — `McpServer::new()` appelle `open_connection()` pour 3 DBs → chaque call échoue à `configure_common()` → MCP server crash immédiatement au démarrage → VS Code le redémarre en boucle → délai perçu
2. **VS Code extension initialization time** — le reload VS Code relance l'extension complète qui doit re-découvrir et reconnecter le MCP server (délai normal ~30-60s mais amplifié si crashes répétés)

**Y a-t-il un watch/retry dans le MCP server ?** NON — `server.rs:run()` lit stdin dans une boucle et exit quand stdin se ferme. Pas de socket watch, pas de health check, pas de retry sur daemon.

**Est-ce que c'est un problème à corriger dans le MCP server ?** PARTIELLEMENT :
- Le MCP server n'a pas besoin de reconnect daemon (il est indépendant)
- La vraie fix est de corriger Bug #1 (qui cause les crashes MCP au restart)
- Optionnellement : améliorer la résilience de `open_connection()` (fallback si busy_timeout échoue)

### Réponses aux questions de l'audit

| Question | Réponse |
|---------|---------|
| MCP devrait-il survivre à un redémarrage daemon ? | Oui — par conception il n'en dépend pas. Si VS Code le tue quand daemon down, c'est un comportement VS Code extension à investiguer. |
| Y a-t-il un watch/retry sur le socket IPC côté MCP ? | NON — MCP ne se connecte jamais au socket IPC. Pas de retry nécessaire pour ce canal. |
| Signal de reconnexion ou health-check loop ? | Non nécessaire pour MCP↔daemon. Mais corriger Bug #1 résoudra le problème de restart MCP. |

### Fix recommandé

**Fix primaire** : Corriger Bug #1. MCP restart fonctionnera ensuite normalement.

**Fix secondaire optionnel** : Dans `configure_common()`, en cas d'erreur, logger un warning mais ne pas retourner d'erreur fatale. Permettre l'ouverture de connexion sans `busy_timeout`. Mais c'est une dégradation intentionnelle — à décider par cor.

---

## Situation actuelle — Dual daemon détecté (hors scope M28)

**Observation en direct** :

```
PID 289158  daemon run-foreground  (lancé 16:43 — stale, non tué)
PID 291841  daemon run-foreground  (lancé 16:50 — actif, PID file actuel)
```

Les deux PIDs tiennent le socket `processor.sock`. C'est le scénario R5 du plan cli-first-daemon.

**Impact** : Les deux daemons traitent les IPC en parallèle — risque de double-processing des captures, contention sur les DBs.

**Action recommandée** : `kill 289158` (stale) — à faire manuellement.

---

## Séquencement de dispatch

| Ordre | Action | Fichier | LOC | Urgence |
|-------|--------|---------|-----|---------|
| 1 | Kill stale daemon 289158 | — | opérationnel | IMMÉDIAT |
| 2 | Fix Bug #1 : execute_batch() pour busy_timeout | database.rs | ~3 | CRITIQUE |
| 3 | Vérifier MCP reconnect après fix #1 | — | test | INFO |
| 4 | (Optionnel) Fix secondaire MCP resilience | database.rs | ~5 | BASSE |

---

## Résumé pour cor

**Bug #1** : Régression introduite lors du fix C3 (discordance constante). Le correcteur a utilisé `conn.execute()` au lieu de `conn.execute_batch()` pour `busy_timeout`. Fix trivial : déplacer le PRAGMA dans le batch. ~3 LOC, `database.rs` uniquement. **Prêt pour dispatch immédiat.**

**Bug #2** : Pas un bug du MCP server — c'est une conséquence de Bug #1 (MCP crash au restart). Corriger Bug #1 résoudra le comportement MCP. Le MCP server est architecturalement indépendant du daemon.

**Dual daemon** : PID 289158 doit être tué manuellement. Ajoute une vérification PID mutex au daemon start dans une prochaine version.

*Transmis à cor — en attente de validation pour dispatch coder.*
