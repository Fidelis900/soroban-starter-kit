import React, { useState } from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';
import { Integration } from './types';
import { IntegrationCard } from './IntegrationCard';
import { IntegrationModal } from './IntegrationModal';

interface Props {
  mode: 'discover' | 'installed';
}

const CATEGORIES: Integration['category'][] = ['wallet', 'defi', 'analytics', 'tooling', 'other'];

export const IntegrationDiscovery: React.FC<Props> = ({ mode }) => {
  const { availableIntegrations, installedIntegrations } = useMarketplace();
  const [search, setSearch] = useState('');
  const [activeCategory, setActiveCategory] = useState<Integration['category'] | 'all'>('all');
  const [selectedIntegration, setSelectedIntegration] = useState<Integration | null>(null);
  
  const allItems = mode === 'discover' ? availableIntegrations : installedIntegrations;
  
  const visibleItems = allItems.filter(item => {
    const matchesSearch = item.name.toLowerCase().includes(search.toLowerCase()) || 
                         item.description.toLowerCase().includes(search.toLowerCase());
    const matchesCategory = activeCategory === 'all' || item.category === activeCategory;
    return matchesSearch && matchesCategory;
  });

  return (
    <div className="integration-discovery">
      {/* Search Header */}
      <div className="discovery-header">
        <input 
          type="text" 
          placeholder="Search extensions, tools, or services..." 
          className="search-input"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="filter-chips">
          <button 
            className={`chip ${activeCategory === 'all' ? 'active' : ''}`}
            onClick={() => setActiveCategory('all')}
          >
            All
          </button>
          {CATEGORIES.map(cat => (
            <button 
              key={cat}
              className={`chip ${activeCategory === cat ? 'active' : ''}`}
              onClick={() => setActiveCategory(cat)}
            >
              {cat.charAt(0).toUpperCase() + cat.slice(1)}
            </button>
          ))}
        </div>
      </div>

      {visibleItems.length === 0 ? (
        <div className="empty-state">
          <span className="empty-icon">🔍</span>
          <p className="empty-title">No integrations match your criteria</p>
          <p className="empty-text">Try different keywords or check another category.</p>
        </div>
      ) : (
        <div className="integration-grid">
          {visibleItems.map(item => (
            <IntegrationCard 
              key={item.id} 
              item={item} 
              onViewDetails={(i) => setSelectedIntegration(i)} 
            />
          ))}
        </div>
      )}

      {/* Integration Modal */}
      {selectedIntegration && (
        <IntegrationModal 
          item={selectedIntegration} 
          onClose={() => setSelectedIntegration(null)} 
        />
      )}
    </div>
  );
};
