AI Smartness has reached its limits while exploring the possibilities of a Claudecode nudge. A new project is therefore emerging on another repository:https://github.com/VzKtS/Scope

See you soon for much more and much better.
<p align="center">
  <img src="./readme-src/ais-logo-name.png" alt="AI Smartness" width="400">
</p>

<p align="center">
  <strong>Persistent cognitive memory for AI agents</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-6.8.0-blue" alt="Version 6.8.0">
  <img src="https://img.shields.io/badge/language-Rust-orange" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="MIT License">
</p>

<p align="center">
  <a href="README.md">English</a> |
  <a href="README_FR.md">Francais</a> |
  <a href="README_ES.md">Espanol</a>
</p>

---

## What is AI Smartness?

AI Smartness is a persistent cognitive memory runtime for AI agents. It transforms Claude Code into an agent capable of maintaining semantic context across long sessions, detecting connections between concepts, sharing knowledge with other agents, and resuming work after weeks as if you just stepped away for coffee.

The system runs entirely locally — a local LLM (Qwen2.5-Instruct via llama-cpp-2 with Vulkan GPU) handles all memory extraction at zero API cost. No data leaves your machine.

![AI Smartness Demo](./readme-src/ai-smartness.gif)

## Key Features

- **Persistent Memory** — Threads capture every meaningful interaction. File reads, code edits, decisions, reasoning — all extracted and stored in per-agent SQLite databases
- **69 MCP Tools** — Native tools for memory recall, thread management, bridge analysis, shared cognition, task delegation, cognitive messaging, and more
- **Semantic Bridges** — Automatic discovery of connections between threads via the gossip system (cosine similarity + concept overlap). Enables associative memory chains
- **Multi-Agent System** — Isolated memory per agent with shared cognition for cross-agent knowledge exchange. Supervision hierarchy, task delegation, cognitive inbox
- **Local LLM Inference** — Qwen2.5-Instruct 3B/7B via llama-cpp-2 (Vulkan GPU). ONNX embeddings (all-MiniLM-L6-v2). Zero API calls for memory operations
- **Engram Retriever** — 10-validator consensus pipeline decides which threads to inject into each prompt. 3-phase process: hash pre-filter, scoring, consensus
- **Self-Augmentation** — File Chronicle, Mind Priority, Deep Recall, Session Handoff, Freshness Score, Annotation (v6.2-v6.8)
- **GUI Dashboard** — Visual thread browser, bridge DAG graph, agent hierarchy, full configuration editor (Tauri/WebKit)
- **Hook System** — Transparent integration with Claude Code via inject, capture, pretool, and stop hooks
- **4 Execution Modes** — CLI, MCP Server (JSON-RPC stdin/stdout), Daemon (background), GUI (desktop)

![AI Smartness Dashboard](./readme-src/ai-smartness-dashboard.png)

## Quick Start

```bash
# Clone and build
git clone https://github.com/VzKtS/ai-smartness
cd ai-smartness
cargo build --release

# Download ONNX Runtime + embedding model
ai-smartness setup-onnx

# Start the daemon
ai-smartness daemon start

# Initialize your project
cd /path/to/your/project
ai-smartness init

# Create and select an agent
ai-smartness agent add cod --role programmer
ai-smartness agent select cod

# Open Claude Code — memory injection starts automatically
```

For GUI support, build with `cargo build --release --features gui` (requires webkit2gtk, gtk3, libsoup on Linux).

## Architecture Overview

**Single Rust crate** with 4 execution modes:

```
Claude Code <--hooks--> ai-smartness <--IPC--> Daemon
                |                                 |
            MCP Server                    Prune / Gossip / Pool
            (69 tools)                    (background processing)
                |
          3 SQLite DBs:
          - agent.db     (per-agent memory)
          - registry.db  (global agent/project registry)
          - shared.db    (per-project shared cognition)
```

**Memory injection flow:**
1. User sends prompt -> inject hook fires
2. Engram Retriever runs 10-validator consensus on all active threads
3. Top threads + bridges + session state injected (8 KB cap)
4. Agent responds -> capture hook processes the output
5. Daemon extracts threads, bridges, concepts via local LLM

**Key components:**
- **ThreadManager** — Thread creation, content hashing, duplicate detection
- **Gossip v2** — Bridge discovery via ConceptIndex (5-step pipeline, every 5 min)
- **Decayer** — Exponential half-life decay for threads and bridges
- **Archiver** — Stale thread archival with LLM synthesis
- **RuleDetector** — Automatic detection of user preferences (EN+FR patterns)

![AI Smartness Graph](./readme-src/ai-smartness-graph.png)

## Documentation

| Document | Description |
|----------|-------------|
| [FEATURES.md](docs/FEATURES.md) | Exhaustive feature inventory and technical details |
| [User Guide (EN)](docs/USER_GUIDE.md) | Complete user guide — installation, concepts, MCP tools, configuration |
| [Guide Utilisateur (FR)](docs/USER_GUIDE_FR.md) | Guide utilisateur complet en francais |
| [Guia del Usuario (ES)](docs/USER_GUIDE_ES.md) | Guia del usuario completa en espanol |

## Requirements

| Dependency | Version | Notes |
|-----------|---------|-------|
| Rust | >= 1.85 | `rustup` recommended |
| Claude Code | latest | CLI or VS Code extension |
| ONNX Runtime | auto-downloaded | `ai-smartness setup-onnx` |
| **GUI only:** webkit2gtk | 4.1 | `libwebkit2gtk-4.1-dev` |
| **GUI only:** GTK 3 | latest | `libgtk-3-dev` |
| **GUI only:** libsoup | 3.0 | `libsoup-3.0-dev` |

**Platforms:** Linux (fully supported), macOS (implemented, not yet tested), Windows (implemented, not yet tested).

## License

[MIT](LICENSE)

## Contributing

Contributions are welcome. Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes
4. Push to the branch (`git push origin feature/my-feature`)
5. Open a Pull Request

For bug reports and feature requests, open an issue on [GitHub](https://github.com/VzKtS/ai-smartness/issues).
