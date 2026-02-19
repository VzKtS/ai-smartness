import type { ChildProcess } from 'child_process';
/**
 * Check if a Claude process is idle (no stdout for IDLE_THRESHOLD_MS).
 */
export declare function isClaudeIdle(pid: number): boolean;
/**
 * Monitor a Claude child process for stdout activity.
 * Only stream_event and assistant message types count as activity.
 */
export declare function ensureMonitored(proc: ChildProcess): void;
/**
 * Discover Claude CLI child processes from the current Node.js process.
 */
export declare function discoverClaudeProcesses(): ChildProcess[];
/**
 * Try to inject a message to a specific Claude process.
 * Retries up to MAX_RETRIES times waiting for idle state.
 */
export declare function tryStdinInjection(proc: ChildProcess, agentId: string, fromAgent: string, messageBody: string): Promise<boolean>;
/**
 * Clean up all tracking state.
 */
export declare function cleanup(): void;
//# sourceMappingURL=stdinInjection.d.ts.map