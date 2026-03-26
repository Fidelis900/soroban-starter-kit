import React from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';

export const MarketplaceAnalytics: React.FC = () => {
  const { analytics } = useMarketplace();

  if (!analytics) return <div className="p-xl text-muted">No analytics data available yet.</div>;

  return (
    <div className="marketplace-analytics">
      <div className="grid grid-3 mb-xl">
        <div className="card glass-effect text-center p-lg">
          <span className="text-muted text-sm block mb-xs">Active Users</span>
          <h2 className="m-0 text-highlight">{analytics.activeUsers.toLocaleString()}</h2>
        </div>
        <div className="card glass-effect text-center p-lg">
          <span className="text-muted text-sm block mb-xs">Total Installs</span>
          <h2 className="m-0 text-highlight">{analytics.totalInstalls.toLocaleString()}</h2>
        </div>
        <div className="card glass-effect text-center p-lg">
          <span className="text-muted text-sm block mb-xs">Avg. Rating</span>
          <h2 className="m-0 text-highlight">4.8 / 5.0</h2>
        </div>
      </div>

      <div className="grid grid-2">
        <div className="card glass-effect">
          <div className="card-header">
            <h3 className="card-title">Popular Categories</h3>
          </div>
          <div className="p-lg">
            {analytics.topCategories.map((cat, idx) => (
              <div key={cat.category} className="mb-md">
                <div className="flex justify-between text-sm mb-xs">
                  <span>{cat.category}</span>
                  <span className="text-muted">{cat.count} installs</span>
                </div>
                <div className="progress-bar" style={{ height: '8px', background: 'rgba(255,255,255,0.05)', borderRadius: '4px', overflow: 'hidden' }}>
                  <div className="progress-fill" style={{ 
                    height: '100%', 
                    width: `${(cat.count / 2100) * 100}%`, 
                    background: 'var(--color-highlight)',
                    boxShadow: '0 0 10px rgba(233,69,96,0.5)'
                  }} />
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="card glass-effect flex flex-col">
          <div className="card-header p-md border-b">
            <h3 className="card-title m-0">Recent Feedback</h3>
          </div>
          <div className="p-lg flex flex-col gap-md flex-1 overflow-y-auto" style={{ maxHeight: '400px' }}>
            {analytics.recentReviews.map(review => (
              <div key={review.id} className="text-sm border-b pb-sm last:border-0">
                <div className="flex justify-between mb-xs">
                  <span className="font-bold">{review.user}</span>
                  <span className="text-warning">{'★'.repeat(review.rating)}</span>
                </div>
                <p className="m-0 text-secondary italic mb-xs">"{review.comment}"</p>
                <span className="text-muted" style={{ fontSize: '0.7rem' }}>{new Date(review.date).toLocaleDateString()}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="card glass-effect mt-xl p-xl">
        <h3 className="mb-lg">Installation Trends (Last 7 Days)</h3>
        <div className="flex items-end gap-md" style={{ height: '150px' }}>
          {analytics.installTrend.map(day => (
            <div key={day.date} className="flex-1 flex flex-col items-center gap-sm">
              <div className="trend-bar" style={{ 
                width: '100%', 
                height: `${(day.count / 75) * 100}%`, 
                background: 'linear-gradient(to top, var(--color-highlight), transparent)',
                borderRadius: '4px 4px 0 0'
              }} />
              <span className="text-muted" style={{ fontSize: '0.65rem' }}>{day.date.split('-').slice(1).join('/')}</span>
            </div>
          ))}
        </div>
      </div>

      <style>{`
        .grid-3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1.5rem; }
        .block { display: block; }
        .m-0 { margin: 0; }
        .text-highlight { color: var(--color-highlight); }
        .text-warning { color: var(--color-warning); }
        .text-success { color: var(--color-success); }
        .last\\:border-0:last-child { border-bottom: none; }
        @media (max-width: 768px) {
          .grid-3 { grid-template-columns: 1fr; }
          .grid-2 { grid-template-columns: 1fr; }
        }
      `}</style>
    </div>
  );
};
