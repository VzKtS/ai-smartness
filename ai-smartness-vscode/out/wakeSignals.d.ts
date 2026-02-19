export interface WakeSignal {
    agent_id: string;
    from: string;
    message: string;
    mode?: 'cognitive' | 'inbox';
    timestamp: string;
    acknowledged: boolean;
    acknowledged_at?: string;
}
/**
 * Read wake signal for a specific agent.
 */
export declare function readWakeSignal(agentId: string): WakeSignal | null;
/**
 * Acknowledge a wake signal.
 */
export declare function acknowledgeSignal(agentId: string): void;
/**
 * Count pending (unacknowledged) signals for an agent.
 * Returns 0 or 1 (one signal file per agent).
 */
export declare function countPending(agentId: string): number;
/**
 * Clean up old acknowledged signals (older than 5 minutes).
 * Returns count of deleted files.
 */
export declare function cleanupOldSignals(): number;
//# sourceMappingURL=wakeSignals.d.ts.map