import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

// ─── Types ───

export interface AgentInfo {
    id: string;
    role: string;
    status: string;
    supervisor: string | null;
    team: string | null;
    mode: string;
}

export type DaemonStatus = 'running' | 'stopped' | 'stale';

export interface DaemonInfo {
    status: DaemonStatus;
    pid: number | null;
}

// ─── Binary Resolution ───

let cachedBin: string | null = null;

/**
 * Locate the ai-smartness binary.
 * Priority: which → ~/.local/bin → /usr/local/bin → bare name.
 */
export function resolveBinary(): string {
    if (cachedBin) { return cachedBin; }

    try {
        const result = execSync('which ai-smartness 2>/dev/null', {
            timeout: 2000,
            encoding: 'utf8'
        });
        const p = result.trim();
        if (p) {
            cachedBin = p;
            return p;
        }
    } catch {
        // Not in PATH
    }

    const candidates = [
        path.join(os.homedir(), '.local', 'bin', 'ai-smartness'),
        '/usr/local/bin/ai-smartness',
    ];
    for (const c of candidates) {
        if (fs.existsSync(c)) {
            cachedBin = c;
            return c;
        }
    }

    cachedBin = 'ai-smartness';
    return cachedBin;
}

// ─── Agent Commands ───

/**
 * List agents for a project.
 * Parses text output: ID  ROLE  STATUS  SUPERVISOR  TEAM  MODE
 */
export function listAgents(projectHash: string): AgentInfo[] {
    try {
        const bin = resolveBinary();
        const result = execSync(
            `${bin} agent list --project-hash ${projectHash} 2>/dev/null`,
            { timeout: 5000, encoding: 'utf8' }
        );

        const agents: AgentInfo[] = [];
        const lines = result.trim().split('\n');
        for (const line of lines) {
            // Skip header, separator, footer, empty
            if (
                line.startsWith('ID') ||
                line.startsWith('-') ||
                line.includes('Total:') ||
                !line.trim()
            ) {
                continue;
            }
            const parts = line.trim().split(/\s{2,}/);
            if (parts.length >= 6) {
                agents.push({
                    id: parts[0],
                    role: parts[1],
                    status: parts[2],
                    supervisor: parts[3] === '-' ? null : parts[3],
                    team: parts[4] === '-' ? null : parts[4],
                    mode: parts[5],
                });
            }
        }
        return agents;
    } catch {
        return [];
    }
}

/**
 * Select an agent for a project session.
 * Pass null to clear the binding.
 */
export function selectAgent(agentId: string | null, projectHash: string): boolean {
    try {
        const bin = resolveBinary();
        const cmd = agentId
            ? `${bin} agent select ${agentId} --project-hash ${projectHash}`
            : `${bin} agent select --project-hash ${projectHash}`;
        execSync(`${cmd} 2>/dev/null`, { timeout: 5000, encoding: 'utf8' });
        return true;
    } catch {
        return false;
    }
}

// ─── Daemon Commands ───

/**
 * Get daemon status by parsing CLI output.
 */
export function daemonStatus(): DaemonInfo {
    try {
        const bin = resolveBinary();
        const result = execSync(`${bin} daemon status 2>/dev/null`, {
            timeout: 3000,
            encoding: 'utf8'
        });

        if (result.includes('not running')) {
            return { status: 'stopped', pid: null };
        }
        if (result.includes('stale')) {
            return { status: 'stale', pid: null };
        }

        const pidMatch = result.match(/PID\s+(\d+)/);
        const pid = pidMatch ? parseInt(pidMatch[1], 10) : null;
        return { status: 'running', pid };
    } catch {
        return { status: 'stopped', pid: null };
    }
}

/**
 * Start the daemon.
 */
export function daemonStart(): boolean {
    try {
        const bin = resolveBinary();
        const result = execSync(`${bin} daemon start 2>/dev/null`, {
            timeout: 5000,
            encoding: 'utf8'
        });
        return !result.toLowerCase().includes('error');
    } catch {
        return false;
    }
}
