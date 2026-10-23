# Web UI Dashboard

The dashboard is a React (Vite) app in `web/`, built to `web/dist/` and served by the Rust
backend on `127.0.0.1:3000` when started with `cargo run --release -- --ui`.
If `web/dist` is missing or older than `web/src`, the bot runs `npm install && npm run build` first
(Node.js required).

## Security
* The server binds to `127.0.0.1` only.
* A random token is generated per run and appended to the URL the bot opens
  (`http://localhost:3000/?token=…`, also printed in the log). The frontend stores it in
  `sessionStorage` and sends it as the `x-polycopier-token` header; every `/api/*` request
  without it is rejected (401).
* `Host` must be `localhost:3000` / `127.0.0.1:3000` and any `Origin` must match (403 otherwise).
  No CORS headers are sent, so other websites cannot call or read the API.
* The private key is **write-only**: `GET /api/env` returns only whether it is set, the signer
  address derived from it, and the funder address.

## 1. Setup Wizard
Shown when `PRIVATE_KEY` or `FUNDER_ADDRESS` is missing/invalid. Submitting writes `.env`
(and a default `config.toml` if absent), then the process restarts itself (`--ui-reboot`,
same token, no new browser tab). A missing `config.toml` alone does not trigger the wizard —
defaults are generated automatically.

## 2. Header
* **LIVE / SIM** badge — `SIM` when started with `--sim`.
* Total balance, unrealized PnL, copies executed, trades skipped, next scan countdown.
* **Loss Guard** — `Off`, current streak `n/max`, or `Cooldown Ns` while new BUYs are paused.

## 3. Dashboard
* **Our Positions** — currently held positions with source wallet, our vs target average price
  and live PnL. (Closed positions are not shown; see `copy_ledger.json`.)
* **Active orders** — open limit orders.
* **Target positions** — scanner view of target holdings and their status.
* **Live Feed** — last 20 evaluated trades with skip reasons.

## 4. Settings
Edits `config.toml` and `.env`. Saving writes both files; you are then asked to restart the
process to apply changes (settings are not hot-reloaded).

* **Secrets** — signer address (read-only), write-only private key field (leave empty to keep),
  funder address.
* **Targets** — add/remove target wallets.
* **Sizing mode** — `fixed`, `self_pct`, `target_usd`, `target_scalar`.
* **Risk** — max USD per trade, slippage, scanner bounds, max consecutive losses and cooldown
  (a loss = an order cancelled because the target's position fell past `max_copy_loss_pct`;
  the streak resets only on a profitable close).
