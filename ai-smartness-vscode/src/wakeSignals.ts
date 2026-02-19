import * as fs from 'fs';
import { wakeSignalsDir, wakeSignalPath } from './paths';

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
export function readWakeSignal(agentId: string): WakeSignal | null {
    const signalPath = wakeSignalPath(agentId);
    try {
        if (!fs.existsSync(signalPath)) { return null; }
        const content = fs.readFileSync(signalPath, 'utf8');
        return JSON.parse(content) as WakeSignal;
    } catch {
        return null;
    }
}

/**
 * Acknowledge a wake signal.
 */
export function acknowledgeSignal(agentId: string): void {
    const signalPath = wakeSignalPath(agentId);
    try {
        if (!fs.existsSync(signalPath)) { return; }
        const content = fs.readFileSync(signalPath, 'utf8');
        const signal: WakeSignal = JSON.parse(content);
        signal.acknowledged = true;
        signal.acknowledged_at = new Date().toISOString();
        fs.writeFileSync(signalPath, JSON.stringify(signal, null, 2));
    } catch {
        // Signal may have been deleted between read and write
    }
}

/**
 * Count pending (unacknowledged) signals for an agent.
 * Returns 0 or 1 (one signal file per agent).
 */
export function countPending(agentId: string): number {
    const signal = readWakeSignal(agentId);
    return (signal && !signal.acknowledged) ? 1 : 0;
}

/**
 * Clean up old acknowledged signals (older than 5 minutes).
 * Returns count of deleted files.
 */
export function cleanupOldSignals(): number {
    const dir = wakeSignalsDir();
    let deleted = 0;
    try {
        if (!fs.existsSync(dir)) { return 0; }
        const files = fs.readdirSync(dir).filter(f => f.endsWith('.signal'));
        const cutoff = Date.now() - 5 * 60 * 1000;
        for (const file of files) {
            try {
                const filePath = `${dir}/${file}`;
                const content = fs.readFileSync(filePath, 'utf8');
                const signal: WakeSignal = JSON.parse(content);
                if (signal.acknowledged && signal.acknowledged_at) {
                    const ackTime = new Date(signal.acknowledged_at).getTime();
                    if (ackTime < cutoff) {
                        fs.unlinkSync(filePath);
                        deleted++;
                    }
                }
            } catch {
                continue;
            }
        }
    } catch {
        // Ignore
    }
    return deleted;
}
