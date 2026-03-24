import React, { createContext, useContext, useState, useEffect } from 'react';
import { Integration, MarketplaceState, DeveloperSubmission, Review } from '../components/marketplace/types';

interface MarketplaceContextType extends MarketplaceState {
  installIntegration: (id: string, config?: Record<string, string>) => Promise<void>;
  uninstallIntegration: (id: string) => Promise<void>;
  submitIntegration: (submission: Omit<DeveloperSubmission, 'id' | 'status' | 'submittedAt'>) => Promise<void>;
  submitReview: (review: Omit<Review, 'id' | 'date'>) => Promise<void>;
}

const MarketplaceContext = createContext<MarketplaceContextType | undefined>(undefined);

// Mock Data for Discovery
const MOCK_INTEGRATIONS: Integration[] = [
  {
    id: 'int-1',
    name: 'DeFi Swap Widget',
    description: 'Instantly swap assets directly from the dashboard.',
    developer: 'SorobanTech',
    version: '1.2.0',
    rating: 4.8,
    reviewsCount: 142,
    isCompatible: true,
    requiredPermissions: ['sign_transaction'],
    category: 'defi',
    status: 'available',
    iconUrl: 'https://api.dicebear.com/7.x/identicon/svg?seed=defi_swap',
  },
  {
    id: 'int-2',
    name: 'Portfolio Tracker',
    description: 'Advanced analytics and chart visualization for your wallet.',
    developer: 'AnalyticsDAO',
    version: '2.0.1',
    rating: 4.5,
    reviewsCount: 89,
    isCompatible: true,
    requiredPermissions: ['read_balances'],
    category: 'analytics',
    status: 'available',
    iconUrl: 'https://api.dicebear.com/7.x/identicon/svg?seed=portfolio',
  },
  {
    id: 'int-3',
    name: 'Multi-Sig Pro',
    description: 'Manage multi-signature setups with ease and security.',
    developer: 'SafeGuard',
    version: '0.9.5',
    rating: 4.9,
    reviewsCount: 34,
    isCompatible: false, // For testing compatibility UI
    minAppVersion: '2.0.0',
    requiredPermissions: ['sign_transaction', 'manage_keys'],
    category: 'wallet',
    status: 'available',
    iconUrl: 'https://api.dicebear.com/7.x/identicon/svg?seed=multisig',
  }
];

export const MarketplaceProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [availableIntegrations, setAvailableIntegrations] = useState<Integration[]>(MOCK_INTEGRATIONS);
  const [installedIntegrations, setInstalledIntegrations] = useState<Integration[]>([]);
  const [submissions, setSubmissions] = useState<DeveloperSubmission[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load installed from localStorage (simulating offline-first IDB)
  useEffect(() => {
    const saved = localStorage.getItem('fidelis_installed_plugins');
    if (saved) {
      try {
        setInstalledIntegrations(JSON.parse(saved));
      } catch (e) {
        console.error("Failed to load plugins", e);
      }
    }
  }, []);

  const savePlugins = (plugins: Integration[]) => {
    setInstalledIntegrations(plugins);
    localStorage.setItem('fidelis_installed_plugins', JSON.stringify(plugins));
  };

  const installIntegration = async (id: string, config?: Record<string, string>) => {
    setIsLoading(true);
    try {
      // Simulate network request
      await new Promise(resolve => setTimeout(resolve, 800));
      const integration = availableIntegrations.find(i => i.id === id);
      if (!integration) throw new Error("Integration not found");
      
      const newInstalled = [...installedIntegrations, { ...integration, status: 'installed' as const }];
      savePlugins(newInstalled);
      
      // Update available list to mark as installed
      setAvailableIntegrations(prev => 
        prev.map(i => i.id === id ? { ...i, status: 'installed' } : i)
      );
    } catch (err: any) {
      setError(err.message);
    } finally {
      setIsLoading(false);
    }
  };

  const uninstallIntegration = async (id: string) => {
    setIsLoading(true);
    try {
      await new Promise(resolve => setTimeout(resolve, 500));
      savePlugins(installedIntegrations.filter(i => i.id !== id));
      setAvailableIntegrations(prev => 
        prev.map(i => i.id === id ? { ...i, status: 'available' } : i)
      );
    } catch (err: any) {
      setError(err.message);
    } finally {
      setIsLoading(false);
    }
  };

  const submitIntegration = async (submission: Omit<DeveloperSubmission, 'id' | 'status' | 'submittedAt'>) => {
    setIsLoading(true);
    try {
      await new Promise(resolve => setTimeout(resolve, 1000));
      const newSubmission: DeveloperSubmission = {
        ...submission,
        id: `sub-${Date.now()}`,
        status: 'pending',
        submittedAt: new Date().toISOString()
      };
      setSubmissions(prev => [...prev, newSubmission]);
    } finally {
      setIsLoading(false);
    }
  };

  const submitReview = async (review: Omit<Review, 'id' | 'date'>) => {
    // In a real app this would post to backend. For now we just mock.
    await new Promise(resolve => setTimeout(resolve, 400));
    console.log("Review submitted:", review);
  };

  return (
    <MarketplaceContext.Provider value={{
      availableIntegrations,
      installedIntegrations,
      submissions,
      analytics: null, // Mock data logic can be added here
      isLoading,
      error,
      installIntegration,
      uninstallIntegration,
      submitIntegration,
      submitReview
    }}>
      {children}
    </MarketplaceContext.Provider>
  );
};

export const useMarketplace = () => {
  const context = useContext(MarketplaceContext);
  if (context === undefined) {
    throw new Error('useMarketplace must be used within a MarketplaceProvider');
  }
  return context;
};
