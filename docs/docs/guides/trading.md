---
sidebar_position: 9
title: Trading
---

# Trading

Pawz includes a trading dashboard for cryptocurrency operations via Coinbase, Ethereum DEX (Uniswap V3), and Solana DEX (Jupiter + PumpPortal).

:::warning
Trading involves real money. Configure safety limits before enabling.
:::

## Trading policy

Go to **Settings → Advanced** to configure safety limits:

| Setting | Default | Description |
|---------|---------|-------------|
| **Auto-approve** | Off | Allow trades without human approval |
| **Max trade (USD)** | $100 | Maximum single trade size |
| **Max daily loss (USD)** | $500 | Stop trading if daily losses exceed this |
| **Allowed pairs** | [] | Restrict to specific trading pairs |
| **Allow transfers** | Off | Allow sending crypto to external wallets |
| **Max transfer (USD)** | $0 | Maximum single transfer amount |

## Dashboard

The trading view shows:

| Card | Description |
|------|-------------|
| **Today's P&L** | Profit/loss for the current day |
| **Bought Today** | Total buy volume |
| **Sold Today** | Total sell volume |
| **Transfers** | Total transfer volume |
| **DEX Swaps** | Total DEX swap volume |
| **Total Operations** | Number of trades today |
| **Daily Spent** | Running total against daily limit |

## Coinbase

Enable the **Coinbase** skill and provide:
- `COINBASE_API_KEY`
- `COINBASE_API_SECRET`

**Tools:** `coinbase_balance`, `coinbase_price`, `coinbase_buy`, `coinbase_sell`, `coinbase_send`

## Ethereum DEX (Uniswap V3)

Enable the **DEX Trading** skill and provide:
- `DEX_RPC_URL` — Ethereum RPC endpoint
- `DEX_PRIVATE_KEY` — wallet private key
- `DEX_WALLET_ADDRESS` — wallet address

**Tools (13):**

| Tool | Description |
|------|-------------|
| `dex_wallet_create` | Create a new wallet |
| `dex_balance` | Check wallet balance |
| `dex_quote` | Get a swap quote |
| `dex_swap` | Execute a swap |
| `dex_transfer` | Transfer tokens |
| `dex_portfolio` | View portfolio |
| `dex_token_info` | Token details |
| `dex_check_token` | Token safety check |
| `dex_search_token` | Search for tokens |
| `dex_watch_wallet` | Monitor a wallet |
| `dex_whale_transfers` | Track large transfers |
| `dex_top_traders` | Top traders for a token |
| `dex_trending` | Trending tokens |

## Solana DEX (Jupiter + PumpPortal)

Enable the **Solana DEX** skill and provide:
- `JUPITER_API_KEY`
- `SOLANA_RPC_URL`
- `SOLANA_PRIVATE_KEY`
- `SOLANA_WALLET_ADDRESS`

**Tools (7):** `sol_wallet_create`, `sol_balance`, `sol_quote`, `sol_swap`, `sol_transfer`, `sol_portfolio`, `sol_token_info`

## Positions

Track open positions with:
- Entry price and amount
- Stop-loss percentage
- Take-profit percentage
- Current price tracking
- Status: `open`, `closed_sl`, `closed_tp`, `closed_manual`

## Security

- Coinbase and DEX credentials are **server-side only** — never injected into agent prompts
- Trade approval checks the trading policy (amount limits, allowed pairs, daily spend)
- All DEX and Solana tools are auto-approved (no HIL) to enable fast execution
- Transfers require explicit `allow_transfers: true` in the policy
