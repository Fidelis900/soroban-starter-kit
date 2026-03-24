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
  status: 'available' | 'installed' | 'update_available';
}

export interface Review {
  id: string;
  integrationId: string;
  user: string;
  rating: number;
  comment: string;
  date: string;
}

export interface DeveloperSubmission {
  id: string;
  name: string;
  description: string;
  status: 'pending' | 'approved' | 'rejected';
  submittedAt: string;
}

export interface MarketplaceAnalytics {
  activeUsers: number;
  totalInstalls: number;
  topCategories: { category: string; count: number }[];
  recentReviews: Review[];
}

export interface MarketplaceState {
  availableIntegrations: Integration[];
  installedIntegrations: Integration[];
  submissions: DeveloperSubmission[];
  analytics: MarketplaceAnalytics | null;
  isLoading: boolean;
  error: string | null;
}
