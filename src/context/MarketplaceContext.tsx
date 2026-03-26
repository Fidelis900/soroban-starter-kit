import React, { createContext, useContext, useState, useEffect, useMemo } from 'react';
import { Integration, MarketplaceState, DeveloperSubmission, Review, MarketplaceAnalytics } from '../components/marketplace/types';

interface MarketplaceContextType extends MarketplaceState {
  installIntegration: (id: string, config?: Record<string, string>) => Promise<void>;
  uninstallIntegration: (id: string) => Promise<void>;
  updateIntegrationConfig: (id: string, config: Record<string, string>) => Promise<void>;
  checkForUpdates: () => Promise<void>;
  submitIntegration: (submission: Omit<DeveloperSubmission, 'id' | 'status' | 'submittedAt'>) => Promise<void>;
  submitReview: (review: Omit<Review, 'id' | 'date'>) => Promise<void>;
  getReviewsForIntegration: (integrationId: string) => Review[];
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
    environment: 'any',
    lastUpdated: '2024-03-15T09:00:00Z',
    configSchema: {
      'apiKey': { label: 'API Key', type: 'password', placeholder: 'Enter your swap API key' },
      'slippage': { label: 'Default Slippage (%)', type: 'number', placeholder: '0.5' }
    }
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
    environment: 'mainnet',
    lastUpdated: '2024-02-28T11:30:00Z',
  },
  {
    id: 'int-3',
    name: 'Multi-Sig Pro',
    description: 'Manage multi-signature setups with ease and security.',
    developer: 'SafeGuard',
    version: '0.9.5',
    rating: 4.9,
    reviewsCount: 34,
    isCompatible: false,
    minAppVersion: '2.0.0',
    requiredPermissions: ['sign_transaction', 'manage_keys'],
    category: 'wallet',
    status: 'available',
    iconUrl: 'https://api.dicebear.com/7.x/identicon/svg?seed=multisig',
    dependencies: ['soroban-auth-lib'],
    lastUpdated: '2024-03-20T14:15:00Z',
  },
  {
    id: 'int-4',
    name: 'NFT Explorer',
    description: 'Browse and manage your digital collectibles on Stellar.',
    developer: 'MintMaster',
    version: '1.0.5',
    rating: 4.2,
    reviewsCount: 56,
    isCompatible: true,
    requiredPermissions: ['read_balances'],
    category: 'other',
    status: 'available',
    iconUrl: 'https://api.dicebear.com/7.x/identicon/svg?seed=nft',
    lastUpdated: '2024-03-10T16:45:00Z',
  }
];

const INITIAL_REVIEWS: Review[] = [
  {
    id: 'rev-1',
    integrationId: 'int-1',
    user: 'StellarExplorer',
    rating: 5,
    comment: 'Best swap widget I have used. Very smooth!',
    date: '2024-03-20T10:00:00Z'
  },
  {
    id: 'rev-2',
    integrationId: 'int-1',
    user: 'CryptoNerd',
    rating: 4,
    comment: 'Good, but could use more pairs.',
    date: '2024-03-22T14:30:00Z'
  }
];

const MOCK_ANALYTICS: MarketplaceAnalytics = {
  activeUsers: 1250,
  totalInstalls: 4820,
  topCategories: [
    { category: 'DeFi', count: 2100 },
    { category: 'Wallet', count: 1200 },
    { category: 'Analytics', count: 950 },
  ],
  recentReviews: INITIAL_REVIEWS,
  installTrend: [
    { date: '2024-03-20', count: 45 },
    { date: '2024-03-21', count: 52 },
    { date: '2024-03-22', count: 38 },
    { date: '2024-03-23', count: 65 },
    { date: '2024-03-24', count: 70 },
  ]
};

