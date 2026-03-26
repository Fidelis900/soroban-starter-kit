export interface Integration {
  id: string;
  name: string;
  description: string;
  developer: string;
  version: string;
  rating: number;
  reviewsCount: number;
  isCompatible: boolean;
  minAppVersion?: string;
  requiredPermissions: string[];
  iconUrl?: string;
  category: 'wallet' | 'defi' | 'analytics' | 'tooling' | 'other';
  status: 'available' | 'installed' | 'update_available' | 'installing' | 'failed';
  configSchema?: Record<string, { label: string; type: 'text' | 'password' | 'number'; placeholder?: string }>;
  configValues?: Record<string, string>;
  environment?: 'testnet' | 'mainnet' | 'any';
  dependencies?: string[];
  lastUpdated?: string;
  updateAvailable?: boolean;
}

export interface Review {
  id: string;
  integrationId: string;
  user: string;
  rating: number; // 1-5 stars
  comment: string;
  date: string;
}

export interface DeveloperSubmission {
  id: string;
  name: string;
  description: string;
  version: string;
  category: Integration['category'];
  status: 'pending' | 'approved' | 'rejected';
  submittedAt: string;
  compatibilityDetails?: string;
  configSchema?: string;
}

export interface MarketplaceAnalytics {
  activeUsers: number;
  totalInstalls: number;
  topCategories: { category: string; count: number }[];
  recentReviews: Review[];
  installTrend: { date: string; count: number }[]; // For simple charts
}

export interface MarketplaceState {
  availableIntegrations: Integration[];
  installedIntegrations: Integration[];
  submissions: DeveloperSubmission[];
  analytics: MarketplaceAnalytics | null;
  isLoading: boolean;
  error: string | null;
}
