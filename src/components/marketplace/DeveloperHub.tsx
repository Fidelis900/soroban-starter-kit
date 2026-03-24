import React, { useState } from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';

export const DeveloperHub: React.FC = () => {
  const { submissions, submitIntegration, isLoading } = useMarketplace();
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [formData, setFormData] = useState({
    name: '',
    description: '',
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name || !formData.description) return;
    
    await submitIntegration(formData);
    setFormData({ name: '', description: '' });
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
          <div className="form-group">
            <label className="form-label">Description</label>
            <textarea 
              className="form-input"
              rows={4}
              value={formData.description}
              onChange={e => setFormData({...formData, description: e.target.value})}
              placeholder="Describe what your integration does..."
              required
            />
          </div>
          <button type="submit" disabled={isLoading} className="btn btn-primary">
            {isLoading ? 'Submitting...' : 'Submit for Review'}
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
        <div className="submissions-list flex flex-col gap-md">
          {submissions.map(sub => (
            <div key={sub.id} className="card glass-effect flex justify-between items-center">
              <div>
                <h4 style={{ margin: 0 }}>{sub.name}</h4>
                <span className="text-muted" style={{ fontSize: '0.85rem' }}>Submitted: {new Date(sub.submittedAt).toLocaleDateString()}</span>
              </div>
              <div>
                <span className={`badge compact ${sub.status === 'pending' ? 'warning' : 'success'}`} style={{ position: 'relative', top: 0, right: 0 }}>
                  {sub.status.toUpperCase()}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
