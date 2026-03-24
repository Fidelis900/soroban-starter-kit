import { openDB, DBSchema, IDBPDatabase } from 'idb';
import { Balance, EscrowData, CachedTransaction, UserPreferences } from './types';

/**
 * FidelisDB - IndexedDB database for offline storage
 * Provides persistent storage for balances, transactions, and cached data
 */
interface FidelisDBSchema extends DBSchema {
  balances: {
    key: string;
    value: Balance;
    indexes: { 'by-address': string; 'by-timestamp': number };
  };
  escrows: {
    key: string;
    value: EscrowData;
    indexes: { 'by-status': string; 'by-address': string };
  };
  pendingTransactions: {
    key: string;
    value: CachedTransaction;
    indexes: { 'by-status': string; 'by-timestamp': number };
  };
  syncedTransactions: {
    key: string;
    value: CachedTransaction;
    indexes: { 'by-timestamp': number };
  };
  preferences: {
    key: string;
    value: UserPreferences;
  };
  cache: {
    key: string;
    value: { data: unknown; timestamp: number; expiresAt: number };
  };
}

const DB_NAME = 'fidelis-soroban-db';
const DB_VERSION = 1;

class StorageService {
  private db: IDBPDatabase<FidelisDBSchema> | null = null;
  private initPromise: Promise<void> | null = null;

  /**
   * Initialize the IndexedDB database
   */
  async init(): Promise<void> {
    if (this.db) return;
    if (this.initPromise) return this.initPromise;

    this.initPromise = this.initializeDB();
    return this.initPromise;
  }

  private async initializeDB(): Promise<void> {
    this.db = await openDB<FidelisDBSchema>(DB_NAME, DB_VERSION, {
      upgrade(db) {
        // Balances store
        if (!db.objectStoreNames.contains('balances')) {
          const balanceStore = db.createObjectStore('balances', { keyPath: 'id' });
          balanceStore.createIndex('by-address', 'address');
          balanceStore.createIndex('by-timestamp', 'lastUpdated');
        }

        // Escrows store
        if (!db.objectStoreNames.contains('escrows')) {
          const escrowStore = db.createObjectStore('escrows', { keyPath: 'id' });
          escrowStore.createIndex('by-status', 'status');
          escrowStore.createIndex('by-address', 'buyer');
        }

        // Pending transactions store (for offline queue)
        if (!db.objectStoreNames.contains('pendingTransactions')) {
          const txStore = db.createObjectStore('pendingTransactions', { keyPath: 'id' });
          txStore.createIndex('by-status', 'status');
          txStore.createIndex('by-timestamp', 'createdAt');
        }

        // Synced transactions store (history)
        if (!db.objectStoreNames.contains('syncedTransactions')) {
          const syncedStore = db.createObjectStore('syncedTransactions', { keyPath: 'id' });
          syncedStore.createIndex('by-timestamp', 'createdAt');
        }

        // User preferences store
        if (!db.objectStoreNames.contains('preferences')) {
          db.createObjectStore('preferences', { keyPath: 'id' });
        }

        // Generic cache store
        if (!db.objectStoreNames.contains('cache')) {
          db.createObjectStore('cache', { keyPath: 'key' });
        }
      },
    });
  }

  private ensureDB(): IDBPDatabase<FidelisDBSchema> {
    if (!this.db) {
      throw new Error('Database not initialized. Call init() first.');
    }
    return this.db;
  }

  // ==================== BALANCES ====================

  /**
   * Save a balance to offline storage
   */
  async saveBalance(balance: Balance): Promise<void> {
    const db = this.ensureDB();
    await db.put('balances', {
      ...balance,
      lastUpdated: Date.now(),
    });
  }

  /**
   * Get balance for a specific address
   */
  async getBalance(address: string, contractId: string): Promise<Balance | undefined> {
    const db = this.ensureDB();
    const id = `${contractId}_${address}`;
    return db.get('balances', id);
  }

  /**
   * Get all cached balances for an address
   */
  async getBalancesByAddress(address: string): Promise<Balance[]> {
    const db = this.ensureDB();
    return db.getAllFromIndex('balances', 'by-address', address);
  }

  /**
   * Get all cached balances
   */
  async getAllBalances(): Promise<Balance[]> {
    const db = this.ensureDB();
    return db.getAll('balances');
  }

  // ==================== ESCROWS ====================

  /**
   * Save escrow data to offline storage
   */
  async saveEscrow(escrow: EscrowData): Promise<void> {
    const db = this.ensureDB();
    await db.put('escrows', {
      ...escrow,
      lastUpdated: Date.now(),
    });
  }

