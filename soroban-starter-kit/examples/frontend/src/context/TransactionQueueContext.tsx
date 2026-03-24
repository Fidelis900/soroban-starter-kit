import React, { createContext, useContext, useEffect, useState, useCallback, ReactNode } from 'react';
import { transactionQueue } from '../services/transactionQueue';
import { syncService } from '../services/sync';
import { CachedTransaction, SyncStatus, TransactionType } from '../services/storage/types';
import { useConnectivity } from './ConnectivityContext';

interface TransactionQueueContextType {
  pendingTransactions: CachedTransaction[];
  syncedTransactions: CachedTransaction[];
  syncStatus: SyncStatus;
  isInitialized: boolean;
  createTransaction: (
    type: TransactionType,
    contractId: string,
    method: string,
    params: Record<string, unknown>
  ) => Promise<CachedTransaction>;
  syncNow: () => Promise<void>;
  retryTransaction: (id: string) => Promise<void>;
  deleteTransaction: (id: string) => Promise<void>;
  resolveConflict: (id: string, resolution: 'local' | 'server' | 'merged') => Promise<void>;
}

const TransactionQueueContext = createContext<TransactionQueueContextType | undefined>(undefined);

interface TransactionQueueProviderProps {
  children: ReactNode;
  submitTransaction?: (tx: CachedTransaction) => Promise<{ success: boolean; error?: string }>;
  fetchServerData?: () => Promise<{ balances: unknown[]; escrows: unknown[] }>;
}

export function TransactionQueueProvider({ 
  children,
  submitTransaction,
  fetchServerData,
}: TransactionQueueProviderProps): JSX.Element {
  const { isOnline } = useConnectivity();
  const [pendingTransactions, setPendingTransactions] = useState<CachedTransaction[]>([]);
  const [syncedTransactions, setSyncedTransactions] = useState<CachedTransaction[]>([]);
  const [syncStatus, setSyncStatus] = useState<SyncStatus>({
    lastSyncTime: null,
    pendingCount: 0,
    isSyncing: false,
    lastError: null,
  });
  const [isInitialized, setIsInitialized] = useState(false);

  const refreshTransactions = useCallback(async () => {
    try {
      const [pending, synced] = await Promise.all([
        transactionQueue.getPendingTransactions(),
        transactionQueue.getSyncedTransactions(),
      ]);
      setPendingTransactions(pending);
      setSyncedTransactions(synced);
    } catch (error) {
      console.error('Failed to refresh transactions:', error);
    }
  }, []);

  const createTransaction = useCallback(
    async (
      type: TransactionType,
      contractId: string,
      method: string,
      params: Record<string, unknown>
    ): Promise<CachedTransaction> => {
      const validation = transactionQueue.validateTransaction(type, params);
      if (!validation.valid) {
        throw new Error(validation.errors.join(', '));
      }

      const tx = await transactionQueue.createTransaction(type, contractId, method, params);
      await refreshTransactions();
      return tx;
    },
    [refreshTransactions]
  );

  const syncNow = useCallback(async () => {
    if (!submitTransaction || !fetchServerData) {
      console.warn('Sync functions not provided');
      return;
    }

    const result = await syncService.sync(submitTransaction, fetchServerData);
    if (result.success) {
      await refreshTransactions();
    }
  }, [submitTransaction, fetchServerData, refreshTransactions]);

  const retryTransaction = useCallback(
    async (id: string) => {
      if (!submitTransaction) {
        console.warn('Submit function not provided');
        return;
      }

      await transactionQueue.retryTransaction(id, submitTransaction);
      await refreshTransactions();
    },
    [submitTransaction, refreshTransactions]
  );

  const deleteTransaction = useCallback(
    async (id: string) => {
      await transactionQueue.deleteTransaction(id);
      await refreshTransactions();
    },
    [refreshTransactions]
  );

  const resolveConflict = useCallback(
    async (id: string, resolution: 'local' | 'server' | 'merged') => {
      await syncService.resolveConflictManually(id, resolution);
      await refreshTransactions();
    },
    [refreshTransactions]
  );

  // Initialize
  useEffect(() => {
    const init = async () => {
      await transactionQueue.init();
      await syncService.init();
      await refreshTransactions();
      setIsInitialized(true);
    };
    init();
  }, [refreshTransactions]);

  // Subscribe to sync status
  useEffect(() => {
    const unsubscribe = syncService.subscribe((status) => {
      setSyncStatus(status);
    });
    return unsubscribe;
  }, []);

  // Auto-sync when coming online
  useEffect(() => {
    if (isOnline && isInitialized && submitTransaction && fetchServerData) {
      syncService.autoSync(isOnline, submitTransaction, fetchServerData);
    }
  }, [isOnline, isInitialized, submitTransaction, fetchServerData]);

  // Listen for online/offline events
  useEffect(() => {
    const handleOnline = () => {
      if (submitTransaction && fetchServerData) {
        syncNow();
      }
    };

    window.addEventListener('app:online', handleOnline);
    return () => window.removeEventListener('app:online', handleOnline);
  }, [syncNow, submitTransaction, fetchServerData]);

  return (
    <TransactionQueueContext.Provider
      value={{
        pendingTransactions,
        syncedTransactions,
        syncStatus,
        isInitialized,
        createTransaction,
        syncNow,
        retryTransaction,
        deleteTransaction,
        resolveConflict,
      }}
    >
      {children}
    </TransactionQueueContext.Provider>
  );
}

export function useTransactionQueue(): TransactionQueueContextType {
  const context = useContext(TransactionQueueContext);
  if (!context) {
    throw new Error('useTransactionQueue must be used within a TransactionQueueProvider');
  }
  return context;
}

export default TransactionQueueContext;
