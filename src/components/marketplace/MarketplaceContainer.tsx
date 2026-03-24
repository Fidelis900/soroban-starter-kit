import React, { useState } from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';
import { IntegrationDiscovery } from './IntegrationDiscovery';
import { DeveloperHub } from './DeveloperHub';

export const MarketplaceContainer: React.FC = () => {
  const [activeSubTab, setActiveSubTab] = useState<'discover' | 'installed' | 'developer'>('discover');
  
  return (
    <div className="marketplace-container">
      {/* Premium Header */}
      <div className="marketplace-header">
        <h2 className="marketplace-title">Marketplace</h2>
        <p className="marketplace-subtitle">Discover powerful plugins and integrations to extend Fidelis.</p>
      </div>
      
      {/* Sub Navigation */}
      <div className="marketplace-tabs">
        <button 
          className={`marketplace-tab ${activeSubTab === 'discover' ? 'active' : ''}`}
          onClick={() => setActiveSubTab('discover')}
        >
          <span className="icon">🌍</span> Discovery
        </button>
        <button 
          className={`marketplace-tab ${activeSubTab === 'installed' ? 'active' : ''}`}
          onClick={() => setActiveSubTab('installed')}
        >
          <span className="icon">📦</span> Installed
        </button>
        <button 
          className={`marketplace-tab ${activeSubTab === 'developer' ? 'active' : ''}`}
          onClick={() => setActiveSubTab('developer')}
        >
          <span className="icon">👩‍💻</span> Developer Hub
        </button>
      </div>

      <div className="marketplace-content">
        {activeSubTab === 'discover' && <IntegrationDiscovery mode="discover" />}
        {activeSubTab === 'installed' && <IntegrationDiscovery mode="installed" />}
        {activeSubTab === 'developer' && <DeveloperHub />}
      </div>
    </div>
  );
};