export const MarketplaceProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [availableIntegrations, setAvailableIntegrations] = useState<Integration[]>(MOCK_INTEGRATIONS);
  const [installedIntegrations, setInstalledIntegrations] = useState<Integration[]>([]);
  const [reviews, setReviews] = useState<Review[]>(INITIAL_REVIEWS);
  const [submissions, setSubmissions] = useState<DeveloperSubmission[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // App version for compatibility checks
  const CURRENT_APP_VERSION = '1.5.0';

  // Load installed from localStorage
  useEffect(() => {
    const saved = localStorage.getItem('fidelis_installed_plugins');
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        setInstalledIntegrations(parsed);
        // Sync available items status
        setAvailableIntegrations(prev => 
          prev.map(i => {
            const isInstalled = parsed.some((p: Integration) => p.id === i.id);
            return isInstalled ? { ...i, status: 'installed' } : i;
          })
        );
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
    setError(null);
    try {
      const integration = availableIntegrations.find(i => i.id === id);
      if (!integration) throw new Error("Integration not found");

      // Compatibility Check
      if (!integration.isCompatible || (integration.minAppVersion && integration.minAppVersion > CURRENT_APP_VERSION)) {
        throw new Error(`Incompatible: Requires App v${integration.minAppVersion || '2.0.0'}`);
      }

      // Simulate installation
      setAvailableIntegrations(prev => prev.map(i => i.id === id ? { ...i, status: 'installing' } : i));
      await new Promise(resolve => setTimeout(resolve, 1200));
      
      const newInstalledItem: Integration = { 
        ...integration, 
        status: 'installed' as const,
        configValues: config || {}
      };
      
      const newInstalled = [...installedIntegrations, newInstalledItem];
      savePlugins(newInstalled);
      
      setAvailableIntegrations(prev => 
        prev.map(i => i.id === id ? { ...i, status: 'installed', configValues: config || {} } : i)
      );
    } catch (err: any) {
      setError(err.message);
      setAvailableIntegrations(prev => prev.map(i => i.id === id ? { ...i, status: 'failed' } : i));
    } finally {
      setIsLoading(false);
    }
  };

  const updateIntegrationConfig = async (id: string, config: Record<string, string>) => {
    setIsLoading(true);
    try {
      await new Promise(resolve => setTimeout(resolve, 500));
      const newInstalled = installedIntegrations.map(i => 
        i.id === id ? { ...i, configValues: config } : i
      );
      savePlugins(newInstalled);
      setAvailableIntegrations(prev => 
        prev.map(i => i.id === id ? { ...i, configValues: config } : i)
      );
    } catch (err: any) {
      setError(err.message);
    } finally {
      setIsLoading(false);
    }
  };

  const checkForUpdates = async () => {
    setIsLoading(true);
    try {
      await new Promise(resolve => setTimeout(resolve, 1500));
      // Randomly mark some as having updates
      setAvailableIntegrations(prev => prev.map(i => {
        if (i.status === 'installed' && Math.random() > 0.5) {
          return { ...i, updateAvailable: true };
        }
        return i;
      }));
    } finally {
      setIsLoading(false);
    }
  };

  const uninstallIntegration = async (id: string) => {
    setIsLoading(true);
    try {
      await new Promise(resolve => setTimeout(resolve, 500));
      const newInstalled = installedIntegrations.filter(i => i.id !== id);
      savePlugins(newInstalled);
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
      await new Promise(resolve => setTimeout(resolve, 1500));
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

  const submitReview = async (reviewData: Omit<Review, 'id' | 'date'>) => {
    setIsLoading(true);
    try {
      await new Promise(resolve => setTimeout(resolve, 600));
      const newReview: Review = {
        ...reviewData,
        id: `rev-${Date.now()}`,
        date: new Date().toISOString()
      };
      setReviews(prev => [newReview, ...prev]);
      
      // Update integration rating (mock)
      setAvailableIntegrations(prev => prev.map(i => {
        if (i.id === reviewData.integrationId) {
          const newCount = i.reviewsCount + 1;
          const newRating = Number(((i.rating * i.reviewsCount + reviewData.rating) / newCount).toFixed(1));
          return { ...i, rating: newRating, reviewsCount: newCount };
        }
        return i;
      }));
    } finally {
      setIsLoading(false);
    }
  };

  const getReviewsForIntegration = (integrationId: string) => {
    return reviews.filter(r => r.integrationId === integrationId);
  };

  const analytics = useMemo(() => ({
    ...MOCK_ANALYTICS,
    recentReviews: reviews.slice(0, 5),
    totalInstalls: MOCK_ANALYTICS.totalInstalls + installedIntegrations.length
  }), [reviews, installedIntegrations]);

  return (
    <MarketplaceContext.Provider value={{
      availableIntegrations,
      installedIntegrations,
      submissions,
      analytics,
      isLoading,
      error,
      installIntegration,
      uninstallIntegration,
      updateIntegrationConfig,
      checkForUpdates,
      submitIntegration,
      submitReview,
      getReviewsForIntegration
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
