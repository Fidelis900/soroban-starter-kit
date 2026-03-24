import React from 'react';
import { useTransactionQueue } from '../context/TransactionQueueContext';
import { useConnectivity } from '../context/ConnectivityContext';

/**
 * SyncStatus Component
 * Shows the current sync status and allows manual sync
 */
export function SyncStatus(): JSX.Element {
  const { syncStatus, syncNow } = useTransactionQueue();
  const { isOnline } = useConnectivity();

  const formatLastSync = (): string => {
    if (!syncStatus.lastSyncTime) return 'Never';
    
    const now = Date.now();
    const diff = now - syncStatus.lastSyncTime;
    
    if (diff < 60000) return 'Just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)} minutes ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)} hours ago`;
    return new Date(syncStatus.lastSyncTime).toLocaleDateString();
  };

  return (
    <div className="card">
      <div className="card-header">
        <span className="card-title">Sync Status</span>
        {syncStatus.isSyncing && (
          <span className="spinner" />
        )}
      </div>

      <div className="flex flex-col gap-md">
        <div className="flex justify-between items-center">
          <span className="text-muted">Last Sync</span>
          <span>{formatLastSync()}</span>
        </div>

        <div className="flex justify-between items-center">
          <span className="text-muted">Pending Transactions</span>
          <span className={syncStatus.pendingCount > 0 ? 'text-warning' : 'text-success'}>
            {syncStatus.pendingCount}
          </span>
        </div>

        {syncStatus.lastError && (
          <div className="flex justify-between items-center">
            <span className="text-muted">Last Error</span>
            <span className="text-error" style={{ fontSize: '0.875rem', maxWidth: '200px', textAlign: 'right' }}>
              {syncStatus.lastError}
            </span>
          </div>
        )}

        <button
          onClick={syncNow}
          disabled={!isOnline || syncStatus.isSyncing}
          className="btn btn-primary w-full"
        >
          {syncStatus.isSyncing ? (
            <>
              <span className="spinner" style={{ width: '16px', height: '16px' }} />
              Syncing...
            </>
          ) : (
            'Sync Now'
          )}
        </button>
      </div>
    </div>
  );
}

/**
 * OfflineIndicator Component
 * Shows a subtle indicator that data may be cached
 */
export function OfflineIndicator(): JSX.Element | null {
  const { isOnline } = useConnectivity();
  const { syncStatus } = useTransactionQueue();

  if (isOnline) return null;

  return (
    <div className="offline-badge" style={{ marginLeft: 'auto' }}>
      <span>📴</span>
      <span>Offline Mode</span>
      {syncStatus.pendingCount > 0 && (
        <span>• {syncStatus.pendingCount} pending</span>
      )}
    </div>
  );
}

export default SyncStatus;
