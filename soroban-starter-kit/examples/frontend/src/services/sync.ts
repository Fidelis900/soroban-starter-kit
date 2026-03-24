import { storageService } from './storage';
import { transactionQueue } from './transactionQueue';
import { CachedTransaction, ConflictData, SyncStatus } from './storage/types';

/**
 * Sync Service
 * Handles synchronization of offline data with the server and conflict resolution
 */
class SyncService {
  private syncInProgress = false;
  private syncListeners: Set<(status: SyncStatus) => void> = new Set();
  private lastSyncStatus: SyncStatus = {
    lastSyncTime: null,
    pendingCount: 0,
    isSyncing: false,
    lastError: null,
  };

  /**
   * Initialize sync service
   */
  async init(): Promise<void> {
    await storageService.init();
    await this.updatePendingCount();
  }

  /**
   * Subscribe to sync status changes
   */
  subscribe(listener: (status: SyncStatus) => void): () => void {
    this.syncListeners.add(listener);
    return () => this.syncListeners.delete(listener);
  }

  /**
   * Get current sync status
   */
  getSyncStatus(): SyncStatus {
    return { ...this.lastSyncStatus };
  }

  /**
   * Update pending count
   */
  async updatePendingCount(): Promise<void> {
    const count = await transactionQueue.getPendingCount();
    this.lastSyncStatus = {
      ...this.lastSyncStatus,
      pendingCount: count,
    };
    this.notifyListeners();
  }

  private notifyListeners(): void {
    this.syncListeners.forEach(listener => listener(this.lastSyncStatus));
  }

  /**
   * Update sync status
   */
  private updateStatus(update: Partial<SyncStatus>): void {
    this.lastSyncStatus = {
      ...this.lastSyncStatus,
      ...update,
    };
    this.notifyListeners();
  }

  /**
   * Perform full sync
   */
  async sync(
    submitTransaction: (tx: CachedTransaction) => Promise<{ success: boolean; error?: string }>,
    fetchServerData: () => Promise<{ balances: unknown[]; escrows: unknown[] }>
  ): Promise<{ success: boolean; error?: string }> {
    if (this.syncInProgress) {
      return { success: false, error: 'Sync already in progress' };
    }

    this.syncInProgress = true;
    this.updateStatus({ isSyncing: true, lastError: null });

    try {
      // Step 1: Process pending transactions
      const txResult = await transactionQueue.processQueue(submitTransaction);
      
      // Step 2: Fetch fresh data from server
      const serverData = await fetchServerData();
      
      // Step 3: Resolve conflicts (if any)
      await this.resolveConflicts(serverData);
      
      // Step 4: Update last sync time
      this.updateStatus({
        lastSyncTime: Date.now(),
        pendingCount: txResult.failed,
        isSyncing: false,
      });

      return { success: txResult.failed === 0, error: txResult.failed > 0 ? `${txResult.failed} transactions failed` : undefined };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown sync error';
      this.updateStatus({ isSyncing: false, lastError: errorMessage });
      return { success: false, error: errorMessage };
    } finally {
      this.syncInProgress = false;
    }
  }

  /**
   * Resolve conflicts between local and server data
   */
  private async resolveConflicts(serverData: { balances: unknown[]; escrows: unknown[] }): Promise<void> {
    // Get local pending transactions that might have conflicts
    const pendingTxs = await transactionQueue.getPendingTransactions();
    
    for (const tx of pendingTxs) {
      if (tx.status === 'conflict') {
        await this.handleConflict(tx, serverData);
      }
    }
  }

  /**
   * Handle a specific conflict
   */
  private async handleConflict(
    tx: CachedTransaction,
    serverData: { balances: unknown[]; escrows: unknown[] }
  ): Promise<void> {
    if (!tx.conflictData) return;

    const resolution = await this.determineResolution(tx.conflictData);
    
    switch (resolution) {
      case 'local':
        // Keep local version, force submit
        await transactionQueue.updateTransactionStatus(tx.id, 'pending');
        break;
      case 'server':
        // Discard local, use server version
        await transactionQueue.deleteTransaction(tx.id);
        break;
      case 'merged':
        // Create merged version
        const mergedTx = this.createMergedTransaction(tx, tx.conflictData);
        await storageService.savePendingTransaction(mergedTx);
        break;
    }
  }

  /**
   * Determine best conflict resolution strategy
   */
  private async determineResolution(conflict: ConflictData): Promise<'local' | 'server' | 'merged'> {
    const now = Date.now();
    const localAge = now - conflict.timestamp;
    const serverAge = conflict.serverState ? now - (conflict.serverState['lastUpdated'] as number || 0) : 0;

    // If local is newer than server, prefer local
    if (localAge > serverAge && localAge < 300000) { // 5 minutes
      return 'local';
    }

    // If server is significantly newer, prefer server
    if (serverAge > localAge && serverAge > 300000) {
      return 'server';
    }

    // Otherwise, try to merge
    return 'merged';
  }

  /**
   * Create a merged transaction from conflict
   */
  private createMergedTransaction(tx: CachedTransaction, conflict: ConflictData): CachedTransaction {
    // Simple merge: use local params but update version
    return {
      ...tx,
      localVersion: tx.localVersion + 1,
      conflictData: {
        ...conflict,
        resolution: 'merged',
      },
    };
  }

  /**
   * Manually resolve a conflict
   */
  async resolveConflictManually(
    txId: string,
    resolution: 'local' | 'server' | 'merged'
  ): Promise<void> {
    const tx = await transactionQueue.getTransaction(txId);
    if (!tx || tx.status !== 'conflict') {
      throw new Error('Transaction not found or not in conflict state');
    }

    switch (resolution) {
      case 'local':
        await transactionQueue.updateTransactionStatus(txId, 'pending');
        break;
      case 'server':
        await transactionQueue.deleteTransaction(txId);
        break;
      case 'merged':
        if (tx.conflictData) {
          const mergedTx = this.createMergedTransaction(tx, tx.conflictData);
          await storageService.savePendingTransaction(mergedTx);
        }
        break;
    }
  }

  /**
   * Check if there are conflicts to resolve
   */
  async hasConflicts(): Promise<boolean> {
    const pending = await transactionQueue.getPendingTransactions();
    return pending.some(tx => tx.status === 'conflict');
  }

  /**
   * Get all conflicts
   */
  async getConflicts(): Promise<CachedTransaction[]> {
    const pending = await transactionQueue.getPendingTransactions();
    return pending.filter(tx => tx.status === 'conflict');
  }

  /**
   * Auto-sync when connection is restored
   */
  async autoSync(
    isOnline: boolean,
    submitTransaction: (tx: CachedTransaction) => Promise<{ success: boolean; error?: string }>,
    fetchServerData: () => Promise<{ balances: unknown[]; escrows: unknown[] }>
  ): Promise<void> {
    if (!isOnline) return;

    const prefs = await storageService.getPreferences('user');
    if (prefs?.autoSync) {
      await this.sync(submitTransaction, fetchServerData);
    }
  }

  /**
   * Get time since last sync
   */
  getTimeSinceLastSync(): string | null {
    if (!this.lastSyncStatus.lastSyncTime) return null;
    
    const seconds = Math.floor((Date.now() - this.lastSyncStatus.lastSyncTime) / 1000);
    
    if (seconds < 60) return 'Just now';
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
    return `${Math.floor(seconds / 86400)}d ago`;
  }
}

export const syncService = new SyncService();
export default syncService;
