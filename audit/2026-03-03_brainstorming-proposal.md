# Audit complet — brainstorming-proposal.md

## Items marqués DONE — vérification

| Tier | Item | Statut réel | Détail |
|------|------|-------------|--------|
| 1.1 | Ghost wake fix | ✅ CONFIRMÉ | messaging.rs:343-354 — wake_scheduled=false après traitement |
| 1.2 | Proactive help nudges (~100 prompts) | ❌ OBSOLÈTE | Remplacé par la runtime rule permanente injectée à chaque prompt dans le reminder. Plus besoin de nudge ponctuel |
| 2.1 | Task tracking heartbeat | ✅ CONFIRMÉ | beat.rs:87-89 — pending_tasks dans BeatState |
| 2.2 | Shared threads heartbeat | ❌ OBSOLÈTE | Remplacé par 6.2 (engram fédéré local+shared) — les threads partagés seront intégrés directement dans le recall et le reminder |
| 3.1 | Broadcast interrupt=true | ✅ CONFIRMÉ | messaging.rs:295 — broadcast avec priority |
| 3.2 | Git branch tracking | ✅ CONFIRMÉ | beat.rs:93-98 — git_branch dans BeatState |

**Verdict DONE** : 4/6 confirmés, **2 obsolètes (1.2, 2.2)** — remplacés par runtime rule permanente et engram fédéré (6.2).

## Items NON marqués DONE — déjà implémentés

| Tier | Item | Statut | Détail |
|------|------|--------|--------|
| 4 | task_complete auto-notify | ✅ DÉJÀ FAIT | server.rs:563, agents.rs:397-445 — notification automatique au délégateur |

**Action** : marquer Tier 4 comme DONE.

## Items partiellement implémentés

| Tier | Item | État | Ce qui manque |
|------|------|------|---------------|
| 5.1 | Message threading (reply_to) | ✅ DONE | v5.5.4-alpha — reply_to_id/thread_id dans Message struct, mappé en DB, exposé dans msg_inbox + ai_msg_focus |
| 9 | ai_help refactoring | ⚠️ | ai_help existe mais minimal (dump statique). Pas de param topic pour filtrer par catégorie |
| D1 | ai_continuity CRUD | ✅ DONE | v5.5.5-alpha — param action sur ai_continuity_edges (list/set/unset/scan_orphans/repair). Storage set/unset_continuity_parent ajoutés |
| T15 | Tool Safety | ✅ DONE | v5.5.6-alpha — mode "list" par défaut + catch-all → InvalidInput pour ai_concepts/ai_label. labels/concepts optionnels |
| 5.3 | Task context_path | ✅ DONE | v5.5.7-alpha — context_path sur AgentTask, migration registry V8, exposé dans task_status |

## Items NOT IMPLEMENTED — pertinence et risque

| Tier | Item | Pertinent ? | Risque | Commentaire |
|------|------|-------------|--------|-------------|
| 6.1 | agent_context tool | ✅ Oui | Moyen | Expose le contexte d'un agent à un autre — attention à la sécurité/isolation |
| 6.2 | ai_recall scope=shared | ✅ Oui | Moyen | Recall cross-agent — nécessite un engram sur shared DB ou fédération de résultats |
| 6.3 | ai_sync delta | ✅ Oui | Faible | Sync incrémental plutôt que full — optimisation, pas critique |
| 7.1 | BuildResult structured | ⚠️ Moyen | Faible | Utile mais dépend du workflow CI/hook. Peut attendre la TUI |
| 7.3 | Post-commit notif | ⚠️ Moyen | Faible | Nice-to-have, le beat tracking git_branch couvre partiellement le besoin |
| 8.2 | Guardcode | ❌ Discutable | ÉLEVÉ | Complexe, intrusif, risque de faux positifs. À reconsidérer après upgrade LLM |
| 10.1 | AuditResult | ⚠️ Moyen | Faible | Structuré mais pas prioritaire — l'audit se fait déjà manuellement |
| D2 | DynamicQuotaAllocator | ✅ Oui | Moyen | Important pour multi-agent à grande échelle, mais prématuré tant qu'on n'a pas plus de 3-4 agents actifs simultanément |
| T14 | Thinking capture | ✅ Oui | Faible | Dépend du provider (Claude extended thinking). Très utile pour le CONTEXTE décisionnel |
