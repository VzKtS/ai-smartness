/**
 * Scan for Claude CLI child processes in the extension host.
 * Must be called every tick to keep tracking up to date.
 */
export declare function discoverClaudeProcesses(): void;
export declare function isDebounced(agentId: string): boolean;
/**
 * Build the prompt text depending on communication mode.
 */
export declare function buildPromptText(agentId: string, fromAgent: string, messageBody: string, mode: 'cognitive' | 'inbox'): string;
/**
 * Try to inject a message into a Claude process stdin.
 * Respects debounce, waits for idle, retries up to MAX_RETRIES.
 */
export declare function tryInject(agentId: string, text: string): Promise<boolean>;
/**
 * Non-blocking single-attempt inject. Used by AgentController.
 * PID-targeted: reads beat.json to find the correct Claude process for this agent.
 * Falls back to first idle Claude process if PID not available.
 * Never blocks, never retries — the controller handles retry logic.
 *
 * options.skipIdleCheck: if true, bypass idle detection (used for urgent/interrupt signals).
 */
export declare function tryInjectSync(agentId: string, text: string, projHash?: string, options?: {
    skipIdleCheck?: boolean;
}): boolean;
export declare function cleanup(): void;
//# sourceMappingURL=stdinInjection.d.ts.map