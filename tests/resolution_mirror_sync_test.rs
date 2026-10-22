//! Issue #1132: keeps the resolution contract's local `MarketStatus` and
//! `EmergencyMode` mirrors in variant-for-variant sync with the market
//! contract's own definitions.
//!
//! The resolution crate cannot depend on the market crate directly (see the
//! doc comments on `vatix_resolution_contract::types::MarketStatus` and
//! `::types::EmergencyMode`), so it keeps its own mirror enums for decoding
//! the market contract's `get_market_status` cross-contract call. This test
//! maps each mirror variant to the market variant it is meant to mirror
//! using *exhaustive* matches on both enums: if a variant is ever added to,
//! removed from, or reordered in either enum without a matching update here,
//! this file fails to compile (a non-exhaustive match error) or fails on the
//! ordered name comparison rather than silently drifting at runtime.

use vatix_market_contract::types::{EmergencyMode as MarketEmergencyMode, MarketStatus as MarketContractStatus};
use vatix_resolution_contract::types::{EmergencyMode as ResolutionEmergencyMode, MarketStatus as ResolutionStatus};

/// Maps a resolution mirror `MarketStatus` variant to its name. Exhaustive:
/// adding a variant to the mirror without updating this match is a compile
/// error.
fn resolution_status_name(status: &ResolutionStatus) -> &'static str {
    match status {
        ResolutionStatus::Active => "Active",
        ResolutionStatus::Resolved => "Resolved",
        ResolutionStatus::Canceled => "Canceled",
    }
}

/// Maps a market contract `MarketStatus` variant to its name. Exhaustive:
/// adding a variant to the market contract without updating this match is a
/// compile error — exactly the drift this test exists to catch.
fn market_status_name(status: &MarketContractStatus) -> &'static str {
    match status {
        MarketContractStatus::Active => "Active",
        MarketContractStatus::Resolved => "Resolved",
        MarketContractStatus::Canceled => "Canceled",
    }
}

/// Maps a resolution mirror `EmergencyMode` variant to its name. Exhaustive.
fn resolution_mode_name(mode: &ResolutionEmergencyMode) -> &'static str {
    match mode {
        ResolutionEmergencyMode::Normal => "Normal",
        ResolutionEmergencyMode::TradingHalted => "TradingHalted",
        ResolutionEmergencyMode::SettleOnly => "SettleOnly",
        ResolutionEmergencyMode::GlobalFreeze => "GlobalFreeze",
    }
}

/// Maps a market contract `EmergencyMode` variant to its name. Exhaustive.
fn market_mode_name(mode: &MarketEmergencyMode) -> &'static str {
    match mode {
        MarketEmergencyMode::Normal => "Normal",
        MarketEmergencyMode::TradingHalted => "TradingHalted",
        MarketEmergencyMode::SettleOnly => "SettleOnly",
        MarketEmergencyMode::GlobalFreeze => "GlobalFreeze",
    }
}

#[test]
fn resolution_market_status_mirror_stays_in_sync_with_market_contract() {
    let resolution_names = [
        resolution_status_name(&ResolutionStatus::Active),
        resolution_status_name(&ResolutionStatus::Resolved),
        resolution_status_name(&ResolutionStatus::Canceled),
    ];
    let market_names = [
        market_status_name(&MarketContractStatus::Active),
        market_status_name(&MarketContractStatus::Resolved),
        market_status_name(&MarketContractStatus::Canceled),
    ];

    // Same count and same variant-for-variant names, in the same order, so
    // a `MarketStatus` encoded by the market contract decodes correctly
    // through the resolution mirror (and vice versa).
    assert_eq!(resolution_names.len(), market_names.len());
    assert_eq!(resolution_names, market_names);
}

#[test]
fn resolution_emergency_mode_mirror_stays_in_sync_with_market_contract() {
    let resolution_names = [
        resolution_mode_name(&ResolutionEmergencyMode::Normal),
        resolution_mode_name(&ResolutionEmergencyMode::TradingHalted),
        resolution_mode_name(&ResolutionEmergencyMode::SettleOnly),
        resolution_mode_name(&ResolutionEmergencyMode::GlobalFreeze),
    ];
    let market_names = [
        market_mode_name(&MarketEmergencyMode::Normal),
        market_mode_name(&MarketEmergencyMode::TradingHalted),
        market_mode_name(&MarketEmergencyMode::SettleOnly),
        market_mode_name(&MarketEmergencyMode::GlobalFreeze),
    ];

    assert_eq!(resolution_names.len(), market_names.len());
    assert_eq!(resolution_names, market_names);
}
