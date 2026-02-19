import * as vscode from 'vscode';
/**
 * Create and show the status bar item.
 */
export declare function create(): vscode.StatusBarItem;
/**
 * Update the status bar with current state.
 */
export declare function update(agentId: string | null, pendingCount: number, daemonAlive: boolean, enabled: boolean): void;
/**
 * Dispose the status bar item.
 */
export declare function dispose(): void;
//# sourceMappingURL=statusBar.d.ts.map