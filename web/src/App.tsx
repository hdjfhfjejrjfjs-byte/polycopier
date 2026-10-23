import { useEffect, useState } from 'react';
import SettingsManager from './SettingsManager';
import SetupWizard from './SetupWizard';
import { apiFetch, ApiError, hasToken } from './api';
import type { Dec, SetupRequired, StateResponse } from './api';

const num = (d: Dec): number => Number(d);

function App() {
  const [state, setState] = useState<StateResponse | null>(null);
  const [activeTab, setActiveTab] = useState<'dashboard' | 'settings'>('dashboard');
  const [setupRequired, setSetupRequired] = useState(false);
  const [authError, setAuthError] = useState(false);

  useEffect(() => {
    const fetchState = async () => {
      try {
        const data = await apiFetch<StateResponse | SetupRequired>('/api/state');
        setAuthError(false);
        if ('status' in data && data.status === 'setup_required') {
          setSetupRequired(true);
        } else {
          setSetupRequired(false);
          setState(data as StateResponse);
        }
      } catch (err) {
        if (err instanceof ApiError && (err.status === 401 || err.status === 403)) {
          setAuthError(true);
        }
        console.error('Failed to fetch state', err);
      }
    };
    
    fetchState();
    const interval = setInterval(fetchState, 1000);
    return () => clearInterval(interval);
  }, []);

  if (authError || !hasToken()) {
    return (
      <div className="loading-container">
        <div style={{ color: 'var(--text-secondary)', maxWidth: '480px', textAlign: 'center' }}>
          Access token missing or invalid. Open the dashboard using the link printed by
          Polycopier at startup (it contains <code>?token=...</code>).
        </div>
      </div>
    );
  }

  if (setupRequired) {
    return <SetupWizard />;
  }

  if (!state) {
    return (
      <div className="loading-container">
        <div className="spinner"></div>
        <div style={{ color: 'var(--text-secondary)' }}>Connecting to Polycopier Daemon...</div>
      </div>
    );
  }

  const targets = [...(state.target_positions || [])].sort((a, b) => {
    const statusScore = (s: string) => s.includes('Monitoring') ? 0 : s.includes('Entered') ? 1 : 2;
    return statusScore(a.status) - statusScore(b.status);
  });

  const getStatusLabel = (status: unknown): string => {
      if (typeof status === 'string') { return status; }
      if (status && typeof status === 'object') { return Object.keys(status)[0]; }
      return 'UNKNOWN';
  };

  const CopyWalletButton = ({ wallet }: { wallet: string }) => {
    const [copied, setCopied] = useState(false);
    return (
      <button 
        onClick={() => {
          navigator.clipboard.writeText(wallet);
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        }}
        title="Copy full address"
        style={{ 
          background: 'none', 
          border: 'none', 
          cursor: 'pointer', 
          padding: '2px', 
          display: 'flex', 
          alignItems: 'center', 
          color: copied ? 'var(--accent-positive)' : 'var(--text-secondary)', 
          opacity: copied ? 1 : 0.7,
          transition: 'all 0.2s ease'
        }}
        onMouseEnter={(e) => !copied && (e.currentTarget.style.opacity = '1')}
        onMouseLeave={(e) => !copied && (e.currentTarget.style.opacity = '0.7')}
      >
        {copied ? (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="20 6 9 17 4 12"></polyline>
          </svg>
        ) : (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
          </svg>
        )}
      </button>
    );
  };

  return (
    <div className="dashboard-container">
      <header>
        <div className="header-title" style={{ display: 'flex', justifyContent: 'space-between', width: '100%' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
            <h1>Polycopier</h1>
            <div className="live-badge">{state.is_sim ? 'SIM' : 'LIVE'}</div>
          </div>
          <div className="nav-tabs">
            <button className={`nav-tab ${activeTab === 'dashboard' ? 'active' : ''}`} onClick={() => setActiveTab('dashboard')}>Dashboard</button>
            <button className={`nav-tab ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')}>Settings & Env</button>
          </div>
        </div>
        
        <div className="stats-grid">
          <div className="stat-card">
            <span className="stat-label">Total Balance</span>
            <span className="stat-value">${num(state.total_balance).toFixed(2)}</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Unrealized PnL</span>
            <span className={`stat-value ${num(state.unrealized_pnl) < 0 ? 'val-negative' : num(state.unrealized_pnl) > 0 ? 'val-positive' : 'val-neutral'}`}>
              ${num(state.unrealized_pnl).toFixed(2)}
            </span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Copies Executed</span>
            <span className="stat-value">{state.copies_executed}</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Trades Skipped</span>
            <span className="stat-value">{state.trades_skipped}</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Next Scan</span>
            <span className="stat-value">{state.next_scan_secs}s</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Loss Guard</span>
            <span className={`stat-value ${state.risk.cooldown_remaining_secs ? 'val-negative' : 'val-neutral'}`}>
              {state.risk.max_consecutive_losses === 0
                ? 'Off'
                : state.risk.cooldown_remaining_secs
                  ? `Cooldown ${state.risk.cooldown_remaining_secs}s`
                  : `${state.risk.consecutive_losses}/${state.risk.max_consecutive_losses}`}
            </span>
          </div>
        </div>
      </header>

      {activeTab === 'dashboard' ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '1.5rem' }}>
          {/* Our Open Positions - FULL WIDTH */}
          <div className="glass-panel">
            <div className="panel-header">
              Our Positions
              <span className="panel-subtitle">Held: {Object.keys(state.positions || {}).length}</span>
            </div>
            <div className="table-container">
              <table>
                <thead>
                  <tr>
                    <th>Market</th>
                    <th>Source</th>
                    <th>Size</th>
                    <th>Our Avg</th>
                    <th>Target Avg</th>
                    <th>Diff %</th>
                    <th>Live PnL</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {Object.values(state.positions || {}).map((p, i) => {
                    const t = targets.find((x) => x.token_id === p.token_id);
                    const title = t ? t.title : `${p.token_id.substring(0, 15)}...`;
                    const sourceWallet = state.position_sources ? (state.position_sources[p.token_id] || 'Unknown') : 'Unknown';
                    const shortWallet = sourceWallet === 'Unknown' ? 'Unknown' : `${sourceWallet.substring(0, 6)}...${sourceWallet.substring(sourceWallet.length - 4)}`;
                    const ourAvg = num(p.average_entry_price);
                    const targetAvg = t ? num(t.avg_price) : 0;
                    const curPrice = t ? num(t.cur_price) : ourAvg;
                    const diffPct = targetAvg > 0 ? ((ourAvg - targetAvg) / targetAvg) * 100 : 0;
                    const pnlUsd = (curPrice - ourAvg) * num(p.size);
                    const pnlPct = ourAvg > 0 ? ((curPrice - ourAvg) / ourAvg) * 100 : 0;
                    
                    return (
                      <tr key={i}>
                        <td className="td-truncate" title={title} style={{ maxWidth: '300px' }}>{title}</td>
                        <td>
                          {sourceWallet !== 'Unknown' ? (
                            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                              <span style={{ fontFamily: 'monospace', color: 'var(--text-secondary)' }} title={sourceWallet}>{shortWallet}</span>
                              <CopyWalletButton wallet={sourceWallet} />
                            </div>
                          ) : (
                            <span style={{ fontFamily: 'monospace', color: 'var(--text-secondary)' }} title={sourceWallet}>{shortWallet}</span>
                          )}
                        </td>
                        <td>{num(p.size).toFixed(2)}</td>
                        <td>${ourAvg.toFixed(3)}</td>
                        <td>{targetAvg > 0 ? `$${targetAvg.toFixed(3)}` : '-'}</td>
                        <td className={targetAvg > 0 ? (diffPct > 0 ? 'val-negative' : 'val-positive') : ''}>
                          {targetAvg > 0 ? `${diffPct > 0 ? '+' : ''}${diffPct.toFixed(1)}%` : '-'}
                        </td>
                        <td className={pnlUsd !== 0 ? (pnlUsd > 0 ? 'val-positive' : 'val-negative') : ''}>
                          {pnlUsd > 0 ? '+' : ''}${pnlUsd.toFixed(2)} ({pnlPct > 0 ? '+' : ''}{pnlPct.toFixed(1)}%)
                        </td>
                        <td><span className="status status-HELD">HELD</span></td>
                      </tr>
                    );
                  })}
                  {Object.keys(state.positions || {}).length === 0 && (
                    <tr><td colSpan={8} style={{ textAlign: 'center', color: 'var(--text-secondary)' }}>No open positions</td></tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>

          <div className="grid-cols-2">
            <div style={{ display: 'flex', flexDirection: 'column', gap: '1.5rem' }}>

            {/* Active API Limit Orders */}
            <div className="glass-panel">
              <div className="panel-header">
                Active API Limits
                <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                  <span className="live-badge" style={{ fontSize: '0.6rem', padding: '2px 4px' }}>LIVE BOOK</span>
                  <span className="panel-subtitle">Pending: {(state.active_orders || []).length}</span>
                </div>
              </div>
              <div className="table-container">
                <table>
                  <thead>
                    <tr>
                      <th>Market</th>
                      <th>Source</th>
                      <th>Size</th>
                      <th>Limit Price</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(state.active_orders || []).map((order, i) => {
                      const tokenId = order.token_id;
                      const t = targets.find((x) => x.token_id === tokenId);
                      const title = t ? t.title : `${tokenId.substring(0, 15)}...`;
                      const sourceWallet = state.position_sources ? (state.position_sources[tokenId] || 'Unknown') : 'Unknown';
                      const shortWallet = sourceWallet === 'Unknown' ? 'Unknown' : `${sourceWallet.substring(0, 6)}...${sourceWallet.substring(sourceWallet.length - 4)}`;
                      
                      return (
                        <tr key={i}>
                          <td className="td-truncate" title={title}>{title}</td>
                          <td>
                            {sourceWallet !== 'Unknown' ? (
                              <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                                <span style={{ fontFamily: 'monospace', color: 'var(--text-secondary)' }} title={sourceWallet}>{shortWallet}</span>
                                <CopyWalletButton wallet={sourceWallet} />
                              </div>
                            ) : (
                              <span style={{ fontFamily: 'monospace', color: 'var(--text-secondary)' }} title={sourceWallet}>{shortWallet}</span>
                            )}
                          </td>
                          <td><span className={`side-${order.side}`}>{order.side}</span> {num(order.size).toFixed(2)}</td>
                          <td>${num(order.price).toFixed(3)}</td>
                          <td><span className={`status status-${order.side === 'BUY' ? 'QUEUED' : 'HELD'}`}>PENDING {order.side}</span></td>
                        </tr>
                      );
                    })}
                    {(state.active_orders || []).length === 0 && (
                      <tr><td colSpan={5} style={{ textAlign: 'center', color: 'var(--text-secondary)' }}>No resting limit orders</td></tr>
                    )}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Target Positions (Scanning/Watching) */}
            <div className="glass-panel">
              <div className="panel-header">
                Target Positions (Scanning / Watching)
                <span className="panel-subtitle">Total Scanned: {targets.length}</span>
              </div>
              <div className="table-container" style={{ maxHeight: '400px', overflowY: 'auto' }}>
                <table>
                  <thead>
                    <tr>
                      <th>Market</th>
                      <th>Side</th>
                      <th>Target Avg</th>
                      <th>Cur Price</th>
                      <th>PnL</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {targets.slice(0, 30).map((t, i) => {
                      const statusKey = getStatusLabel(t.status);
                      const pnl = num(t.percent_pnl) * 100;
                      return (
                        <tr key={i}>
                          <td className="td-truncate" title={t.title}>{t.title}</td>
                          <td>{t.outcome}</td>
                          <td>${num(t.avg_price).toFixed(3)}</td>
                          <td>${num(t.cur_price).toFixed(3)}</td>
                          <td className={pnl < 0 ? 'val-negative' : pnl > 0 ? 'val-positive' : ''}>
                            {pnl > 0 ? '+' : ''}{pnl.toFixed(1)}%
                          </td>
                          <td>
                            <div style={{ display: 'flex', flexDirection: 'column' }}>
                              <span className={`status status-${statusKey.replace('Skipped', '')}`}>
                                {statusKey}
                              </span>
                              {statusKey === 'Monitoring' && t.engine_reason && (
                                <span style={{ fontSize: '0.75rem', color: 'var(--val-negative)', marginTop: '4px' }}>
                                  Skipped: {t.engine_reason}
                                </span>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>

          </div>

          <div className="glass-panel" style={{ alignSelf: 'flex-start', position: 'sticky', top: '2rem' }}>
            <div className="panel-header">Live Feed</div>
            <div className="feed-list" style={{ maxHeight: '800px' }}>
              {(state.live_feed || []).slice(0, 20).map((feed, i) => {
                const ev = feed.original_event;
                return (
                  <div key={i} className="feed-item">
                    <div className="feed-item-header">
                      <span className={`side-${ev.side}`}>{ev.side}</span>
                      <span className="feed-token">{ev.token_id.substring(0, 8)}...</span>
                      <span>${num(ev.price).toFixed(3)}</span>
                    </div>
                    <div style={{ fontSize: '0.875rem' }}>Size: {num(ev.size).toFixed(2)}</div>
                    {!feed.validated && feed.reason && (
                      <div className="feed-reason">Skipped: {feed.reason}</div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </div>
      ) : (
        <SettingsManager />
      )}
    </div>
  );
}

export default App;
