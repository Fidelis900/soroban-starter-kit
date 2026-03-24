import React from 'react';
import { Balance } from '../services/storage/types';
import { useConnectivity } from '../context/ConnectivityContext';

interface BalanceDisplayProps {
  balance: Balance;
  showOfflineBadge?: boolean;
}

/**
 * BalanceDisplay Component
 * Displays a token balance with offline indicator
 */
export function BalanceDisplay({ balance, showOfflineBadge = true }: BalanceDisplayProps): JSX.Element {
  const { isOnline } = useConnectivity();

  const formatBalance = (amount: string): string => {
    // Convert from stroops (7 decimals) to human readable
    try {
      const num = BigInt(amount);
      const stroops = 10000000;
      const wholePart = num / BigInt(stroops);
      const fractionalPart = num % BigInt(stroops);
      
      if (fractionalPart === BigInt(0)) {
        return wholePart.toString();
      }
      
      const fractionalStr = fractionalPart.toString().padStart(7, '0');
      return `${wholePart}.${fractionalStr.slice(0, 2)}`;
    } catch {
      return amount;
    }
  };

  const formatLastUpdated = (timestamp: number): string => {
    const now = Date.now();
    const diff = now - timestamp;
    
    if (diff < 60000) return 'Just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return new Date(timestamp).toLocaleDateString();
  };

  const isStale = Date.now() - balance.lastUpdated > 300000; // 5 minutes

  return (
    <div className="card balance-card">
      <div className="card-header">
        <span className="card-title">{balance.tokenSymbol}</span>
        {showOfflineBadge && !isOnline && (
          <span className="offline-badge">
            📴 Cached
          </span>
        )}
        {showOfflineBadge && isOnline && isStale && (
          <span className="offline-badge synced">
            ✓ Synced
          </span>
        )}
      </div>
      
      <div className="balance-amount">
        {formatBalance(balance.amount)}
      </div>
      
      <div className="balance-last-updated">
        Last updated: {formatLastUpdated(balance.lastUpdated)}
        {!isOnline && ' (offline)'}
      </div>
    </div>
  );
}

/**
 * BalanceList Component
 * Displays a list of token balances
 */
interface BalanceListProps {
  balances: Balance[];
  showOfflineBadge?: boolean;
  emptyMessage?: string;
}

export function BalanceList({ 
  balances, 
  showOfflineBadge = true,
  emptyMessage = 'No cached balances',
}: BalanceListProps): JSX.Element {
  if (balances.length === 0) {
    return (
      <div className="card" style={{ textAlign: 'center', padding: '32px' }}>
        <p className="text-muted">{emptyMessage}</p>
      </div>
    );
  }

  return (
    <div className="grid grid-2">
      {balances.map((balance) => (
        <BalanceDisplay 
          key={balance.id} 
          balance={balance}
          showOfflineBadge={showOfflineBadge}
        />
      ))}
    </div>
  );
}

export default BalanceDisplay;
