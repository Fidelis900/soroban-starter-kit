import React from 'react';
import { useConnectivity } from '../context/ConnectivityContext';

/**
 * ConnectivityStatus Component
 * Displays the current connection status with visual indicator
 */
export function ConnectivityStatus(): JSX.Element {
  const { status, isOnline, retryConnection } = useConnectivity();

  const getStatusConfig = () => {
    switch (status) {
      case 'online':
        return {
          label: 'Online',
          className: 'online',
          canRetry: false,
        };
      case 'offline':
        return {
          label: 'Offline',
          className: 'offline',
          canRetry: true,
        };
      case 'syncing':
        return {
          label: 'Syncing...',
          className: 'syncing',
          canRetry: false,
        };
      default:
        return {
          label: 'Unknown',
          className: 'offline',
          canRetry: false,
        };
    }
  };

  const config = getStatusConfig();

  return (
    <div className="status-indicator">
      <span className={`status-dot ${config.className}`} />
      <span>{config.label}</span>
      {config.canRetry && (
        <button
          onClick={retryConnection}
          className="btn btn-secondary"
          style={{ marginLeft: '8px', padding: '4px 8px', fontSize: '12px' }}
        >
          Retry
        </button>
      )}
    </div>
  );
}

/**
 * OfflineBanner Component
 * Shows a warning banner when offline
 */
export function OfflineBanner(): JSX.Element | null {
  const { status, wasOffline } = useConnectivity();

  if (status === 'online') {
    return null;
  }

  return (
    <div className={`offline-banner ${status === 'offline' ? '' : 'syncing'}`}>
      {status === 'offline' ? (
        <>
          <span>📴</span>
          <span>You're offline. Transaction will be queued and synced when connection is restored.</span>
        </>
      ) : (
        <>
          <span className="spinner" />
          <span>Reconnecting...</span>
        </>
      )}
    </div>
  );
}

export default ConnectivityStatus;
