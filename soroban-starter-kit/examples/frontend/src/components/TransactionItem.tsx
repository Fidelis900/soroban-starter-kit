import React from 'react';
import { CachedTransaction, TransactionStatus } from '../services/storage/types';
import { transactionQueue } from '../services/transactionQueue';

interface TransactionItemProps {
  transaction: CachedTransaction;
  onRetry?: (id: string) => void;
  onDelete?: (id: string) => void;
  onResolveConflict?: (id: string, resolution: 'local' | 'server' | 'merged') => void;
}

/**
 * TransactionItem Component
 * Displays a single transaction with its status and actions
 */
export function TransactionItem({
  transaction,
  onRetry,
  onDelete,
  onResolveConflict,
}: TransactionItemProps): JSX.Element {
  const details = transactionQueue.getTransactionDetails(transaction);

  const getStatusLabel = (status: TransactionStatus): string => {
    switch (status) {
      case 'pending':
        return 'Pending';
      case 'syncing':
        return 'Syncing';
      case 'synced':
        return 'Synced';
      case 'failed':
        return 'Failed';
      case 'conflict':
        return 'Conflict';
      default:
        return status;
    }
  };

  const formatDate = (timestamp: number): string => {
    const date = new Date(timestamp);
    return date.toLocaleString();
  };

  const formatAmount = (amount: string | number): string => {
    if (typeof amount === 'number') {
      return amount.toLocaleString();
    }
    return amount;
  };

  return (
    <div className="transaction-item">
      <div className="transaction-icon">{details.icon}</div>
      
      <div className="transaction-details">
        <div className="transaction-title">{details.title}</div>
        <div className="transaction-description">
          {details.description}
          {transaction.params.amount && (
            <span> - {formatAmount(transaction.params.amount as string)}</span>
          )}
        </div>
        <div className="text-muted" style={{ fontSize: '0.75rem', marginTop: '4px' }}>
          Created: {formatDate(transaction.createdAt)}
          {transaction.syncedAt && <span> • Synced: {formatDate(transaction.syncedAt)}</span>}
        </div>
        {transaction.error && (
          <div className="text-error" style={{ fontSize: '0.75rem', marginTop: '4px' }}>
            Error: {transaction.error}
          </div>
        )}
      </div>

      <div className="flex flex-col items-center gap-sm">
        <span className={`transaction-status ${transaction.status}`}>
          {transaction.status === 'syncing' && <span className="spinner" style={{ width: '12px', height: '12px' }} />}
          {getStatusLabel(transaction.status)}
        </span>

        {transaction.status === 'failed' && onRetry && (
          <button
            onClick={() => onRetry(transaction.id)}
            className="btn btn-secondary"
            style={{ padding: '4px 8px', fontSize: '0.75rem' }}
            disabled={transaction.retryCount >= 3}
          >
            Retry ({3 - transaction.retryCount})
          </button>
        )}

        {transaction.status === 'conflict' && onResolveConflict && (
          <div className="flex gap-sm">
            <button
              onClick={() => onResolveConflict(transaction.id, 'local')}
              className="btn btn-secondary"
              style={{ padding: '2px 6px', fontSize: '0.65rem' }}
            >
              Keep Local
            </button>
            <button
              onClick={() => onResolveConflict(transaction.id, 'server')}
              className="btn btn-secondary"
              style={{ padding: '2px 6px', fontSize: '0.65rem' }}
            >
              Use Server
            </button>
          </div>
        )}

        {(transaction.status === 'pending' || transaction.status === 'failed') && onDelete && (
          <button
            onClick={() => onDelete(transaction.id)}
            className="btn btn-secondary"
            style={{ padding: '4px 8px', fontSize: '0.75rem', color: 'var(--color-error)' }}
          >
            Cancel
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * TransactionList Component
 * Displays a list of transactions
 */
interface TransactionListProps {
  transactions: CachedTransaction[];
  onRetry?: (id: string) => void;
  onDelete?: (id: string) => void;
  onResolveConflict?: (id: string, resolution: 'local' | 'server' | 'merged') => void;
  emptyMessage?: string;
}

export function TransactionList({
  transactions,
  onRetry,
  onDelete,
  onResolveConflict,
  emptyMessage = 'No transactions',
}: TransactionListProps): JSX.Element {
  if (transactions.length === 0) {
    return (
      <div className="card" style={{ textAlign: 'center', padding: '32px' }}>
        <p className="text-muted">{emptyMessage}</p>
      </div>
    );
  }

  return (
    <div className="transaction-list">
      {transactions.map((tx) => (
        <TransactionItem
          key={tx.id}
          transaction={tx}
          onRetry={onRetry}
          onDelete={onDelete}
          onResolveConflict={onResolveConflict}
        />
      ))}
    </div>
  );
}

export default TransactionItem;
