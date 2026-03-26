import React, { useState, useEffect } from 'react';
import { Integration } from './types';
import { useMarketplace } from '../../context/MarketplaceContext';
import { ReviewSection } from './ReviewSection';

interface Props {
  item: Integration;
  onClose: () => void;
}

export const IntegrationModal: React.FC<Props> = ({ item, onClose }) => {
  const { installIntegration, uninstallIntegration, updateIntegrationConfig, isLoading } = useMarketplace();
  const [config, setConfig] = useState<Record<string, string>>(item.configValues || {});
  const [showConfig, setShowConfig] = useState(item.status === 'installed');

  useEffect(() => {
    if (item.configValues) {
      setConfig(item.configValues);
    }
  }, [item.configValues]);

  const handleInstall = () => {
    installIntegration(item.id, config);
  };

  const handleUpdateConfig = () => {
    updateIntegrationConfig(item.id, config);
  };

  const handleConfigChange = (key: string, value: string) => {
    setConfig(prev => ({ ...prev, [key]: value }));
  };

  return (
    <div className="modal-overlay glass-effect" onClick={onClose}>
      <div className="modal-content glass-effect" onClick={e => e.stopPropagation()}>
        <button className="modal-close" onClick={onClose}>&times;</button>
        
        <div className="modal-header">
          <img src={item.iconUrl || 'https://api.dicebear.com/7.x/identicon/svg?seed=' + item.name} alt={item.name} className="modal-icon" />
          <div className="modal-title-group">
            <h2 className="modal-name">{item.name}</h2>
            <span className="modal-developer">Developed by {item.developer} • v{item.version}</span>
            <div className="flex gap-sm mt-xs">
              <span className="category-tag">{item.category}</span>
              {item.isCompatible ? (
                <span className="badge compact success" style={{ position: 'relative', top: 0, right: 0 }}>✅ Compatible</span>
              ) : (
                <span className="badge compact warning" style={{ position: 'relative', top: 0, right: 0 }}>⚠️ Incompatible</span>
              )}
            </div>
          </div>
        </div>

        <div className="modal-body">
          <div className="grid grid-2">
            <div className="modal-info">
              <h3>About</h3>
              <p className="modal-desc">{item.description}</p>
              
              <div className="technical-details mt-lg">
                <h4>Technical Details</h4>
                <ul className="text-secondary text-sm">
                  <li><strong>Environment:</strong> {item.environment || 'Any'}</li>
                  <li><strong>Required Permissions:</strong> {item.requiredPermissions.join(', ')}</li>
                  {item.minAppVersion && <li><strong>Min App Version:</strong> v{item.minAppVersion}</li>}
                  {item.dependencies && <li><strong>Dependencies:</strong> {item.dependencies.join(', ')}</li>}
                </ul>
              </div>

              {item.configSchema && (
                <div className="config-section mt-lg border-t pt-md">
                  <h3>{item.status === 'installed' ? 'Configuration' : 'Initial Setup'}</h3>
                  <p className="text-sm text-muted mb-md">
                    {item.status === 'installed' 
                      ? 'Manage settings for this integration.' 
                      : 'Configure the integration before installing.'}
                  </p>
                  
                  {Object.entries(item.configSchema).map(([key, schema]) => (
                    <div className="form-group mb-md" key={key}>
                      <label className="form-label text-sm">{schema.label}</label>
                      <input 
                        type={schema.type} 
                        placeholder={schema.placeholder}
                        value={config[key] || ''} 
                        onChange={(e) => handleConfigChange(key, e.target.value)}
                        className="form-input" 
                      />
                    </div>
                  ))}

                  {item.status === 'installed' && (
                    <button 
                      className="btn btn-secondary btn-sm"
                      onClick={handleUpdateConfig}
                      disabled={isLoading}
                    >
                      {isLoading ? 'Updating...' : 'Save Settings'}
                    </button>
                  )}
                </div>
              )}
            </div>

            <div className="modal-reviews">
              <ReviewSection integrationId={item.id} />
            </div>
          </div>
        </div>

        <div className="modal-footer">
          {item.status === 'installed' ? (
            <div className="flex justify-between w-full align-center">
               <button 
                className="btn btn-ghost danger"
                onClick={() => uninstallIntegration(item.id)}
                disabled={isLoading}
              >
                Uninstall Integration
              </button>
              
              <div className="flex gap-md">
                {item.updateAvailable && (
                  <button 
                    className="btn btn-secondary" 
                    onClick={() => installIntegration(item.id)}
                    disabled={isLoading}
                  >
                    {isLoading ? 'Updating...' : 'Update Available'}
                  </button>
                )}
                <button className="btn btn-primary" onClick={onClose}>Done</button>
              </div>
            </div>
          ) : (
            <button 
              className="btn btn-primary w-full shadow-effect"
              onClick={handleInstall}
              disabled={!item.isCompatible || isLoading}
            >
              {isLoading ? 'Installing...' : item.isCompatible ? 'Install Integration' : 'Incompatible with Current Version'}
            </button>
          )}
        </div>
      </div>
      
      <style>{`
        .modal-overlay {
          position: fixed;
          top: 0;
          left: 0;
          right: 0;
          bottom: 0;
          background: rgba(0,0,0,0.8);
          display: flex;
          align-items: center;
          justify-content: center;
          z-index: 1000;
          animation: fadeIn 0.3s;
        }
        .modal-content {
          width: 90%;
          max-width: 900px;
          max-height: 90vh;
          background: var(--color-bg-secondary);
          border-radius: var(--radius-lg);
          border: 1px solid var(--color-border);
          position: relative;
          display: flex;
          flex-direction: column;
          overflow: hidden;
          box-shadow: 0 20px 50px rgba(0,0,0,0.5);
        }
        .modal-close {
          position: absolute;
          top: 1rem;
          right: 1.5rem;
          background: none;
          border: none;
          color: white;
          font-size: 2rem;
          cursor: pointer;
          z-index: 10;
        }
        .modal-header {
          padding: 2.5rem;
          background: linear-gradient(135deg, rgba(26,26,46,0.6) 0%, rgba(15,52,96,0.4) 100%);
          display: flex;
          gap: 1.5rem;
          align-items: center;
          border-bottom: 1px solid var(--color-border);
        }
        .modal-icon {
          width: 80px;
          height: 80px;
          border-radius: var(--radius-md);
          background: white;
          padding: 4px;
        }
        .modal-body {
          padding: 2.5rem;
          overflow-y: auto;
          flex: 1;
        }
        .modal-footer {
          padding: 1.5rem 2.5rem;
          border-top: 1px solid var(--color-border);
          background: var(--color-bg-tertiary);
        }
        .grid-2 {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 3rem;
        }
        @media (max-width: 768px) {
          .grid-2 { grid-template-columns: 1fr; gap: 1.5rem; }
        }
        .modal-desc {
          line-height: 1.6;
          color: var(--color-text-secondary);
        }
        .text-sm { font-size: 0.875rem; }
        .border-t { border-top: 1px solid var(--color-border); }
        .success { background: var(--color-success) !important; color: white !important; }
      `}</style>
    </div>
  );
};
