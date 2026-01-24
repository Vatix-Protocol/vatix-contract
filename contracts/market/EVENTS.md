# Vatix Contract Events

This document describes the event emission system implemented in the Vatix market contract.

## Overview

The event system allows off-chain indexers and backends to listen for on-chain state changes and update their databases accordingly. This enables real-time updates, audit trails, and efficient data synchronization.

## Event Types

### 1. Market Created Event
- **Symbol**: `MKT_CREAT`
- **Topics**: `(symbol, market_id)`
- **Data**: `(question, end_time, creator)`
- **Emitted when**: A new market is created

### 2. Market Resolved Event
- **Symbol**: `MKT_RESOL`
- **Topics**: `(symbol, market_id)`
- **Data**: `(outcome, resolved_at)`
- **Emitted when**: A market outcome is resolved

### 3. Position Updated Event
- **Symbol**: `POS_UPD`
- **Topics**: `(symbol, market_id, user_address)`
- **Data**: `(yes_shares, no_shares, locked_collateral)`
- **Emitted when**: User's position changes (buy/sell shares)

### 4. Position Settled Event
- **Symbol**: `POS_SETT`
- **Topics**: `(symbol, market_id, user_address)`
- **Data**: `(payout, settled_at)`
- **Emitted when**: User settles their position after market resolution

### 5. Collateral Deposited Event
- **Symbol**: `COLLAT_DEP`
- **Topics**: `(symbol, market_id, user_address)`
- **Data**: `amount`
- **Emitted when**: User deposits collateral to a market

### 6. Collateral Withdrawn Event
- **Symbol**: `COLLAT_WD`
- **Topics**: `(symbol, market_id, user_address)`
- **Data**: `amount`
- **Emitted when**: User withdraws collateral from a market

## Usage in Contract Code

```rust
use crate::events;

// Emit market created event
let market_id_str = String::from_str(&env, &market_id.to_string());
events::emit_market_created(&env, &market_id_str, &question, end_time, &creator);

// Emit position updated event
events::emit_position_updated(&env, &market_id, &user, yes_shares, no_shares, locked_collateral);
```

## Backend Integration

Off-chain services can listen for these events to:

1. **Update Database**: Sync on-chain state with off-chain database
2. **Real-time Updates**: Push updates to frontend applications
3. **Analytics**: Track market statistics and user behavior
4. **Audit Trail**: Maintain complete history of all actions

## Event Filtering

Events can be filtered by:
- **Symbol**: Filter by event type (e.g., only market creation events)
- **Market ID**: Filter events for specific markets
- **User Address**: Filter events for specific users

## Testing

The event system includes comprehensive tests in `events_test.rs` that verify:
- All event types are emitted correctly
- Event data structure is valid
- Multiple events can be emitted in sequence
- Events integrate properly with contract functions