import React, { useEffect, useState } from 'react';
import { ConnectivityStatus, OfflineBanner } from './components/ConnectivityStatus';
import { TransactionList } from './components/TransactionItem';
import { BalanceList } from './components/BalanceDisplay';
import { SyncStatus, OfflineIndicator } from './components/SyncStatus';
import { useConnectivity } from './context/ConnectivityContext';
import { useStorage } from './context/StorageContext';
import { useTransactionQueue } from './context/TransactionQueueContext';

/**
 * Main App Component
 * Demonstrates offline functionality with balances and transactions
 */
function App(): JSX.Element {
  const { isOnline } = useConnectivity();
  const { balances, escrows, isInitialized } = useStorage();
  const { 
    pendingTransactions, 
    syncedTransactions, 
    syncStatus,
    createTransaction,
    syncNow,
    retryTransaction,
    deleteTransaction,
    resolveConflict,
  } = useTransactionQueue();

  const [activeTab, setActiveTab] = useState<'balances' | 'pending' | 'history'>('balances');
  const [isDemoLoading, setIsDemoLoading] = useState(false);

  // Demo function to simulate transaction submission
  const handleSubmitTransaction = async (): Promise<void> => {
    setIsDemoLoading(true);
    try {
      await createTransaction(
        'transfer',
        'CONTRACT_ID_HERE',
        'transfer',
        {
          to: 'GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX',
          amount: '10000000', // 1 token
        }
      );
    } catch (error) {
      console.error('Failed to create transaction:', error);
    } finally {
      setIsDemoLoading(false);
    }
  };

  if (!isInitialized) {
    return (
      <div className="container" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100vh' }}>
        <div className="flex flex-col items-center gap-md">
          <span className="spinner" style={{ width: '32px', height: '32px' }} />
          <span>Initializing offline storage...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen">
      {/* Offline Banner */}
      <OfflineBanner />

      {/* Header */}
      <header className="header container">
        <div className="flex items-center gap-md">
          <h1>🔐 Fidelis</h1>
          <span className="text-muted">Soroban DApp</span>
        </div>
        
        <div className="flex items-center gap-md">
          <OfflineIndicator />
          <ConnectivityStatus />
        </div>
      </header>

      {/* Main Content */}
      <main className="main-content container">
        {/* Demo Section - Create Transaction */}
        <section className="card mb-lg">
          <div className="card-header">
            <span className="card-title">Quick Actions</span>
          </div>
          
          <div className="flex gap-md items-center">
            <button
              onClick={handleSubmitTransaction}
              disabled={isDemoLoading}
              className="btn btn-primary"
            >
              {isDemoLoading ? (
                <>
                  <span className="spinner" style={{ width: '16px', height: '16px' }} />
                  Creating...
                </>
              ) : (
                '＋ Queue Transfer (Demo)'
              )}
            </button>
            
            <button
              onClick={syncNow}
              disabled={!isOnline || syncStatus.isSyncing}
              className="btn btn-secondary"
            >
              {syncStatus.isSyncing ? 'Syncing...' : 'Sync Now'}
            </button>
            
            <span className="text-muted" style={{ marginLeft: 'auto' }}>
              {pendingTransactions.length} pending • {syncedTransactions.length} synced
            </span>
          </div>
        </section>

        {/* Tab Navigation */}
        <div className="flex gap-md mb-lg" style={{ borderBottom: '1px solid var(--color-border)', paddingBottom: 'var(--spacing-md)' }}>
          <button
            onClick={() => setActiveTab('balances')}
            className={activeTab === 'balances' ? 'btn btn-primary' : 'btn btn-secondary'}
            style={{ backgroundColor: activeTab === 'balances' ? 'var(--color-highlight)' : 'transparent' }}
          >
            📊 Cached Balances
          </button>
          <button
            onClick={() => setActiveTab('pending')}
            className={activeTab === 'pending' ? 'btn btn-primary' : 'btn btn-secondary'}
            style={{ backgroundColor: activeTab === 'pending' ? 'var(--color-highlight)' : 'transparent' }}
          >
            ⏳ Pending ({pendingTransactions.length})
          </button>
          <button
            onClick={() => setActiveTab('history')}
            className={activeTab === 'history' ? 'btn btn-primary' : 'btn btn-secondary'}
            style={{ backgroundColor: activeTab === 'history' ? 'var(--color-highlight)' : 'transparent' }}
          >
            ✓ Synced History ({syncedTransactions.length})
          </button>
        </div>

        {/* Content Area */}
        <div className="grid" style={{ gridTemplateColumns: '1fr 300px' }}>
          {/* Main Panel */}
          <div>
            {activeTab === 'balances' && (
              <>
                <h2 className="mb-md">Token Balances</h2>
                {isOnline ? (
                  <p className="text-muted mb-md">
                    You're online. Balances are fetched from the network.
                  </p>
                ) : (
                  <p className="text-warning mb-md">
                    You're offline. Showing cached balances from your last online session.
                  </p>
                )}
                <BalanceList 
                  balances={balances}
                  emptyMessage="No cached balances. Connect to the network to fetch your balances."
                />
                
                {/* Show demo balances when empty */}
                {balances.length === 0 && (
                  <div className="card mt-lg" style={{ textAlign: 'center' }}>
                    <p className="text-muted">
                      💡 In a real app, your token balances will be cached automatically when you go offline.
                    </p>
                  </div>
                )}
              </>
            )}

            {activeTab === 'pending' && (
              <>
                <h2 className="mb-md">Pending Transactions</h2>
                {!isOnline && (
                  <p className="text-warning mb-md">
                    You're offline. Transactions will be queued and submitted when connection is restored.
                  </p>
                )}
                <TransactionList
                  transactions={pendingTransactions}
                  onRetry={retryTransaction}
                  onDelete={deleteTransaction}
                  onResolveConflict={resolveConflict}
                  emptyMessage="No pending transactions. Your transactions will appear here when queued."
                />
              </>
            )}

            {activeTab === 'history' && (
              <>
                <h2 className="mb-md">Synced Transactions</h2>
                <TransactionList
                  transactions={syncedTransactions}
                  emptyMessage="No synced transactions yet."
                />
              </>
            )}
          </div>

          {/* Sidebar */}
          <div>
            <SyncStatus />
            
            {/* Storage Info */}
            <div className="card mt-lg">
              <div className="card-header">
                <span className="card-title">Storage</span>
              </div>
              
              <div className="flex flex-col gap-sm">
                <div className="flex justify-between">
                  <span className="text-muted">Cached Items</span>
                  <span>{balances.length + escrows.length}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted">Transactions</span>
                  <span>{pendingTransactions.length + syncedTransactions.length}</span>
                </div>
              </div>
            </div>

            {/* Offline Info */}
            <div className="card mt-lg">
              <div className="card-header">
                <span className="card-title">ℹ️ Offline Mode</span>
              </div>
              
              <div className="flex flex-col gap-sm text-muted" style={{ fontSize: '0.875rem' }}>
                <p>
                  • Your balances are cached automatically
                </p>
                <p>
                  • Transactions are queued when offline
                </p>
                <p>
                  • Data syncs when connection restores
                </p>
                <p>
                  • Conflicts are resolved automatically
                </p>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
