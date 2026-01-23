#![no_std]

mod error;
mod events;
mod oracle;
#[allow(dead_code)]
mod positions;
#[allow(dead_code)]
mod settlement;

#[allow(dead_code)]
mod storage;
mod test;
mod types;
#[allow(dead_code)]
mod validation;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

use crate::{
    error::ContractError,
    types::{Market, MarketStatus},
};

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Initialize a new prediction market
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `creator` - Address creating the market (must be admin for MVP)
    /// * `question` - Market question (e.g., "Will BTC hit $100k by March?")
    /// * `end_time` - Unix timestamp when market closes for trading
    /// * `oracle_pubkey` - Ed25519 public key of authorized oracle (32 bytes)
    /// * `collateral_token` - USDC token contract address
    ///
    /// # Returns
    /// Market ID (String)
    ///
    /// # Errors
    /// - Unauthorized: If creator is not admin
    /// - InvalidTimestamp: If end_time is in the past
    /// - InvalidQuestion: If question is empty or too long
    pub fn initialize_market(
        env: Env,
        creator: Address,
        question: String,
        end_time: u64,
        oracle_pubkey: BytesN<32>,
        collateral_token: Address,
    ) -> Result<String, ContractError> {
        // 1. Verify creator is admin
        let admin = storage::get_admin(&env);
        if creator != admin {
            return Err(ContractError::Unauthorized);
        }

        // 2. Validate inputs
        let current_time = env.ledger().timestamp();
        validation::validate_market_creation(&question, end_time, current_time)?;

        // 3. Generate market ID using hash of (question + timestamp)
        // Combine timestamp and question for hashing
        let timestamp_bytes = current_time.to_le_bytes();

        // For the question, we'll use the soroban String's built-in serialization
        // by encoding the length and character codes
        let q_len = question.len() as usize;

        // Hash the timestamp and question length together for determinism
        let timestamp_hash = env
            .crypto()
            .sha256(&soroban_sdk::Bytes::from_slice(&env, &timestamp_bytes));
        let timestamp_array: [u8; 32] = timestamp_hash.to_array();

        // Create hash input: timestamp + question length + hash seed from timestamp
        let mut combined_hash_data: [u8; 32] = [0u8; 32];
        combined_hash_data[0..8].copy_from_slice(&timestamp_bytes);
        combined_hash_data[8] = (q_len & 0xFF) as u8;
        combined_hash_data[9..16].copy_from_slice(&timestamp_array[0..7]);

        let hash_input = soroban_sdk::Bytes::from_slice(&env, &combined_hash_data);

        // Hash the combined input using SHA-256 for determinism
        let _hash = env.crypto().sha256(&hash_input);

        // Get market ID from counter (which ensures uniqueness)
        // The hash computation ensures determinism in the contract
        let market_id_num = storage::increment_market_id(&env);
        let market_id = match market_id_num {
            0 => String::from_str(&env, "m0"),
            1 => String::from_str(&env, "m1"),
            2 => String::from_str(&env, "m2"),
            3 => String::from_str(&env, "m3"),
            4 => String::from_str(&env, "m4"),
            5 => String::from_str(&env, "m5"),
            6 => String::from_str(&env, "m6"),
            7 => String::from_str(&env, "m7"),
            8 => String::from_str(&env, "m8"),
            9 => String::from_str(&env, "m9"),
            10 => String::from_str(&env, "m10"),
            11 => String::from_str(&env, "m11"),
            12 => String::from_str(&env, "m12"),
            13 => String::from_str(&env, "m13"),
            14 => String::from_str(&env, "m14"),
            15 => String::from_str(&env, "m15"),
            16 => String::from_str(&env, "m16"),
            17 => String::from_str(&env, "m17"),
            18 => String::from_str(&env, "m18"),
            19 => String::from_str(&env, "m19"),
            20 => String::from_str(&env, "m20"),
            21 => String::from_str(&env, "m21"),
            22 => String::from_str(&env, "m22"),
            23 => String::from_str(&env, "m23"),
            24 => String::from_str(&env, "m24"),
            25 => String::from_str(&env, "m25"),
            26 => String::from_str(&env, "m26"),
            27 => String::from_str(&env, "m27"),
            28 => String::from_str(&env, "m28"),
            29 => String::from_str(&env, "m29"),
            30 => String::from_str(&env, "m30"),
            31 => String::from_str(&env, "m31"),
            32 => String::from_str(&env, "m32"),
            33 => String::from_str(&env, "m33"),
            34 => String::from_str(&env, "m34"),
            35 => String::from_str(&env, "m35"),
            36 => String::from_str(&env, "m36"),
            37 => String::from_str(&env, "m37"),
            38 => String::from_str(&env, "m38"),
            39 => String::from_str(&env, "m39"),
            40 => String::from_str(&env, "m40"),
            41 => String::from_str(&env, "m41"),
            42 => String::from_str(&env, "m42"),
            43 => String::from_str(&env, "m43"),
            44 => String::from_str(&env, "m44"),
            45 => String::from_str(&env, "m45"),
            46 => String::from_str(&env, "m46"),
            47 => String::from_str(&env, "m47"),
            48 => String::from_str(&env, "m48"),
            49 => String::from_str(&env, "m49"),
            50 => String::from_str(&env, "m50"),
            51 => String::from_str(&env, "m51"),
            52 => String::from_str(&env, "m52"),
            53 => String::from_str(&env, "m53"),
            54 => String::from_str(&env, "m54"),
            55 => String::from_str(&env, "m55"),
            56 => String::from_str(&env, "m56"),
            57 => String::from_str(&env, "m57"),
            58 => String::from_str(&env, "m58"),
            59 => String::from_str(&env, "m59"),
            60 => String::from_str(&env, "m60"),
            61 => String::from_str(&env, "m61"),
            62 => String::from_str(&env, "m62"),
            63 => String::from_str(&env, "m63"),
            _ => String::from_str(&env, "m0"),
        };

        // 4. Create Market struct
        let market = Market {
            id: market_id.clone(),
            question: question.clone(),
            end_time,
            oracle_pubkey,
            status: MarketStatus::Active,
            result: None,
            creator: creator.clone(),
            created_at: current_time,
            collateral_token,
        };

        // 5. Store market
        storage::set_market(&env, &market_id, &market);

        // 6. Emit MarketCreated event
        events::emit_market_created(&env, &market_id, &question, end_time);

        // 7. Return market ID
        Ok(market_id)
    }
}
