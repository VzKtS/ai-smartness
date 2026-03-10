import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

const PROJECT_HASH_LEN = 12;

/**
 * Cross-platform data directory.
 * Must match Rust path_utils::data_dir() exactly.
 */
export function dataDir(): string {
    const override = process.env.AI_SMARTNESS_DATA_DIR;
    if (override) { return override; }

    const home = os.homedir();
    switch (process.platform) {
        case 'darwin':
            return path.join(home, 'Library', 'Application Support', 'ai-smartness');
        case 'win32':
            return path.join(
                process.env.APPDATA || path.join(home, 'AppData', 'Roaming'),
                'ai-smartness'
            );
        default:
            return path.join(
                process.env.XDG_CONFIG_HOME || path.join(home, '.config'),
                'ai-smartness'
            );
    }
}

export function projectsDir(): string {
    return path.join(dataDir(), 'projects');
}

export function projectDir(projectHash: string): string {
    return path.join(projectsDir(), projectHash);
}

export function agentBeatPath(projHash: string, agentId: string): string {
    return path.join(projectDir(projHash), 'agents', agentId, 'beat.json');
}

export function wakeSignalsDir(): string {
    return path.join(dataDir(), 'wake_signals');
}

export function wakeSignalPath(agentId: string): string {
    return path.join(wakeSignalsDir(), `${agentId}.signal`);
}

export function daemonPidPath(): string {
    return path.join(dataDir(), 'daemon.pid');
}

export function agentSessionPath(projHash: string): string {
    return path.join(projectDir(projHash), 'session_agent');
}

/**
 * Compute project hash from canonical path.
 * Must match Rust id_gen::hash_path_string(): SHA-256, first 12 hex chars.
 */
export function projectHash(canonicalPath: string): string {
    return crypto
        .createHash('sha256')
        .update(canonicalPath)
        .digest('hex')
        .substring(0, PROJECT_HASH_LEN);
}

/**
 * Resolve project hash from a workspace folder path.
 * Uses realpathSync to match Rust's canonicalize().
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
 * Per-session agents directory.
 * Each file is named by session_id and contains a plaintext agent_id.
 */
export function sessionAgentsDir(projHash: string): string {
    return path.join(projectDir(projHash), 'session_agents');
}

/**
 * Read session agent from plaintext file (global fallback).
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

/**
 * List all agents bound to active sessions (panels) for this project.
 * Reads all files in session_agents/ directory. Returns unique agent IDs.
 */
export function listSessionAgents(projHash: string): string[] {
    const dir = sessionAgentsDir(projHash);
    try {
        if (!fs.existsSync(dir)) { return []; }
        const files = fs.readdirSync(dir);
        const agents = new Set<string>();
        for (const file of files) {
            try {
                const content = fs.readFileSync(path.join(dir, file), 'utf8').trim();
                if (content) { agents.add(content); }
            } catch {
                continue;
            }
        }
        return [...agents];
    } catch {
        return [];
    }
}
