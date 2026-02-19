/**
 * Cross-platform data directory.
 * Must match Rust path_utils::data_dir() exactly.
 */
export declare function dataDir(): string;
export declare function projectsDir(): string;
export declare function projectDir(projectHash: string): string;
export declare function agentBeatPath(projHash: string, agentId: string): string;
export declare function wakeSignalsDir(): string;
export declare function wakeSignalPath(agentId: string): string;
export declare function daemonPidPath(): string;
export declare function agentSessionPath(projHash: string): string;
/**
 * Compute project hash from canonical path.
 * Must match Rust id_gen::hash_path_string(): SHA-256, first 12 hex chars.
 */
export declare function projectHash(canonicalPath: string): string;
/**
 * Resolve project hash from a workspace folder path.
 * Uses realpathSync to match Rust's canonicalize().
 */
export declare function resolveProjectHash(workspacePath: string): string | null;
/**
 * Per-session agents directory.
 * Each file is named by session_id and contains a plaintext agent_id.
 */
export declare function sessionAgentsDir(projHash: string): string;
/**
 * Read session agent from plaintext file (global fallback).
 */
export declare function readSessionAgent(projHash: string): string | null;
/**
 * List all agents bound to active sessions (panels) for this project.
 * Reads all files in session_agents/ directory. Returns unique agent IDs.
 */
export declare function listSessionAgents(projHash: string): string[];
//# sourceMappingURL=paths.d.ts.map