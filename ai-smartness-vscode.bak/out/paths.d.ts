/**
 * Cross-platform data directory resolution.
 * Must match exactly what the Rust path_utils::data_dir() returns:
 *   Linux:   ~/.config/ai-smartness/
 *   macOS:   ~/Library/Application Support/ai-smartness/
 *   Windows: %APPDATA%/ai-smartness/
 */
export declare function dataDir(): string;
/** {data_dir}/projects/ */
export declare function projectsDir(): string;
/** {data_dir}/projects/{hash}/ */
export declare function projectDir(projectHash: string): string;
/** {data_dir}/wake_signals/ */
export declare function wakeSignalsDir(): string;
/** {data_dir}/wake_signals/{agentId}.signal */
export declare function wakeSignalPath(agentId: string): string;
/** {data_dir}/processor.sock (global daemon) */
export declare function daemonSocketPath(): string;
/** {data_dir}/registry.db */
export declare function registryDbPath(): string;
/** {data_dir}/projects/{hash}/session_agent */
export declare function agentSessionPath(projectHash: string): string;
/** {data_dir}/projects/{hash}/daemon.log */
export declare function projectLogPath(projectHash: string): string;
/**
 * Compute project hash from a canonical path.
 * Must match Rust id_gen::hash_path_string():
 *   SHA-256(canonical_path) → first 12 hex chars
 */
export declare function projectHash(canonicalPath: string): string;
/**
 * Resolve project hash for a workspace folder.
 * Uses fs.realpathSync to match Rust's canonicalize().
 */
export declare function resolveProjectHash(workspacePath: string): string | null;
/**
 * Read the session agent from the session file.
 * Returns agent_id or null if not set.
 */
export declare function readSessionAgent(projHash: string): string | null;
//# sourceMappingURL=paths.d.ts.map