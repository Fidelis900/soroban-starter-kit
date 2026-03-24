import { storageService } from './storage';
import { CachedTransaction, TransactionStatus, TransactionType } from './storage/types';

/**
 * Transaction Queue Manager
 * Handles offline transaction queuing, preparation, and submission
 */
class TransactionQueueService {
  private maxRetries = 3;
  private retryDelay = 2000; // 2 seconds

  /**
   * Initialize the transaction queue
   */
  async init(): Promise<void> {
    await storageService.init();
  }

  /**
   * Create a new pending transaction
   */
  async createTransaction(
    type: TransactionType,
    contractId: string,
    method: string,
    params: Record<string, unknown>
  ): Promise<CachedTransaction> {
    const transaction: CachedTransaction = {
      id: `tx_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      type,
      contractId,
      method,
      params,
      status: 'pending',
      createdAt: Date.now(),
      retryCount: 0,
      localVersion: 1,
    };

    await storageService.savePendingTransaction(transaction);
    return transaction;
  }

  /**
   * Get all pending transactions
   */
  async getPendingTransactions(): Promise<CachedTransaction[]> {
    return storageService.getPendingTransactions();
  }

  /**
   * Get transaction by ID
   */
  async getTransaction(id: string): Promise<CachedTransaction | undefined> {
    return storageService.getPendingTransaction(id);
  }

  /**
   * Update transaction status
   */
  async updateTransactionStatus(
    id: string,
    status: TransactionStatus,
    error?: string
  ): Promise<void> {
    const tx = await storageService.getPendingTransaction(id);
    if (!tx) return;

    const updatedTx: CachedTransaction = {
      ...tx,
      status,
      error,
      lastAttempt: Date.now(),
      retryCount: status === 'failed' ? tx.retryCount + 1 : tx.retryCount,
    };

    await storageService.savePendingTransaction(updatedTx);
  }

  /**
   * Mark transaction as synced
   */
  async markAsSynced(id: string): Promise<void> {
    const tx = await storageService.getPendingTransaction(id);
    if (!tx) return;

    await storageService.markTransactionSynced(tx);
  }

  /**
   * Delete a pending transaction
   */
  async deleteTransaction(id: string): Promise<void> {
    await storageService.deletePendingTransaction(id);
  }

  /**
   * Get count of pending transactions
   */
  async getPendingCount(): Promise<number> {
    const transactions = await storageService.getPendingTransactions();
    return transactions.length;
  }

  /**
   * Get all synced transactions
   */
  async getSyncedTransactions(): Promise<CachedTransaction[]> {
    return storageService.getSyncedTransactions();
  }

  /**
   * Retry failed transactions
   */
  async retryTransaction(
    id: string,
    submitFn: (tx: CachedTransaction) => Promise<{ success: boolean; error?: string }>
  ): Promise<{ success: boolean; error?: string }> {
    const tx = await storageService.getPendingTransaction(id);
    if (!tx) {
      return { success: false, error: 'Transaction not found' };
    }

    if (tx.retryCount >= this.maxRetries) {
      await this.updateTransactionStatus(id, 'failed', 'Max retries exceeded');
      return { success: false, error: 'Max retries exceeded' };
    }

    await this.updateTransactionStatus(id, 'syncing');

    try {
      const result = await submitFn(tx);
      
      if (result.success) {
        await this.markAsSynced(id);
      } else {
        await this.updateTransactionStatus(id, 'pending', result.error);
        // Wait before allowing retry
        await this.delay(this.retryDelay);
      }
      
      return result;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      await this.updateTransactionStatus(id, 'pending', errorMessage);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Process all pending transactions
   */
  async processQueue(
    submitFn: (tx: CachedTransaction) => Promise<{ success: boolean; error?: string }>
  ): Promise<{ processed: number; succeeded: number; failed: number }> {
    const pending = await this.getPendingTransactions();
    let succeeded = 0;
    let failed = 0;

    for (const tx of pending) {
      if (tx.status === 'pending') {
        const result = await this.retryTransaction(tx.id, submitFn);
        if (result.success) {
          succeeded++;
        } else {
          failed++;
        }
      }
    }

    return {
      processed: pending.length,
      succeeded,
      failed,
    };
  }

  /**
   * Clear all pending transactions
   */
  async clearQueue(): Promise<void> {
    const pending = await this.getPendingTransactions();
    for (const tx of pending) {
      await storageService.deletePendingTransaction(tx.id);
    }
  }

  /**
   * Get transaction details for display
   */
  getTransactionDetails(tx: CachedTransaction): {
    title: string;
    description: string;
    icon: string;
  } {
    const typeMap: Record<TransactionType, { title: string; description: string; icon: string }> = {
      transfer: { title: 'Transfer', description: `Send tokens to ${tx.params.to || 'recipient'}`, icon: '↑' },
      mint: { title: 'Mint', description: 'Create new tokens', icon: '✦' },
      burn: { title: 'Burn', description: 'Destroy tokens', icon: '✧' },
      approve: { title: 'Approve', description: 'Allow spender', icon: '✓' },
      escrow_fund: { title: 'Fund Escrow', description: 'Add funds to escrow', icon: '◈' },
      escrow_release: { title: 'Release Funds', description: 'Release funds to seller', icon: '◉' },
      escrow_refund: { title: 'Refund', description: 'Refund to buyer', icon: '↺' },
    };

    return typeMap[tx.type] || { title: 'Transaction', description: 'Unknown type', icon: '•' };
  }

  /**
   * Validate transaction before queuing
   */
  validateTransaction(type: TransactionType, params: Record<string, unknown>): {
    valid: boolean;
    errors: string[];
  } {
    const errors: string[] = [];

    switch (type) {
      case 'transfer':
        if (!params.to) errors.push('Recipient address is required');
        if (!params.amount || params.amount === '0') errors.push('Amount must be greater than 0');
        break;
      case 'escrow_fund':
        if (!params.escrowId) errors.push('Escrow ID is required');
        break;
      case 'escrow_release':
      case 'escrow_refund':
        if (!params.escrowId) errors.push('Escrow ID is required');
        break;
    }

    return { valid: errors.length === 0, errors };
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

export const transactionQueue = new TransactionQueueService();
export default transactionQueue;
