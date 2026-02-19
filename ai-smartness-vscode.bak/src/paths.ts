import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

/**
 * Cross-platform data directory resolution.
 * Must match exactly what the Rust path_utils::data_dir() returns:
 *   Linux:   ~/.config/ai-smartness/
 *   macOS:   ~/Library/Application Support/ai-smartness/
 *   Windows: %APPDATA%/ai-smartness/
 */
export function dataDir(): string {
    const override = process.env.AI_SMARTNESS_DATA_DIR;
    if (override) { return override; }

    const home = os.homedir();
    switch (process.platform) {
        case 'darwin':
            return path.join(home, 'Library', 'Application Support', 'ai-smartness');
        case 'win32':
            return path.join(process.env.APPDATA || path.join(home, 'AppData', 'Roaming'), 'ai-smartness');
        default: // linux, freebsd, etc.
            return path.join(process.env.XDG_CONFIG_HOME || path.join(home, '.config'), 'ai-smartness');
    }
}

/** {data_dir}/projects/ */
export function projectsDir(): string {
    return path.join(dataDir(), 'projects');
}

/** {data_dir}/projects/{hash}/ */
export function projectDir(projectHash: string): string {
    return path.join(projectsDir(), projectHash);
}

/** {data_dir}/wake_signals/ */
export function wakeSignalsDir(): string {
    return path.join(dataDir(), 'wake_signals');
}

/** {data_dir}/wake_signals/{agentId}.signal */
export function wakeSignalPath(agentId: string): string {
    return path.join(wakeSignalsDir(), `${agentId}.signal`);
}

/** {data_dir}/processor.sock (global daemon) */
export function daemonSocketPath(): string {
    return path.join(dataDir(), 'processor.sock');
}

/** {data_dir}/registry.db */
export function registryDbPath(): string {
    return path.join(dataDir(), 'registry.db');
}

/** {data_dir}/projects/{hash}/session_agent */
export function agentSessionPath(projectHash: string): string {
    return path.join(projectDir(projectHash), 'session_agent');
}

/** {data_dir}/projects/{hash}/daemon.log */
export function projectLogPath(projectHash: string): string {
    return path.join(projectDir(projectHash), 'daemon.log');
}

/**
 * Compute project hash from a canonical path.
 * Must match Rust id_gen::hash_path_string():
 *   SHA-256(canonical_path) → first 12 hex chars
 */
export function projectHash(canonicalPath: string): string {
    const hash = crypto.createHash('sha256').update(canonicalPath).digest('hex');
    return hash.substring(0, 12);
}

/**
 * Resolve project hash for a workspace folder.
 * Uses fs.realpathSync to match Rust's canonicalize().
 */
export function resolveProjectHash(workspacePath: string): string | null {
    try {
        const canonical = fs.realpathSync(workspacePath);
        return projectHash(canonical);
    } catch {
        return null;
    }
}

/**
 * Read the session agent from the session file.
 * Returns agent_id or null if not set.
 */
export function readSessionAgent(projHash: string): string | null {
    const sessionPath = agentSessionPath(projHash);
    try {
        if (!fs.existsSync(sessionPath)) { return null; }
        const content = fs.readFileSync(sessionPath, 'utf8').trim();
        return content || null;
    } catch {
        return null;
    }
}
