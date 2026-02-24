use ai_smartness::time_utils;
use ai_smartness::shared::Subscription;
use ai_smartness::AiResult;
use ai_smartness::storage::shared_storage::SharedStorage;

use super::{optional_array, optional_str, optional_usize, required_str, ToolContext};

pub fn handle_discover(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let topics = optional_array(params, "topics").unwrap_or_default();
    let _agent_filter = optional_str(params, "agent_id");

    let shared = SharedStorage::discover(ctx.shared_conn, &topics)?;

    let results: Vec<serde_json::Value> = shared
        .iter()
        .map(|s| {
            serde_json::json!({
                "shared_id": s.shared_id,
                "thread_id": s.thread_id,
                "owner_agent": s.owner_agent,
                "title": s.title,
                "topics": s.topics,
                "published_at": s.published_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(serde_json::json!({"shared": results, "count": results.len()}))
}

pub fn handle_subscribe(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = required_str(params, "shared_id")?;

    let sub = Subscription {
        shared_id: shared_id.clone(),
        subscriber_agent: ctx.agent_id.to_string(),
        subscribed_at: time_utils::now(),
        last_synced: None,
    };

    SharedStorage::subscribe(ctx.shared_conn, &sub)?;
    Ok(serde_json::json!({"subscribed": shared_id}))
}

pub fn handle_unsubscribe(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = required_str(params, "shared_id")?;
    SharedStorage::unsubscribe(ctx.shared_conn, &shared_id, ctx.agent_id)?;
    Ok(serde_json::json!({"unsubscribed": shared_id}))
}

pub fn handle_recommend(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let limit = optional_usize(params, "limit").unwrap_or(5);

    // Recommend shared threads the agent hasn't subscribed to yet
    let all_shared = SharedStorage::discover(ctx.shared_conn, &[])?;
    let my_subs = SharedStorage::list_subscriptions(ctx.shared_conn, ctx.agent_id)?;
    let sub_ids: std::collections::HashSet<String> =
        my_subs.iter().map(|s| s.shared_id.clone()).collect();

    let recommendations: Vec<serde_json::Value> = all_shared
        .iter()
        .filter(|s| !sub_ids.contains(&s.shared_id) && s.owner_agent != ctx.agent_id)
        .take(limit)
        .map(|s| {
            serde_json::json!({
                "shared_id": s.shared_id,
                "title": s.title,
                "owner": s.owner_agent,
                "topics": s.topics,
            })
        })
        .collect();

    Ok(serde_json::json!({"recommendations": recommendations, "count": recommendations.len()}))
}

pub fn handle_sync(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = optional_str(params, "shared_id");

    if let Some(sid) = shared_id {
        SharedStorage::update_sync(ctx.shared_conn, &sid, ctx.agent_id)?;
        Ok(serde_json::json!({"synced": sid}))
    } else {
        let subs = SharedStorage::list_subscriptions(ctx.shared_conn, ctx.agent_id)?;
        let mut synced = 0;
        for sub in &subs {
            let _ = SharedStorage::update_sync(ctx.shared_conn, &sub.shared_id, ctx.agent_id);
            synced += 1;
        }
        Ok(serde_json::json!({"synced_all": synced}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup_shared_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_shared_db(&conn).unwrap();
        conn
    }

    fn setup_agent_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_agent_db(&conn).unwrap();
        conn
    }

    fn setup_registry_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_registry_db(&conn).unwrap();
        conn
    }

    fn insert_shared(conn: &Connection, id: &str, topics: &str) {
        let now = ai_smartness::time_utils::to_sqlite(&ai_smartness::time_utils::now());
        conn.execute(
            "INSERT INTO shared_threads (shared_id, source_thread_id, owner_agent, title, topics, visibility, published_at, updated_at)
             VALUES (?1, ?2, 'agent-a', ?3, ?4, 'network', ?5, ?5)",
            params![id, format!("src-{id}"), format!("Thread {id}"), topics, now],
        ).unwrap();
    }

    fn make_ctx<'a>(agent: &'a Connection, registry: &'a Connection, shared: &'a Connection) -> ToolContext<'a> {
        ToolContext {
            agent_conn: agent,
            registry_conn: registry,
            shared_conn: shared,
            project_hash: "test-proj",
            agent_id: "test-agent",
        }
    }

    #[test]
    fn test_discover_empty() {
        let agent = setup_agent_db();
        let reg = setup_registry_db();
        let shared = setup_shared_db();
        let ctx = make_ctx(&agent, &reg, &shared);

        let result = handle_discover(&serde_json::json!({}), &ctx).unwrap();
        assert_eq!(result["count"], 0);
    }

    #[test]
    fn test_discover_returns_shared_threads() {
        let agent = setup_agent_db();
        let reg = setup_registry_db();
        let shared = setup_shared_db();
        insert_shared(&shared, "s1", "[\"rust\"]");
        insert_shared(&shared, "s2", "[\"python\"]");
        let ctx = make_ctx(&agent, &reg, &shared);

        let result = handle_discover(&serde_json::json!({}), &ctx).unwrap();
        assert_eq!(result["count"], 2);
    }

    #[test]
    fn test_subscribe_and_unsubscribe() {
        let agent = setup_agent_db();
        let reg = setup_registry_db();
        let shared = setup_shared_db();
        insert_shared(&shared, "s1", "[\"rust\"]");
        let ctx = make_ctx(&agent, &reg, &shared);

        let sub_result = handle_subscribe(&serde_json::json!({"shared_id": "s1"}), &ctx).unwrap();
        assert_eq!(sub_result["subscribed"], "s1");

        let unsub_result = handle_unsubscribe(&serde_json::json!({"shared_id": "s1"}), &ctx).unwrap();
        assert_eq!(unsub_result["unsubscribed"], "s1");
    }

    #[test]
    fn test_recommend_excludes_subscribed() {
        let agent = setup_agent_db();
        let reg = setup_registry_db();
        let shared = setup_shared_db();
        insert_shared(&shared, "s1", "[\"rust\"]");
        insert_shared(&shared, "s2", "[\"python\"]");
        let ctx = make_ctx(&agent, &reg, &shared);

        // Subscribe to s1
        handle_subscribe(&serde_json::json!({"shared_id": "s1"}), &ctx).unwrap();

        let result = handle_recommend(&serde_json::json!({}), &ctx).unwrap();
        let recs = result["recommendations"].as_array().unwrap();
        // s1 is subscribed, but both are owned by agent-a, not test-agent
        // So recommend excludes subscribed (s1) but includes s2
        assert!(recs.iter().all(|r| r["shared_id"] != "s1"));
        assert!(recs.iter().any(|r| r["shared_id"] == "s2"));
    }

    #[test]
    fn test_sync_single() {
        let agent = setup_agent_db();
        let reg = setup_registry_db();
        let shared = setup_shared_db();
        insert_shared(&shared, "s1", "[\"rust\"]");
        let ctx = make_ctx(&agent, &reg, &shared);

        handle_subscribe(&serde_json::json!({"shared_id": "s1"}), &ctx).unwrap();
        let result = handle_sync(&serde_json::json!({"shared_id": "s1"}), &ctx).unwrap();
        assert_eq!(result["synced"], "s1");
    }

    #[test]
    fn test_sync_all() {
        let agent = setup_agent_db();
        let reg = setup_registry_db();
        let shared = setup_shared_db();
        insert_shared(&shared, "s1", "[\"rust\"]");
        insert_shared(&shared, "s2", "[\"python\"]");
        let ctx = make_ctx(&agent, &reg, &shared);

        handle_subscribe(&serde_json::json!({"shared_id": "s1"}), &ctx).unwrap();
        handle_subscribe(&serde_json::json!({"shared_id": "s2"}), &ctx).unwrap();
        let result = handle_sync(&serde_json::json!({}), &ctx).unwrap();
        assert_eq!(result["synced_all"], 2);
    }
}
