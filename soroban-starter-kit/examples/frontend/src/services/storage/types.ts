/**
 * Type definitions for offline storage
 */

// Balance types
export interface Balance {
  id: string;
  address: string;
  contractId: string;
  tokenSymbol: string;
  amount: string;
  lastUpdated: number;
}

// Escrow types
export type EscrowStatus = 'initialized' | 'funded' | 'delivered' | 'completed' | 'cancelled' | 'refunded';

export interface EscrowData {
  id: string;
  contractId: string;
  buyer: string;
  seller: string;
  arbiter?: string;
  tokenContractId: string;
  amount: string;
  status: EscrowStatus;
  deadline: number;
  createdAt: number;
  lastUpdated: number;
}

// Transaction types
export type TransactionStatus = 'pending' | 'syncing' | 'synced' | 'failed' | 'conflict';
export type TransactionType = 'transfer' | 'mint' | 'burn' | 'approve' | 'escrow_fund' | 'escrow_release' | 'escrow_refund';

export interface CachedTransaction {
  id: string;
  type: TransactionType;
  contractId: string;
  method: string;
  params: Record<string, unknown>;
  status: TransactionStatus;
  createdAt: number;
  submittedAt?: number;
  syncedAt?: number;
  error?: string;
  retryCount: number;
  lastAttempt?: number;
  // For conflict resolution
  localVersion: number;
  serverVersion?: number;
  conflictData?: ConflictData;
}

export interface ConflictData {
  localState: Record<string, unknown>;
  serverState: Record<string, unknown>;
  timestamp: number;
  resolution?: 'local' | 'server' | 'merged';
}

// User preferences
export interface UserPreferences {
  id: string;
  network: 'testnet' | 'mainnet';
  autoSync: boolean;
  syncOnReconnect: boolean;
  notifications: boolean;
  theme: 'light' | 'dark' | 'system';
  lastLogin?: number;
}

// Connectivity status
export type ConnectionStatus = 'online' | 'offline' | 'syncing';

// Sync status
export interface SyncStatus {
  lastSyncTime: number | null;
  pendingCount: number;
  isSyncing: boolean;
  lastError: string | null;
}

// API Response types
export interface ApiResponse<T> {
  data?: T;
  error?: string;
  timestamp: number;
}

// Network info
export interface NetworkInfo {
  network: 'testnet' | 'mainnet';
  horizonUrl: string;
  sorobanUrl: string;
}
