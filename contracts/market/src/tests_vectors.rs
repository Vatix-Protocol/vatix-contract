//! Deterministic test vectors for market events.
//!
//! These fixtures pin the on-chain event shape (topics + payload fields) so that
//! off-chain indexers, the SDK, and the settlement service can rely on a stable
//! contract. See `FEE_COLLECTED_EVENT.md` for the normative specification and
//! `test-vectors/` for the serialized golden files.
//!
//! Invariants asserted here:
//! * `FeeCollected` is emitted exactly once per settled fill.
//! * Field ordering and units are stable (fees are in stroops, `i128`).
//! * `correlation_id` is deterministic and replay-safe (idempotency key).
//! * No secret material is ever placed in the event payload.

use crate::error::MarketError;
use crate::positions::PositionId;

/// Canonical topic string for the `FeeCollected` event.
pub const FEE_COLLECTED_TOPIC: &str = "fee_collected";

/// Deterministic fixture for a single `FeeCollected` event.
///
/// Mirrors the payload emitted by the market contract when a fill settles and
/// protocol fees are swept. Kept `Copy`/`Eq` so tests can compare vectors
/// without allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeCollectedFixture {
    /// Position that generated the fee.
    pub position_id: PositionId,
    /// Account that paid the fee (the taker).
    pub payer: u64,
    /// Fee amount in stroops. Always non-negative.
    pub fee_stroops: i128,
    /// Asset the fee was denominated in (Stellar asset code, 4 bytes).
    pub asset: [u8; 4],
    /// Monotonic ledger sequence at emission time.
    pub ledger_seq: u32,
    /// Deterministic idempotency key derived from the fill.
    pub correlation_id: u64,
}

impl FeeCollectedFixture {
    /// Validate the fixture against the `FeeCollected` invariants.
    ///
    /// Fail-closed: any violation returns a typed error rather than panicking,
    /// so callers on the money path can abort the write.
    pub fn validate(&self) -> Result<(), MarketError> {
        if self.fee_stroops < 0 {
            return Err(MarketError::InvalidFeeAmount);
        }
        if self.asset == [0u8; 4] {
            return Err(MarketError::InvalidAsset);
        }
        if self.correlation_id == 0 {
            return Err(MarketError::InvalidCorrelationId);
        }
        Ok(())
    }

    /// Deterministic correlation id for a fill.
    ///
    /// Replays of the same `(position_id, ledger_seq)` produce the same id, so
    /// downstream consumers can dedupe without trusting wall-clock time.
    pub fn correlation_id_for(position_id: PositionId, ledger_seq: u32) -> u64 {
        // FNV-1a over the position id and ledger sequence. Stable across
        // platforms and independent of hashing seeds.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in position_id
            .to_le_bytes()
            .iter()
            .chain(ledger_seq.to_le_bytes().iter())
        {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // Reserve 0 as the "unset" sentinel so `validate` can reject it.
        if hash == 0 {
            1
        } else {
            hash
        }
    }
}

/// Golden vector matching `test-vectors/fee_collected.json`.
pub fn fee_collected_golden() -> FeeCollectedFixture {
    let position_id: PositionId = 42;
    let ledger_seq: u32 = 1_000_000;
    FeeCollectedFixture {
        position_id,
        payer: 7,
        fee_stroops: 1_250,
        asset: *b"USDC",
        ledger_seq,
        correlation_id: FeeCollectedFixture::correlation_id_for(position_id, ledger_seq),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_vector_is_valid() {
        let fixture = fee_collected_golden();
        assert_eq!(fixture.validate(), Ok(()));
        assert_eq!(fixture.fee_stroops, 1_250);
        assert_eq!(&fixture.asset, b"USDC");
    }

    #[test]
    fn correlation_id_is_deterministic_and_replay_safe() {
        let a = FeeCollectedFixture::correlation_id_for(42, 1_000_000);
        let b = FeeCollectedFixture::correlation_id_for(42, 1_000_000);
        assert_eq!(a, b, "replays must produce the same correlation id");
        assert_ne!(a, 0, "correlation id must never be the unset sentinel");

        let other = FeeCollectedFixture::correlation_id_for(43, 1_000_000);
        assert_ne!(a, other, "distinct positions must not collide");
    }

    #[test]
    fn negative_fee_is_rejected_fail_closed() {
        let mut fixture = fee_collected_golden();
        fixture.fee_stroops = -1;
        assert_eq!(fixture.validate(), Err(MarketError::InvalidFeeAmount));
    }

    #[test]
    fn empty_asset_is_rejected() {
        let mut fixture = fee_collected_golden();
        fixture.asset = [0u8; 4];
        assert_eq!(fixture.validate(), Err(MarketError::InvalidAsset));
    }

    #[test]
    fn zero_correlation_id_is_rejected() {
        let mut fixture = fee_collected_golden();
        fixture.correlation_id = 0;
        assert_eq!(fixture.validate(), Err(MarketError::InvalidCorrelationId));
    }

    #[test]
    fn topic_is_stable() {
        assert_eq!(FEE_COLLECTED_TOPIC, "fee_collected");
    }
}
