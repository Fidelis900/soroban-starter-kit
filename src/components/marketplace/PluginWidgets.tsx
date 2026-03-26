import React from 'react';
import { useMarketplace } from '../../context/MarketplaceContext';

export const DeFiSwapWidget: React.FC = () => {
  return (
    <div className="card glass-effect p-md mb-md" style={{ borderLeft: '4px solid var(--color-highlight)' }}>
      <div className="flex justify-between items-center mb-sm">
        <h4 className="m-0">💱 Quick Swap</h4>
        <span className="text-muted text-xs">DeFi Swap Widget</span>
      </div>
      <div className="flex gap-sm mb-sm">
        <input type="number" className="form-input flex-1 p-xs text-sm" placeholder="From XLM" />
        <button className="btn btn-secondary btn-sm">⇆</button>
        <input type="number" className="form-input flex-1 p-xs text-sm" placeholder="To USDC" />
      </div>
      <button className="btn btn-primary btn-sm w-full">Swap Now</button>
    </div>
  );
};

export const PortfolioChartWidget: React.FC = () => {
  return (
    <div className="card glass-effect p-md mb-md" style={{ borderLeft: '4px solid #4ecca3' }}>
       <div className="flex justify-between items-center mb-sm">
        <h4 className="m-0">📈 Portfolio Growth</h4>
        <span className="text-muted text-xs">Analytics Pro</span>
      </div>
      <div className="flex items-end gap-xs" style={{ height: '60px' }}>
        {[30, 45, 35, 60, 55, 75, 85].map((h, i) => (
          <div key={i} className="flex-1" style={{ height: `${h}%`, background: 'var(--color-success)', opacity: 0.6, borderRadius: '2px' }} />
        ))}
      </div>
      <div className="flex justify-between mt-sm text-xs text-muted">
        <span>Mar 20</span>
        <span>Today</span>
      </div>
    </div>
  );
};

export const NFTShowcaseWidget: React.FC = () => {
  return (
    <div className="card glass-effect p-md mb-md" style={{ borderLeft: '4px solid #bb86fc' }}>
       <div className="flex justify-between items-center mb-sm">
        <h4 className="m-0">🖼️ Your Hot NFTs</h4>
        <span className="text-muted text-xs">NFT Explorer</span>
      </div>
      <div className="flex gap-sm">
        <div style={{ width: '40px', height: '40px', background: 'linear-gradient(45deg, #f09, #d0e)', borderRadius: '4px' }} />
        <div style={{ width: '40px', height: '40px', background: 'linear-gradient(45deg, #09f, #0ef)', borderRadius: '4px' }} />
        <div style={{ width: '40px', height: '40px', background: 'linear-gradient(45deg, #0f9, #0fe)', borderRadius: '4px' }} />
        <div className="flex items-center justify-center text-xs text-muted" style={{ width: '40px', height: '40px', background: 'rgba(255,255,255,0.05)', borderRadius: '4px' }}>
          +12
        </div>
      </div>
    </div>
  );
};

export const ActiveWidgets: React.FC = () => {
  const { installedIntegrations } = useMarketplace();
  
  if (installedIntegrations.length === 0) return null;

  return (
    <div className="active-widgets-area mb-lg">
      <h3 className="text-sm font-bold text-muted mb-md uppercase tracking-wider">Installed Extensions</h3>
      <div className="grid grid-3 gap-md">
        {installedIntegrations.some(i => i.id === 'int-1') && <DeFiSwapWidget />}
        {installedIntegrations.some(i => i.id === 'int-2') && <PortfolioChartWidget />}
        {installedIntegrations.some(i => i.id === 'int-4') && <NFTShowcaseWidget />}
      </div>
    </div>
  );
};
