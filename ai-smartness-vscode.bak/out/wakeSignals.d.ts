export interface WakeSignal {
    agent_id: string;
    from: string;
    message: string;
    timestamp: string;
    acknowledged: boolean;
    acknowledged_at?: string;
}
/**
 * Read wake signal for a specific agent.
 * File: {data_dir}/wake_signals/{agentId}.signal
 */
export declare function readWakeSignal(agentId: string): WakeSignal | null;
/**
 * Acknowledge a wake signal by setting acknowledged=true.
 */
export declare function acknowledgeSignal(agentId: string): void;
/**
 * List all pending (unacknowledged) wake signals.
 */
export declare function listPendingSignals(): WakeSignal[];
/**
 * Clean up old acknowledged signals (older than 5 minutes).
 */
export declare function cleanupOldSignals(): number;
//# sourceMappingURL=wakeSignals.d.ts.map