  /**
   * Get escrow by ID
   */
  async getEscrow(id: string): Promise<EscrowData | undefined> {
    const db = this.ensureDB();
    return db.get('escrows', id);
  }

  /**
   * Get all escrows by status
   */
  async getEscrowsByStatus(status: string): Promise<EscrowData[]> {
    const db = this.ensureDB();
    return db.getAllFromIndex('escrows', 'by-status', status);
  }

  /**
   * Get all cached escrows
   */
  async getAllEscrows(): Promise<EscrowData[]> {
    const db = this.ensureDB();
    return db.getAll('escrows');
  }

  // ==================== TRANSACTIONS ====================

  /**
   * Save a pending transaction to the queue
   */
  async savePendingTransaction(tx: CachedTransaction): Promise<void> {
    const db = this.ensureDB();
    await db.put('pendingTransactions', tx);
  }

  /**
   * Get all pending transactions
   */
  async getPendingTransactions(): Promise<CachedTransaction[]> {
    const db = this.ensureDB();
    return db.getAll('pendingTransactions');
  }

  /**
   * Get pending transaction by ID
   */
  async getPendingTransaction(id: string): Promise<CachedTransaction | undefined> {
    const db = this.ensureDB();
    return db.get('pendingTransactions', id);
  }

  /**
   * Delete a pending transaction
   */
  async deletePendingTransaction(id: string): Promise<void> {
    const db = this.ensureDB();
    await db.delete('pendingTransactions', id);
  }

  /**
   * Move transaction to synced store after successful submission
   */
  async markTransactionSynced(tx: CachedTransaction): Promise<void> {
    const db = this.ensureDB();
    await db.delete('pendingTransactions', tx.id);
    await db.put('syncedTransactions', {
      ...tx,
      status: 'synced',
      syncedAt: Date.now(),
    });
  }

  /**
   * Get all synced transactions
   */
  async getSyncedTransactions(): Promise<CachedTransaction[]> {
    const db = this.ensureDB();
    return db.getAll('syncedTransactions');
  }

  // ==================== PREFERENCES ====================

  /**
   * Save user preferences
   */
  async savePreferences(prefs: UserPreferences): Promise<void> {
    const db = this.ensureDB();
    await db.put('preferences', prefs);
  }

  /**
   * Get user preferences
   */
  async getPreferences(id: string): Promise<UserPreferences | undefined> {
    const db = this.ensureDB();
    return db.get('preferences', id);
  }

  // ==================== CACHE ====================

  /**
   * Store cached data with expiration
   */
  async setCache(key: string, data: unknown, ttlSeconds: number = 3600): Promise<void> {
    const db = this.ensureDB();
    const now = Date.now();
    await db.put('cache', {
      key,
      data,
      timestamp: now,
      expiresAt: now + ttlSeconds * 1000,
    });
  }

  /**
   * Get cached data if not expired
   */
  async getCache<T>(key: string): Promise<T | null> {
    const db = this.ensureDB();
    const cached = await db.get('cache', key);
    
    if (!cached) return null;
    if (Date.now() > cached.expiresAt) {
      await db.delete('cache', key);
      return null;
    }
    
    return cached.data as T;
  }

  /**
   * Clear expired cache entries
   */
  async clearExpiredCache(): Promise<void> {
    const db = this.ensureDB();
    const tx = db.transaction('cache', 'readwrite');
    const store = tx.objectStore('cache');
    const keys = await store.getAllKeys();
    const now = Date.now();

    for (const key of keys) {
      const entry = await store.get(key);
      if (entry && now > entry.expiresAt) {
        await store.delete(key);
      }
    }
  }

  // ==================== UTILITIES ====================

  /**
   * Clear all offline data
   */
  async clearAll(): Promise<void> {
    const db = this.ensureDB();
    await db.clear('balances');
    await db.clear('escrows');
    await db.clear('pendingTransactions');
    await db.clear('syncedTransactions');
    await db.clear('cache');
  }

  /**
   * Get storage usage estimate
   */
  async getStorageEstimate(): Promise<{ used: number; quota: number }> {
    if (navigator.storage && navigator.storage.estimate) {
      const estimate = await navigator.storage.estimate();
      return {
        used: estimate.usage || 0,
        quota: estimate.quota || 0,
      };
    }
    return { used: 0, quota: 0 };
  }
}

// Export singleton instance
export const storageService = new StorageService();
export default storageService;
