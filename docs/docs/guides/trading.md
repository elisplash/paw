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

### Self-custody wallets

Coinbase wallets are **self-custody** — private keys are generated locally and stored in the OS keychain with AES-256 encryption. Pawz never transmits private keys to any remote server. The encrypted keychain entry is tied to the current user session and is unlocked automatically at runtime.

## Ethereum DEX (Uniswap V3)

Enable the **DEX Trading** skill and provide:
- `DEX_RPC_URL` — Ethereum RPC endpoint
- `DEX_PRIVATE_KEY` — wallet private key
- `DEX_WALLET_ADDRESS` — wallet address

### Supported EVM chains

| Chain | Type |
|-------|------|
| **Ethereum** | Mainnet |
| **Goerli** | Testnet |
| **Sepolia** | Testnet |
| **Polygon** | L2 |
| **Arbitrum** | L2 |
| **Optimism** | L2 |
| **Base** | L2 |

### Known EVM tokens

The following 12 tokens are recognized by default (no address lookup required):

`ETH` · `WETH` · `USDC` · `USDT` · `DAI` · `WBTC` · `UNI` · `LINK` · `PEPE` · `SHIB` · `ARB` · `AAVE`

Any other ERC-20 token can be traded by providing its contract address.

### Fee tiers

Uniswap V3 pools use discrete fee tiers. The router automatically selects the tier with the best liquidity, but you can override it in `dex_quote`:

| Tier | Typical use |
|------|-------------|
| **0.01%** | Stable-to-stable pairs (e.g. USDC/USDT) |
| **0.05%** | Correlated pairs (e.g. ETH/WETH) |
| **0.3%** | Most pairs (default) |
| **1%** | Exotic / low-liquidity pairs |

### Slippage (EVM)

Default slippage tolerance is **0.5%**. Maximum allowed slippage is **5%**. You can override per-swap via the `slippage` parameter on `dex_swap`.

### Price feed

USD valuations for portfolio and P&L cards are sourced from the **DexScreener** price feed, which aggregates prices across all major DEXes in real time.

**Tools (14):**

| Tool | Description |
|------|-------------|
| `dex_wallet_create` | Create a new wallet |
| `dex_balance` | Check wallet balance |
| `dex_quote` | Get a swap quote |
| `dex_swap` | Execute a swap |
| `dex_approve` | Approve token spending for the Uniswap router |
| `dex_transfer` | Transfer tokens |
| `dex_portfolio` | View portfolio |
| `dex_token_info` | Token details |
| `dex_check_token` | Token safety check |
| `dex_search_token` | Search for tokens |
| `dex_watch_wallet` | Monitor a wallet |
| `dex_whale_transfers` | Track large transfers |
| `dex_top_traders` | Top traders for a token |
| `dex_trending` | Trending tokens |

### Honeypot detection

`dex_check_token` performs a comprehensive honeypot analysis before you trade an unknown token:

1. **Simulated buy + sell** — executes a round-trip trade in a forked environment to verify the token can actually be sold after purchase.
2. **Round-trip tax** — measures the percentage lost between buy and sell to detect hidden taxes or transfer fees.
3. **Ownership audit** — checks whether the contract owner can mint, pause, or blacklist.
4. **ERC-20 compliance** — verifies the token implements the standard ERC-20 interface correctly (transfer, approve, transferFrom, balanceOf).
5. **Risk score** — returns a score from **0** (safe) to **30** (extreme risk). Tokens scoring above 15 trigger a warning; above 25, the swap is blocked unless force-approved.

:::tip
Always run `dex_check_token` on unfamiliar tokens before swapping. A failing simulation is the strongest signal that a token is a honeypot.
:::

### Whale tracking

`dex_whale_transfers` monitors the blockchain for large token transfers and surfaces real-time alerts:

- Configurable USD threshold (default: $100,000)
- Shows sender, receiver, token, amount, and USD value
- Highlights transfers involving known exchange hot wallets or smart-contract deployers
- Useful for spotting accumulation or distribution before price moves

### Smart money

`dex_top_traders` identifies the most profitable wallets trading a specific token:

- Ranks wallets by realized P&L over 24h, 7d, or 30d
- Shows win rate, average trade size, and total volume
- Cross-references with known fund and MEV bot addresses
- Use this to follow high-conviction traders into new positions

## Solana DEX (Jupiter + PumpPortal)

Enable the **Solana DEX** skill and provide:
- `JUPITER_API_KEY`
- `SOLANA_RPC_URL`
- `SOLANA_PRIVATE_KEY`
- `SOLANA_WALLET_ADDRESS`

### Routing

Swaps are routed through the **Jupiter Metis v1 API**, which finds the optimal route across all Solana DEXes (Orca, Raydium, Meteora, etc.) in a single call.

For tokens launched on **pump.fun**, **PumpSwap**, or **Raydium** that are not yet indexed by Jupiter, the router falls back to **PumpPortal** for direct on-chain execution.

### Known Solana tokens

The following 11 tokens are recognized by default:

`SOL` · `USDC` · `USDT` · `BONK` · `JUP` · `RAY` · `PYTH` · `WIF` · `ORCA` · `MSOL` · `JITOSOL`

Any other SPL token can be traded by providing its mint address.

### Slippage (Solana)

Default slippage tolerance is **50 bps** (0.5%). For meme tokens with high volatility, slippage can be raised up to **5000 bps** (50%) via the `slippage_bps` parameter on `sol_swap`. Jupiter will always attempt the tightest execution within the allowed range.

### Transaction signing

All Solana transactions are signed locally using **Ed25519** (the Solana native signature scheme). Both **versioned (v0)** and **legacy** transaction formats are supported — the SDK auto-selects the best format based on the instructions involved (v0 is preferred when address lookup tables are available).

### SPL Token transfers

`sol_transfer` handles native SOL and any SPL token. For SPL transfers the tool:

1. **Derives** the recipient's Associated Token Account (ATA) from their wallet address and the token mint.
2. **Auto-creates** the ATA if it does not yet exist (the sender pays the ~0.002 SOL rent).
3. Transfers the specified amount in a single atomic transaction.

**Tools (8):**

| Tool | Description |
|------|-------------|
| `sol_wallet_create` | Create a new Solana wallet |
| `sol_balance` | Check wallet balance (SOL + SPL tokens) |
| `sol_quote` | Get a swap quote via Jupiter / PumpPortal |
| `sol_swap` | Execute a swap |
| `sol_transfer` | Transfer SOL or SPL tokens to another wallet |
| `sol_portfolio` | View full portfolio with USD valuations |
| `sol_token_info` | Token details (supply, holders, metadata) |
| `sol_trending` | Trending tokens on Solana |

## Positions

Track open positions with:
- Entry price and amount
- Stop-loss percentage
- Take-profit percentage
- Current price tracking
- Status: `open`, `closed_sl`, `closed_tp`, `closed_manual`

## Security

- Coinbase and DEX credentials are **server-side only** — never injected into agent prompts
- Coinbase uses **self-custody wallets** with private keys encrypted and stored in the OS keychain
- EVM transactions are signed locally; private keys never leave the process
- Solana transactions use **Ed25519** signing with support for versioned (v0) and legacy formats
- Trade approval checks the trading policy (amount limits, allowed pairs, daily spend)
- All DEX and Solana tools are auto-approved (no HIL) to enable fast execution
- Transfers require explicit `allow_transfers: true` in the policy
- USD valuations are sourced from the **DexScreener** price feed
