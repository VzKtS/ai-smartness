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
/**
 * Locate the ai-smartness binary.
 * Priority: which → ~/.local/bin → /usr/local/bin → bare name.
 */
export declare function resolveBinary(): string;
/**
 * List agents for a project.
 * Parses text output: ID  ROLE  STATUS  SUPERVISOR  TEAM  MODE
 */
export declare function listAgents(projectHash: string): AgentInfo[];
/**
 * Select an agent for a project session.
 * Pass null to clear the binding.
 */
export declare function selectAgent(agentId: string | null, projectHash: string): boolean;
/**
 * Get daemon status by parsing CLI output.
 */
export declare function daemonStatus(): DaemonInfo;
/**
 * Start the daemon.
 */
export declare function daemonStart(): boolean;
//# sourceMappingURL=cli.d.ts.map