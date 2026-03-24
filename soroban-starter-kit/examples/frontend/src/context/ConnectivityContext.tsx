import React, { createContext, useContext, useEffect, useState, useCallback, ReactNode } from 'react';
import { ConnectionStatus } from '../services/storage/types';

interface ConnectivityContextType {
  status: ConnectionStatus;
  isOnline: boolean;
  wasOffline: boolean;
  retryConnection: () => Promise<void>;
}

const ConnectivityContext = createContext<ConnectivityContextType | undefined>(undefined);

interface ConnectivityProviderProps {
  children: ReactNode;
}

export function ConnectivityProvider({ children }: ConnectivityProviderProps): JSX.Element {
  const [status, setStatus] = useState<ConnectionStatus>('offline');
  const [wasOffline, setWasOffline] = useState(false);

  const handleOnline = useCallback(() => {
    setStatus('online');
    if (wasOffline) {
      // Trigger sync when coming back online
      window.dispatchEvent(new CustomEvent('app:online'));
    }
  }, [wasOffline]);

  const handleOffline = useCallback(() => {
    setStatus('offline');
    setWasOffline(true);
    window.dispatchEvent(new CustomEvent('app:offline'));
  }, []);

  const retryConnection = useCallback(async () => {
    setStatus('syncing');
    try {
      // Try to make a simple request to check connectivity
      const response = await fetch('https://soroban-testnet.stellar.org', {
        method: 'HEAD',
        mode: 'no-cors',
      });
      setStatus('online');
    } catch {
      setStatus('offline');
    }
  }, []);

  useEffect(() => {
    // Set initial status
    if (navigator.onLine) {
      setStatus('online');
    } else {
      setStatus('offline');
      setWasOffline(true);
    }

    // Add event listeners
    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    // Periodic connectivity check
    const interval = setInterval(() => {
      if (status !== 'syncing') {
        if (navigator.onLine) {
          if (status === 'offline') {
            handleOnline();
          }
        } else {
          handleOffline();
        }
      }
    }, 5000);

    return () => {
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
      clearInterval(interval);
    };
  }, [handleOnline, handleOffline, status]);

  return (
    <ConnectivityContext.Provider
      value={{
        status,
        isOnline: status === 'online',
        wasOffline,
        retryConnection,
      }}
    >
      {children}
    </ConnectivityContext.Provider>
  );
}

export function useConnectivity(): ConnectivityContextType {
  const context = useContext(ConnectivityContext);
  if (!context) {
    throw new Error('useConnectivity must be used within a ConnectivityProvider');
  }
  return context;
}

export default ConnectivityContext;
