import React, { createContext, useContext, useEffect, useState, ReactNode } from 'react';
import { storageService } from '../services/storage';
import { Balance, EscrowData } from '../services/storage/types';

interface StorageContextType {
  isInitialized: boolean;
  balances: Balance[];
  escrows: EscrowData[];
  storageUsed: number;
  storageQuota: number;
  refreshData: () => Promise<void>;
  saveBalance: (balance: Balance) => Promise<void>;
  saveEscrow: (escrow: EscrowData) => Promise<void>;
  getBalance: (address: string, contractId: string) => Promise<Balance | undefined>;
  getEscrow: (id: string) => Promise<EscrowData | undefined>;
  clearCache: () => Promise<void>;
}

const StorageContext = createContext<StorageContextType | undefined>(undefined);

interface StorageProviderProps {
  children: ReactNode;
}

export function StorageProvider({ children }: StorageProviderProps): JSX.Element {
  const [isInitialized, setIsInitialized] = useState(false);
  const [balances, setBalances] = useState<Balance[]>([]);
  const [escrows, setEscrows] = useState<EscrowData[]>([]);
  const [storageUsed, setStorageUsed] = useState(0);
  const [storageQuota, setStorageQuota] = useState(0);

  const initStorage = async () => {
    try {
      await storageService.init();
      setIsInitialized(true);
      await refreshStorageUsage();
      await refreshData();
    } catch (error) {
      console.error('Failed to initialize storage:', error);
    }
  };

  const refreshData = async () => {
    if (!isInitialized) return;
    
    try {
      const [cachedBalances, cachedEscrows] = await Promise.all([
        storageService.getAllBalances(),
        storageService.getAllEscrows(),
      ]);
      
      setBalances(cachedBalances);
      setEscrows(cachedEscrows);
    } catch (error) {
      console.error('Failed to refresh data:', error);
    }
  };

  const refreshStorageUsage = async () => {
    try {
      const { used, quota } = await storageService.getStorageEstimate();
      setStorageUsed(used);
      setStorageQuota(quota);
    } catch (error) {
      console.error('Failed to get storage estimate:', error);
    }
  };

  const saveBalance = async (balance: Balance) => {
    await storageService.saveBalance(balance);
    await refreshData();
  };

  const saveEscrow = async (escrow: EscrowData) => {
    await storageService.saveEscrow(escrow);
    await refreshData();
  };

  const getBalance = async (address: string, contractId: string): Promise<Balance | undefined> => {
    return storageService.getBalance(address, contractId);
  };

  const getEscrow = async (id: string): Promise<EscrowData | undefined> => {
    return storageService.getEscrow(id);
  };

  const clearCache = async () => {
    await storageService.clearExpiredCache();
    await refreshStorageUsage();
    await refreshData();
  };

  useEffect(() => {
    initStorage();
  }, []);

  return (
    <StorageContext.Provider
      value={{
        isInitialized,
        balances,
        escrows,
        storageUsed,
        storageQuota,
        refreshData,
        saveBalance,
        saveEscrow,
        getBalance,
        getEscrow,
        clearCache,
      }}
    >
      {children}
    </StorageContext.Provider>
  );
}

export function useStorage(): StorageContextType {
  const context = useContext(StorageContext);
  if (!context) {
    throw new Error('useStorage must be used within a StorageProvider');
  }
  return context;
}

export default StorageContext;
