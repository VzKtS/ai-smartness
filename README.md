# AI Smartness

Persistent memory and cognitive infrastructure for AI agents.

## Architecture

Rust Cargo workspace with 11 crates:

- **ai-common** — Shared utilities (IDs, time, project registry trait)
- **ai-core** — Domain models, traits, error types
- **ai-storage** — SQLite persistence layer
- **ai-processing** — Text processing, embeddings, coherence
- **ai-intelligence** — Gossip, compaction, memory retrieval
- **ai-guardcode** — Content rules and injection formatting
- **ai-agent-registry** — Multi-agent discovery and heartbeat
- **ai-daemon** — Background daemon (IPC, periodic tasks)
- **ai-mcp** — MCP stdio JSON-RPC server
- **ai-cli** — Command-line interface
- **ai-hook** — Claude Code hooks (inject + capture, <10ms)

## License

MIT — See [LICENSE.md](LICENSE.md)
