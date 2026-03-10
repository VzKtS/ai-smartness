# Fix: GUI freeze ~1 min sur update_agent (save agent)

## Cause racine

`call_daemon()` dans `daemon_ipc_client.rs:119-177` n'a aucun timeout sur le stream IPC :
- `Stream::connect` : pas de connect timeout
- `read_line` : bloque indéfiniment si daemon lent ou non-réactif

`update_agent` est synchrone → bloque l'UI Tauri pendant toute la durée du freeze.

## Fichiers à modifier

1. `ai-smartness/src/processing/daemon_ipc_client.rs` — `call_daemon()` L119
2. `ai-smartness/src/gui/commands.rs` — `update_agent()` L1053, L1079

## Plan

### Fix 1 — Timeout IPC dans `call_daemon()` (~10 LOC)

**CORRIGÉ (sub review)** : `interprocess::local_socket::Stream` n'expose PAS `set_read_timeout()`. L'API `as_raw_fd()` retourne `RawFd` (pas `Result<RawFd>`) — le plan initial était incorrect.

Approche retenue : **thread + channel avec `recv_timeout`** (portable Unix + Windows) :
```rust
let (tx, rx) = std::sync::mpsc::channel();
std::thread::spawn(move || {
    tx.send(do_ipc_call(name, request_json)).ok();
});
rx.recv_timeout(Duration::from_secs(5))
  .map_err(|_| AiError::Provider("Daemon IPC timeout after 5s".into()))?
```

Si Fix 2 (async) est appliqué simultanément : combiner via `tokio::time::timeout` sur le `JoinHandle` de `spawn_blocking`.

### Fix 2 — Async Tauri command (~2 LOC)

**APPROUVÉ (sub review)** : compatible Tauri v1 et v2.

Changer la signature de `update_agent` de :
```rust
pub fn update_agent(...) -> Result<serde_json::Value, String>
```
vers :
```rust
pub async fn update_agent(...) -> Result<serde_json::Value, String>
```

Le `#[tauri::command]` supporte nativement les fonctions async. L'appel IPC bloquant doit être wrappé dans `tauri::async_runtime::spawn_blocking`. `interprocess::local_socket::Stream` est `Send` sur v2.x du crate.

### Fix 3 — Migration one-time au startup (~0 LOC dans update_agent)

**CORRIGÉ (sub review — BLOQUANT)** : le plan initial référençait des APIs inexistantes :
- `migrations::get_registry_db_version()` → n'existe pas
- `REGISTRY_DB_CURRENT_VERSION` → n'existe pas

**Solution retenue** : supprimer l'appel `migrate_registry_db` de `update_agent` (commands.rs:1079) et s'assurer qu'il est appelé **une seule fois** au startup de l'app Tauri (init du `AppState` ou équivalent). Si un check inline est absolument nécessaire, utiliser `migrations::get_schema_version(&conn)` + constante `CURRENT_SCHEMA_VERSION` (disponibles dans migrations.rs).

## Verification

- Build : `cargo build --features gui`
- Test : save agent avec daemon actif → pas de freeze
- Test : save agent avec daemon arrêté → erreur immédiate (< 5s), pas de freeze

## Estimation

~15 LOC, 2 fichiers. Risque faible. Reviewé par sub (corrections intégrées).
