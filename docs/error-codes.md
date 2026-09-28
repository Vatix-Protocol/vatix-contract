# Contract Error Codes (Issue #907)

Every Vatix contract returns a `#[contracterror]` enum (`#[repr(u32)]`). The numeric
codes below are part of the **public ABI**: clients (for example
`apps/web/lib/errors.ts`) and indexers key off the number, not the name.

## Invariants

- **Stable codes:** never renumber, reuse, or delete a released code. Deprecated
  variants stay in the enum; new variants take an unused number in their range.
- **Fail closed:** entrypoints return a typed error and never panic on
  caller-controlled input; a missing/unknown state is an error, not a default.
- **Deterministic not-found:** market lookups that miss return
  `MarketNotFound = 1` (market contract), never a generic validation error.
- **Authz errors are explicit:** wrong caller/role maps to `Unauthorized`,
  `NotAdmin`, or `WrongRole` so policy failures are observable.
- **Upgrade safety:** a storage-layout mismatch returns `UpgradeRequired`
  before any write (see [`scripts/upgrade/UPGRADE_PLAYBOOK.md`](../scripts/upgrade/UPGRADE_PLAYBOOK.md)).

Codes are scoped per contract: `1` in the market contract is not the same error
as `1` in the treasury contract. Always decode with the contract id.

## `market` — [`contracts/market/src/error.rs`](../contracts/market/src/error.rs)

| Code | Variant                         |
| ---: | ------------------------------- |
|    1 | `MarketNotFound`                |
|    2 | `MarketAlreadyResolved`         |
|    3 | `MarketNotResolved`             |
|    4 | `MarketExpired`                 |
|    5 | `MarketNotActive`               |
|    6 | `MarketClosedToDeposits`        |
|    7 | `WithdrawCooldownActive`        |
|    8 | `MarketAlreadyClosed`           |
|    9 | `ClaimBeforeResolve`            |
|   10 | `InsufficientCollateral`        |
|   11 | `PositionAlreadySettled`        |
|   12 | `NoPositionFound`               |
|   13 | `InvalidShareAmount`            |
|   14 | `BatchTooLarge`                 |
|   20 | `InvalidSignature`              |
|   21 | `UnauthorizedOracle`            |
|   22 | `InvalidOutcome`                |
|   23 | `OraclePriceUnavailable`        |
|   24 | `OracleMessageExpired`          |
|   25 | `InvalidThresholdQuorum`        |
|   26 | `StalePrice`                    |
|   30 | `InvalidPrice`                  |
|   31 | `InvalidQuantity`               |
|   32 | `InvalidTimestamp`              |
|   33 | `InvalidQuestion`               |
|   34 | `InvalidOutcomeCount`           |
|   35 | `InvalidAdmin`                  |
|   36 | `BelowMinDeposit`               |
|   37 | `InvalidMetadataUri`            |
|   38 | `InvalidFeeRate`                |
|   39 | `InvalidFeeWaiverAccount`       |
|   40 | `MetadataUriSchemeNotAllowed`   |
|   41 | `Unauthorized`                  |
|   90 | `UnknownDeploymentId`           |
|   91 | `DeploymentIdNetworkMismatch`   |
|   92 | `InvalidDeploymentId`           |
|   93 | `DeploymentRegistryUnavailable` |
|   94 | `DeploymentIdAlreadyRegistered` |

## `outcome-token` — [`contracts/outcome-token/src/error.rs`](../contracts/outcome-token/src/error.rs)

| Code | Variant                         |
| ---: | ------------------------------- |
|    1 | `AlreadyInitialized`            |
|    2 | `NotInitialized`                |
|    3 | `Unauthorized`                  |
|    4 | `InsufficientBalance`           |
|    5 | `InvalidAmount`                 |
|    6 | `Overflow`                      |
|    7 | `MarketNotResolved`             |
|    8 | `UpgradeRequired`               |
|    9 | `NoPendingMarketContractChange` |
|   10 | `TimelockNotElapsed`            |
|   11 | `TransferBlockedAfterResolve`   |
|   12 | `ContractPaused`                |
|   13 | `EmptyMetadata`                 |
|   14 | `InvalidDecimals`               |
|   15 | `MetadataTooLong`               |
|   16 | `NotAdmin`                      |

## `resolution` — [`contracts/resolution/src/error.rs`](../contracts/resolution/src/error.rs)

| Code | Variant                         |
| ---: | ------------------------------- |
|    1 | `CandidateNotFound`             |
|    2 | `CandidateAlreadyExists`        |
|    3 | `CandidateAlreadyChallenged`    |
|    4 | `CandidateAlreadyFinalized`     |
|    5 | `ChallengeWindowOpen`           |
|    6 | `ChallengeWindowClosed`         |
|    7 | `InvalidChallengeWindow`        |
|    8 | `InvalidEvidenceUri`            |
|    9 | `SignatureExpired`              |
|   10 | `InvalidSignatureExpiry`        |
|   11 | `MarketAlreadyResolved`         |
|   12 | `CandidateNotChallenged`        |
|   13 | `AppealLimitExceeded`           |
|   14 | `InsufficientBond`              |
|   15 | `InsufficientCollateral`        |
|   16 | `InvalidCollateral`             |
|   17 | `InsufficientChallengeBond`     |
|   18 | `NotArbitrable`                 |
|   19 | `ArbitrationTimelockNotElapsed` |
|   40 | `Unauthorized`                  |
|   41 | `NotAdmin`                      |
|   42 | `AlreadyInitialized`            |
|   43 | `InvalidAdmin`                  |
|   50 | `EmergencyModeActive`           |
|   51 | `UpgradeRequired`               |
|   52 | `ContractPaused`                |

## `treasury` — [`contracts/treasury/src/error.rs`](../contracts/treasury/src/error.rs)

| Code | Variant               |
| ---: | --------------------- |
|    1 | `AlreadyInitialized`  |
|    2 | `NotInitialized`      |
|   10 | `Unauthorized`        |
|   11 | `WrongRole`           |
|   12 | `Disabled`            |
|   20 | `EmptyDistribution`   |
|   21 | `DuplicateRecipient`  |
|   22 | `InvalidAmount`       |
|   23 | `InsufficientBalance` |
|   24 | `AlreadyDistributed`  |
|   30 | `Overflow`            |
|   31 | `Underflow`           |
|   32 | `InvariantViolated`   |

See the doc comments on each variant in the source for when it is returned.
When adding a variant, update this table in the same PR.
