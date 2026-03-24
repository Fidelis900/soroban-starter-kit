import React, { useState } from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';
import { Integration } from './types';

interface Props {
  mode: 'discover' | 'installed';
}

export const IntegrationDiscovery: React.FC<Props> = ({ mode }) => {
  const { availableIntegrations, installedIntegrations, installIntegration, uninstallIntegration, isLoading } = useMarketplace();
  const [search, setSearch] = useState('');
  
  const allItems = mode === 'discover' ? availableIntegrations : installedIntegrations;
  
  const visibleItems = allItems.filter(item => 
    item.name.toLowerCase().includes(search.toLowerCase()) || 
    item.category.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="integration-discovery">
      {/* Search Header */}
      <div className="discovery-header">
        <input 
          type="text" 
          placeholder="Search by name or category..." 
          className="search-input"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="filter-chips">
          <span className="chip active">All</span>
          <span className="chip">DeFi</span>
          <span className="chip">Analytics</span>
          <span className="chip">Wallet</span>
        </div>
      </div>

      {visibleItems.length === 0 ? (
        <div className="empty-state">
          <span className="empty-icon">📂</span>
          <p className="empty-title">No integrations found</p>
          <p className="empty-text">Try tweaking your search terms.</p>
        </div>
      ) : (
        <div className="integration-grid">
          {visibleItems.map(item => (
            <div key={item.id} className="integration-card glass-effect">
              {/* Compatibility Badge */}
              {!item.isCompatible && (
                <div className="badge warning compact">Requires App v{item.minAppVersion}</div>
              )}
              
              <div className="integration-card-header">
                <img src={item.iconUrl} alt={item.name} className="integration-icon" />
                <div className="integration-title-group">
                  <h3 className="integration-name">{item.name}</h3>
                  <span className="integration-developer">by {item.developer}</span>
                </div>
              </div>
              
              <p className="integration-desc">{item.description}</p>
              
              <div className="integration-meta">
                <span className="rating">⭐ {item.rating} ({item.reviewsCount})</span>
                <span className="category-tag">{item.category}</span>
              </div>
              
              <div className="integration-actions">
                {mode === 'discover' && item.status !== 'installed' ? (
                  <button 
                    disabled={!item.isCompatible || isLoading} 
                    onClick={() => installIntegration(item.id)}
                    className="btn btn-primary w-full shadow-effect"
                  >
                    {item.isCompatible ? 'Install' : 'Incompatible'}
                  </button>
                ) : (
                  <div style={{ display: 'flex', gap: '8px', width: '100%' }}>
                    <button 
                      className="btn btn-secondary flex-1"
                      onClick={() => alert(`Configuring ${item.name}...`)}
                    >
                      Configure
                    </button>
                    {mode === 'installed' && (
                      <button 
                        className="btn btn-ghost danger"
                        onClick={() => uninstallIntegration(item.id)}
                        disabled={isLoading}
                      >
                        Remove
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
