// Typed client for the local Polycopier API.
//
// Every /api request carries the per-process token that the backend appends
// to the dashboard URL (`?token=...`). The token is kept in sessionStorage so
// reloads keep working, and is stripped from the visible URL.

const TOKEN_KEY = 'polycopier_token';
const TOKEN_HEADER = 'x-polycopier-token';

function initToken(): string {
  const params = new URLSearchParams(window.location.search);
  const fromUrl = params.get('token');
  if (fromUrl) {
    sessionStorage.setItem(TOKEN_KEY, fromUrl);
    params.delete('token');
    const qs = params.toString();
    window.history.replaceState(null, '', window.location.pathname + (qs ? `?${qs}` : ''));
    return fromUrl;
  }
  return sessionStorage.getItem(TOKEN_KEY) ?? '';
}

const token = initToken();

export function hasToken(): boolean {
  return token.length > 0;
}

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set(TOKEN_HEADER, token);
  if (init.body !== undefined) headers.set('Content-Type', 'application/json');
  const res = await fetch(path, { ...init, headers });
  const text = await res.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  if (!res.ok) {
    const msg =
      data && typeof data === 'object' && 'error' in data
        ? String((data as { error: unknown }).error)
        : text || res.statusText;
    throw new ApiError(res.status, msg);
  }
  return data as T;
}

export const postJson = <T>(path: string, body: unknown) =>
  apiFetch<T>(path, { method: 'POST', body: JSON.stringify(body) });

// ── DTOs (mirror src/api.rs / src/models.rs; Decimal serializes as string) ──

export type Dec = string | number;
export type TradeSide = 'BUY' | 'SELL';
export type ScanStatus =
  | 'Monitoring'
  | 'Entered'
  | 'SkippedOwned'
  | 'SkippedLoss'
  | 'SkippedGain'
  | 'SkippedPrice'
  | 'SkippedExpired'
  | 'SkippedSize';

export interface Position {
  token_id: string;
  size: Dec;
  average_entry_price: Dec;
}

export interface TargetPosition {
  title: string;
  outcome: string;
  token_id: string;
  cur_price: Dec;
  avg_price: Dec;
  percent_pnl: Dec;
  size: Dec;
  status: ScanStatus;
  source_wallet: string;
  engine_reason: string | null;
}

export interface ActiveApiOrder {
  token_id: string;
  price: Dec;
  size: Dec;
  side: TradeSide;
  original_size: Dec;
}

export interface TradeEvent {
  transaction_hash: string;
  maker_address: string;
  taker_address: string;
  token_id: string;
  price: Dec;
  size: Dec;
  side: TradeSide;
  timestamp: number;
}

export interface EvaluatedTrade {
  original_event: TradeEvent;
  validated: boolean;
  reason: string | null;
}

export interface QueuedOrder {
  token_id: string;
  price: Dec;
  size: Dec;
  side: TradeSide;
  event_end_date: string | null;
}

export interface RiskStatus {
  consecutive_losses: number;
  max_consecutive_losses: number;
  cooldown_remaining_secs: number | null;
}

export interface StateResponse {
  positions: Record<string, Position>;
  live_feed: EvaluatedTrade[];
  total_balance: Dec;
  unrealized_pnl: Dec;
  realized_pnl: Dec;
  target_positions: TargetPosition[];
  copies_executed: number;
  trades_skipped: number;
  copied_count: number;
  next_scan_secs: number;
  pending_orders: Record<string, QueuedOrder>;
  active_orders: ActiveApiOrder[];
  position_sources: Record<string, string>;
  is_sim: boolean;
  risk: RiskStatus;
}

export type SetupRequired = { status: 'setup_required' };

/** GET /api/env — the private key itself is never returned. */
export interface EnvStatus {
  private_key_set: boolean;
  signer_address: string | null;
  funder_address: string;
}

/** POST /api/env — omit / leave empty `private_key` to keep the current key. */
export interface EnvUpdate {
  private_key?: string;
  funder_address: string;
}

type ConfigValue = string | number | null | undefined;

/** config.toml as JSON (see BotConfig in src/config.rs). */
export interface BotConfig {
  targets: { wallets: string[] };
  sizing: { mode: string; copy_size_pct?: ConfigValue };
  execution: Record<string, ConfigValue>;
  scanner: Record<string, ConfigValue>;
  risk: Record<string, ConfigValue>;
  ledger?: Record<string, ConfigValue>;
  [section: string]: unknown;
}
