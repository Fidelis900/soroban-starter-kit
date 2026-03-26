import React, { useState } from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';
import { DeveloperSubmission } from './types';

export const DeveloperHub: React.FC = () => {
  const { submissions, submitIntegration, isLoading } = useMarketplace();
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [formData, setFormData] = useState<Omit<DeveloperSubmission, 'id' | 'status' | 'submittedAt'>>({
    name: '',
    description: '',
    version: '1.0.0',
    category: 'tooling',
    compatibilityDetails: '',
    configSchema: '',
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name || !formData.description) return;
    
    await submitIntegration(formData);
    setFormData({ 
      name: '', 
      description: '', 
      version: '1.0.0', 
      category: 'tooling',
      compatibilityDetails: '',
      configSchema: '',
    });
    setIsFormOpen(false);
  };

  return (
    <div className="developer-hub">
      <div className="flex justify-between items-center mb-lg">
        <div>
          <h3>Your Submissions</h3>
          <p className="text-muted">Manage your published integrations and check analytics.</p>
        </div>
        <button 
          className="btn btn-primary shadow-effect"
          onClick={() => setIsFormOpen(!isFormOpen)}
        >
          {isFormOpen ? 'Cancel' : 'Submit New Integration'}
        </button>
      </div>

      {isFormOpen && (
        <form onSubmit={handleSubmit} className="card glass-effect mb-lg" style={{ animation: 'fadeIn 0.3s' }}>
          <h4 className="mb-md">Submit to Marketplace</h4>
          <div className="form-group">
            <label className="form-label">Integration Name</label>
            <input 
              type="text" 
              className="form-input"
              value={formData.name}
              onChange={e => setFormData({...formData, name: e.target.value})}
              placeholder="e.g. DeFi Yield Tracker"
              required
            />
          </div>
          <div className="form-group mb-md">
            <label className="form-label">Description</label>
            <textarea 
              className="form-input" 
              rows={3}
              value={formData.description}
              onChange={e => setFormData({...formData, description: e.target.value})}
              placeholder="Describe what your integration does..."
              required
            />
          </div>

          <div className="grid grid-2 gap-md mb-md">
            <div className="form-group">
              <label className="form-label">Version</label>
              <input 
                type="text" 
                className="form-input"
                value={formData.version}
                onChange={e => setFormData({...formData, version: e.target.value})}
                placeholder="1.0.0"
                required
              />
            </div>
            <div className="form-group">
              <label className="form-label">Category</label>
              <select 
                className="form-input"
                value={formData.category}
                onChange={e => setFormData({...formData, category: e.target.value as any})}
              >
                <option value="wallet">Wallet</option>
                <option value="defi">DeFi</option>
                <option value="analytics">Analytics</option>
                <option value="tooling">Tooling</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>

          <div className="form-group mb-md">
            <label className="form-label">Compatibility Details</label>
            <input 
              type="text" 
              className="form-input"
              value={formData.compatibilityDetails}
              onChange={e => setFormData({...formData, compatibilityDetails: e.target.value})}
              placeholder="e.g. Compatible with Fidelis v1.5.0+"
            />
          </div>

          <div className="form-group mb-lg">
            <label className="form-label">Config Schema (JSON)</label>
            <textarea 
              className="form-input"
              rows={2}
              value={formData.configSchema}
              onChange={e => setFormData({...formData, configSchema: e.target.value})}
              placeholder='{ "apiKey": { "label": "API Key", "type": "password" } }'
            />
          </div>

          <button type="submit" disabled={isLoading} className="btn btn-primary w-full">
            {isLoading ? 'Submitting...' : 'Submit Integration for Review'}
          </button>
        </form>
      )}

      {submissions.length === 0 ? (
        <div className="empty-state">
          <span className="empty-icon">📈</span>
          <p className="empty-title">No submissions yet</p>
          <p className="empty-text">Publish your first app to see analytics.</p>
        </div>
      ) : (
        <div className="developer-dashboard">
          <div className="grid grid-2 gap-lg mb-xl">
            <div className="card glass-effect p-lg">
              <h4 className="m-0 mb-md">Overview</h4>
              <div className="flex flex-col gap-sm">
                <div className="flex justify-between">
                  <span className="text-muted">Total Submissions</span>
                  <span className="font-bold">{submissions.length}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted">Approved Apps</span>
                  <span className="text-success font-bold">{submissions.filter(s => s.status === 'approved').length}</span>
                </div>
                <div className="flex justify-between">
                   <span className="text-muted">Total Installs (Est.)</span>
                   <span className="font-bold">142</span>
                </div>
              </div>
            </div>
            <div className="card glass-effect p-lg">
              <h4 className="m-0 mb-md">Quick Stats</h4>
              <div className="flex flex-col gap-sm">
                 <div className="flex justify-between">
                   <span className="text-muted">Avg. Rating</span>
                   <span className="text-warning font-bold">4.8 ★</span>
                 </div>
                 <div className="flex justify-between">
                   <span className="text-muted">Revenue Share</span>
                   <span className="font-bold">$12.50</span>
                 </div>
              </div>
            </div>
          </div>

          <h4 className="mb-md">Recent Submissions</h4>
          <div className="submissions-list flex flex-col gap-md">
            {submissions.map(sub => (
              <div key={sub.id} className="card glass-effect flex justify-between items-center p-md">
                <div className="flex gap-md items-center">
                  <div className="sub-icon" style={{ width: '40px', height: '40px', background: 'rgba(255,255,255,0.05)', borderRadius: '8px', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '1.2rem' }}>
                    📦
                  </div>
                  <div>
                    <h4 style={{ margin: 0 }}>{sub.name} <span className="text-muted text-sm font-normal">v{sub.version}</span></h4>
                    <span className="text-muted" style={{ fontSize: '0.85rem' }}>
                      {sub.category.toUpperCase()} • {new Date(sub.submittedAt).toLocaleDateString()}
                    </span>
                  </div>
                </div>
                <div>
                  <span className={`badge compact ${sub.status === 'pending' ? 'warning' : sub.status === 'approved' ? 'success' : 'danger'}`} style={{ position: 'relative', top: 0, right: 0 }}>
                    {sub.status.toUpperCase()}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
