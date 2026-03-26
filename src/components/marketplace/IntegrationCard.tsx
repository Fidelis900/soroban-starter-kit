import React from 'react';
import { Integration } from './types';
import { useMarketplace } from '../../context/MarketplaceContext';

interface Props {
  item: Integration;
  onViewDetails: (item: Integration) => void;
}

export const IntegrationCard: React.FC<Props> = ({ item, onViewDetails }) => {
  const { installIntegration, uninstallIntegration, isLoading } = useMarketplace();

  return (
    <div className="integration-card glass-effect" onClick={() => onViewDetails(item)}>
      {/* Compatibility Badge */}
      {!item.isCompatible && (
        <div className="badge warning compact">Requires App v{item.minAppVersion || '2.0.0'}</div>
      )}
      
      {item.status === 'installing' && (
        <div className="badge compact" style={{ background: 'var(--color-highlight)', color: 'white' }}>
          Installing...
        </div>
      )}

      {item.status === 'failed' && (
        <div className="badge compact" style={{ background: 'var(--color-error)', color: 'white' }}>
          Installation Failed
        </div>
      )}
      
      <div className="integration-card-header">
        <img src={item.iconUrl || 'https://api.dicebear.com/7.x/identicon/svg?seed=' + item.name} alt={item.name} className="integration-icon" />
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
      
      <div className="integration-actions" onClick={e => e.stopPropagation()}>
        {item.status === 'available' || item.status === 'failed' ? (
          <button 
            disabled={!item.isCompatible || isLoading} 
            onClick={() => installIntegration(item.id)}
            className="btn btn-primary w-full shadow-effect"
          >
            {item.isCompatible ? 'Install' : 'Incompatible'}
          </button>
        ) : item.status === 'installed' ? (
          <div style={{ display: 'flex', gap: '8px', width: '100%' }}>
            <button 
              className="btn btn-secondary flex-1"
              onClick={() => onViewDetails(item)}
            >
              Configure
            </button>
            <button 
              className="btn btn-ghost danger"
              onClick={() => uninstallIntegration(item.id)}
              disabled={isLoading}
            >
              Remove
            </button>
          </div>
        ) : (
          <button disabled className="btn btn-secondary w-full">
            Processing...
          </button>
        )}
      </div>
    </div>
  );
};
