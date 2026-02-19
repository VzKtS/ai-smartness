import * as vscode from 'vscode';
export declare function create(): vscode.StatusBarItem;
/**
 * Update status bar with all active agents for this window.
 * @param agents - All agent IDs bound to sessions in this window (deduplicated)
 * @param pendingCount - Total pending wake signals across all agents
 * @param daemonAlive - Whether the daemon process is running
 * @param enabled - Whether auto-wake is enabled
 */
export declare function update(agents: string[], pendingCount: number, daemonAlive: boolean, enabled: boolean): void;
export declare function dispose(): void;
//# sourceMappingURL=statusBar.d.ts.map