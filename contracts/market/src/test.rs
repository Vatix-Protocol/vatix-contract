#[cfg(test)]
mod tests {
    use crate::{storage, MarketContract};
    use soroban_sdk::{Address, Bytes, BytesN, Env, String};

    // Helper to generate deterministic test addresses
    // Using valid 56-character Stellar account strkeys
    fn test_address(env: &Env, suffix: u8) -> Address {
        let strkey = match suffix {
            1 => "GBRPYHIL2C3FQ4ZK3QGNY7LPFQ4M7QWYAAAAAAAAAAAAAA", // ends with A
            2 => "GBRPYHIL2C3FQ4ZK3QGNY7LPFQ4M7QWYAAAAAAAAAAAAAB", // ends with B
            // You can easily add more:
            // 3 => "GBRPYHIL2C3FQ4ZK3QGNY7LPFQ4M7QWYAAAAAAAAAAAAAC",
            _ => panic!("Unsupported suffix {} in test_address", suffix),
        };

        Address::from_str(env, strkey)
    }
}
