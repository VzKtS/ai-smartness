/**
 * Per-agent wake controller.
 *
 * Each agent has its own independent state machine driven by tick().
 * No blocking awaits — the tick loop calls tick() on every controller
 * and returns immediately. Each controller manages its own timing.
 *
 * States:
 *   idle       → watching for new wake signals
 *   pending    → signal detected, waiting for Claude process to be idle
 *   injecting  → writing to stdin (instant, non-blocking)
 *   cooldown   → debounce period after successful injection
 *   cooldown   → backoff between injection rounds (retries forever until delivered)
 */
type State = 'idle' | 'pending' | 'cooldown';
export declare class AgentController {
    readonly agentId: string;
    readonly projectHash: string | null;
    private state;
    private currentSignal;
    private attempts;
    private lastAttemptTime;
    private cooldownUntil;
    private processedSignalKeys;
    private communicationMode;
    private onLog;
    private onNotify;
    constructor(agentId: string, projectHash: string | null, callbacks: {
        onLog: (msg: string) => void;
        onNotify: (msg: string) => void;
        onWarn?: (msg: string) => void;
    });
    setMode(mode: 'cognitive' | 'inbox'): void;
    /**
     * Non-blocking tick. Called every poll interval.
     * Advances the state machine one step. Never blocks.
     */
    tick(autoPrompt: boolean): void;
    /**
     * Force a manual inbox check (from command).
     * Returns true if injection succeeded immediately.
     */
    forceCheck(): boolean;
    get pendingCount(): number;
    get currentState(): State;
    private checkForSignal;
    private tryInjectNow;
    private enterCooldown;
    /**
     * Purge old processed signal keys (memory cleanup).
     */
    cleanup(): void;
}
export {};
//# sourceMappingURL=agentController.d.ts.map