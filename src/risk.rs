//! Risk engine — stateful pre-trade checks run by the strategy engine.
//!
//! A single instance is shared (`SharedRiskEngine`) between the strategy
//! engine (which calls [`RiskEngine::check_trade`]) and the order watcher
//! (which calls [`RiskEngine::record_loss`]), so loss state is observed by
//! both.
//!
//! ## Checks (configured in `config.toml`)
//!
//! | Check | Config key | Default | Applies to |
//! |---|---|---|---|
//! | Micro-trade anti-spoofing | — | $1 minimum | BUY |
//! | Target trade size bounds | `scan_min_amount` / `scan_max_amount` | see config | BUY |
//! | Consecutive-loss circuit breaker | `max_consecutive_losses`, `loss_cooldown_secs` | 0 (disabled) | BUY |
//! | Daily volume limit | `max_daily_volume_usd` | 0 (disabled) | BUY (SELL never counted or blocked) |
//! | Rapid-flip guard | — | 60s cooldown per token | BUY |
//!
//! SELL (closing) trades are never rejected by the risk engine.
//!
//! ## Loss-streak semantics
//!
//! * The streak increments only when an order is cancelled at a loss
//!   ([`RiskEngine::record_loss`], called by the order watcher).
//! * It resets only on a profitable close ([`RiskEngine::record_profitable_close`])
//!   or at UTC midnight. Approving a BUY does **not** reset it.

use crate::config::Config;
use crate::models::{TradeEvent, TradeSide};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Risk engine shared between the strategy engine and the order watcher.
pub type SharedRiskEngine = Arc<Mutex<RiskEngine>>;

/// Re-entry cooldown for the rapid-flip guard.
const RAPID_FLIP_SECS: u64 = 60;

pub struct RiskEngine {
    config: Config,
    /// Total BUY notional (USD) approved today (UTC day).
    daily_volume_usd: Decimal,
    /// UTC date when `daily_volume_usd` was last reset.
    daily_reset_date: chrono::NaiveDate,
    /// Number of loss-triggered cancellations since the last profitable close.
    consecutive_losses: u32,
    /// If `Some(until)`, new BUYs are blocked until `Instant::now() >= until`.
    cooldown_until: Option<Instant>,
    /// token_id → time of last approved BUY for that token.
    last_buy_at: HashMap<String, Instant>,
}

impl RiskEngine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            daily_volume_usd: Decimal::ZERO,
            daily_reset_date: chrono::Utc::now().date_naive(),
            consecutive_losses: 0,
            cooldown_until: None,
            last_buy_at: HashMap::new(),
        }
    }

    /// Convenience constructor for the shared handle.
    pub fn shared(config: Config) -> SharedRiskEngine {
        Arc::new(Mutex::new(Self::new(config)))
    }

    /// Run all risk checks against an incoming `TradeEvent`.
    ///
    /// Returns `Ok(())` if the trade may proceed, or `Err(reason)` to reject it.
    /// SELL events always pass and are not counted toward daily volume.
    pub fn check_trade(&mut self, trade: &TradeEvent) -> Result<(), String> {
        self.roll_day();

        if trade.side != TradeSide::BUY {
            return Ok(());
        }

        let trade_value = trade.size * trade.price;

        if trade_value < Decimal::ONE {
            return Err("Trade value is too small (spoofing protection)".to_string());
        }

        if trade_value < self.config.scan_min_amount || trade_value > self.config.scan_max_amount {
            return Err(format!(
                "Trade USD value ({}) falls outside allowed bounds ({} - {})",
                trade_value, self.config.scan_min_amount, self.config.scan_max_amount
            ));
        }

        if let Some(remaining) = self.cooldown_remaining_secs() {
            return Err(format!(
                "Risk cooldown active ({remaining}s remaining) after {} consecutive losses",
                self.config.max_consecutive_losses
            ));
        }

        if self.config.max_daily_volume_usd > Decimal::ZERO
            && self.daily_volume_usd + trade_value > self.config.max_daily_volume_usd
        {
            return Err(format!(
                "Daily volume limit ${:.2} would be exceeded (used ${:.2}, trade ${:.2})",
                self.config.max_daily_volume_usd, self.daily_volume_usd, trade_value
            ));
        }

        if let Some(&last) = self.last_buy_at.get(&trade.token_id) {
            let age_secs = last.elapsed().as_secs();
            if age_secs < RAPID_FLIP_SECS {
                return Err(format!(
                    "Rapid-flip guard: token {} was entered {}s ago — cooldown {}s",
                    &trade.token_id[..trade.token_id.len().min(12)],
                    age_secs,
                    RAPID_FLIP_SECS
                ));
            }
        }

        self.daily_volume_usd += trade_value;
        self.last_buy_at
            .insert(trade.token_id.clone(), Instant::now());
        Ok(())
    }

    /// Record that an order was cancelled at a loss (called by the order watcher).
    ///
    /// Triggers a cooldown once the configured threshold is reached.
    pub fn record_loss(&mut self) {
        if self.config.max_consecutive_losses == 0 {
            return; // feature disabled
        }
        self.consecutive_losses += 1;
        tracing::warn!(
            "Risk: consecutive losses = {} / {}",
            self.consecutive_losses,
            self.config.max_consecutive_losses
        );
        if self.consecutive_losses >= self.config.max_consecutive_losses {
            self.cooldown_until =
                Some(Instant::now() + Duration::from_secs(self.config.loss_cooldown_secs));
            tracing::warn!(
                "Risk: consecutive-loss limit hit — cooling down for {}s.",
                self.config.loss_cooldown_secs
            );
        }
    }

    /// Record a profitable close — the only event (besides UTC midnight) that
    /// resets the loss streak.
    pub fn record_profitable_close(&mut self) {
        self.consecutive_losses = 0;
    }

    /// Current loss streak.
    pub fn consecutive_losses(&self) -> u32 {
        self.consecutive_losses
    }

    /// Seconds left in an active cooldown, or `None` if not cooling down.
    /// Clears an expired cooldown (and the streak that caused it).
    pub fn cooldown_remaining_secs(&mut self) -> Option<u64> {
        let until = self.cooldown_until?;
        let now = Instant::now();
        if now < until {
            Some(until.duration_since(now).as_secs().max(1))
        } else {
            self.cooldown_until = None;
            self.consecutive_losses = 0;
            tracing::info!("Risk: consecutive-loss cooldown expired — resuming.");
            None
        }
    }

    fn roll_day(&mut self) {
        let today = chrono::Utc::now().date_naive();
        if today != self.daily_reset_date {
            self.daily_volume_usd = Decimal::ZERO;
            self.daily_reset_date = today;
            self.consecutive_losses = 0;
            tracing::info!("Risk: daily volume/loss counters reset for {today}.");
        }
    }
}
