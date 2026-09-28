//! Outcome token contract.
//!
//! Manages outcome tokens for prediction markets. Each outcome token carries
//! metadata (symbol + name) that is surfaced to clients and indexers. To keep
//! metadata well-formed and prevent griefing / storage abuse, symbol and name
//! lengths are capped (see [`MAX_SYMBOL_LEN`] and [`MAX_NAME_LEN`]).

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, String};

/// Maximum number of bytes allowed in an outcome token symbol.
pub const MAX_SYMBOL_LEN: u32 = 12;

/// Maximum number of bytes allowed in an outcome token name.
pub const MAX_NAME_LEN: u32 = 64;

/// Stable, typed error codes for the outcome token contract.
///
/// These codes are part of the public ABI: clients and indexers rely on them
/// to distinguish failure modes without parsing strings.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OutcomeTokenError {
    /// Caller is not authorized to mutate outcome metadata.
    Unauthorized = 1,
    /// Symbol is empty.
    EmptySymbol = 2,
    /// Symbol exceeds [`MAX_SYMBOL_LEN`].
    SymbolTooLong = 3,
    /// Name is empty.
    EmptyName = 4,
    /// Name exceeds [`MAX_NAME_LEN`].
    NameTooLong = 5,
}

/// Persisted metadata for an outcome token.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutcomeMetadata {
    pub symbol: String,
    pub name: String,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Metadata,
}

#[contract]
pub struct OutcomeTokenContract;

#[contractimpl]
impl OutcomeTokenContract {
    /// Initialize the contract with an admin and the initial outcome metadata.
    ///
    /// Fails closed: metadata is validated before any state is written, so an
    /// invalid symbol/name can never be persisted.
    pub fn initialize(
        env: Env,
        admin: Address,
        symbol: String,
        name: String,
    ) -> Result<(), OutcomeTokenError> {
        admin.require_auth();
        validate_metadata(&symbol, &name)?;

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(
            &DataKey::Metadata,
            &OutcomeMetadata {
                symbol: symbol.clone(),
                name: name.clone(),
            },
        );
        Ok(())
    }

    /// Update the outcome metadata. Only the admin may call this.
    ///
    /// Validation runs before the write so untrusted clients cannot bypass the
    /// length caps, and a rejected update leaves existing metadata untouched.
    pub fn set_metadata(
        env: Env,
        caller: Address,
        symbol: String,
        name: String,
    ) -> Result<(), OutcomeTokenError> {
        caller.require_auth();
        require_admin(&env, &caller)?;
        validate_metadata(&symbol, &name)?;

        env.storage().instance().set(
            &DataKey::Metadata,
            &OutcomeMetadata {
                symbol: symbol.clone(),
                name: name.clone(),
            },
        );
        Ok(())
    }

    /// Read the current outcome metadata.
    pub fn metadata(env: Env) -> Result<OutcomeMetadata, OutcomeTokenError> {
        env.storage()
            .instance()
            .get(&DataKey::Metadata)
            .ok_or(OutcomeTokenError::Unauthorized)
    }

    /// Read the current outcome symbol.
    pub fn symbol(env: Env) -> Result<String, OutcomeTokenError> {
        Ok(Self::metadata(env)?.symbol)
    }

    /// Read the current outcome name.
    pub fn name(env: Env) -> Result<String, OutcomeTokenError> {
        Ok(Self::metadata(env)?.name)
    }
}

/// Enforce the metadata length caps. Deny-by-default: empty values are
/// rejected, and any value over the cap is rejected with a stable error code.
fn validate_metadata(symbol: &String, name: &String) -> Result<(), OutcomeTokenError> {
    let symbol_len = symbol.len();
    if symbol_len == 0 {
        return Err(OutcomeTokenError::EmptySymbol);
    }
    if symbol_len > MAX_SYMBOL_LEN {
        return Err(OutcomeTokenError::SymbolTooLong);
    }

    let name_len = name.len();
    if name_len == 0 {
        return Err(OutcomeTokenError::EmptyName);
    }
    if name_len > MAX_NAME_LEN {
        return Err(OutcomeTokenError::NameTooLong);
    }

    Ok(())
}

/// Ensure the caller is the stored admin. Deny-by-default when no admin is set.
fn require_admin(env: &Env, caller: &Address) -> Result<(), OutcomeTokenError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(OutcomeTokenError::Unauthorized)?;
    if &admin != caller {
        return Err(OutcomeTokenError::Unauthorized);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    fn setup() -> (Env, OutcomeTokenContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, OutcomeTokenContract);
        let client = OutcomeTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        (env, client, admin)
    }

    fn symbol(env: &Env, len: u32) -> String {
        String::from_str(env, &"s".repeat(len as usize))
    }

    fn name(env: &Env, len: u32) -> String {
        String::from_str(env, &"n".repeat(len as usize))
    }

    #[test]
    fn accepts_symbol_and_name_at_cap() {
        let (env, client, admin) = setup();
        let sym = symbol(&env, MAX_SYMBOL_LEN);
        let nm = name(&env, MAX_NAME_LEN);
        client.initialize(&admin, &sym, &nm);
        assert_eq!(client.symbol(), sym);
        assert_eq!(client.name(), nm);
    }

    #[test]
    fn rejects_symbol_over_cap() {
        let (env, client, admin) = setup();
        let sym = symbol(&env, MAX_SYMBOL_LEN + 1);
        let nm = name(&env, 4);
        let res = client.try_initialize(&admin, &sym, &nm);
        assert_eq!(res, Err(Ok(OutcomeTokenError::SymbolTooLong)));
    }

    #[test]
    fn rejects_name_over_cap() {
        let (env, client, admin) = setup();
        let sym = symbol(&env, 4);
        let nm = name(&env, MAX_NAME_LEN + 1);
        let res = client.try_initialize(&admin, &sym, &nm);
        assert_eq!(res, Err(Ok(OutcomeTokenError::NameTooLong)));
    }

    #[test]
    fn rejects_empty_symbol_and_name() {
        let (env, client, admin) = setup();
        let empty = String::from_str(&env, "");
        let nm = name(&env, 4);
        assert_eq!(
            client.try_initialize(&admin, &empty, &nm),
            Err(Ok(OutcomeTokenError::EmptySymbol))
        );

        let sym = symbol(&env, 4);
        assert_eq!(
            client.try_initialize(&admin, &sym, &empty),
            Err(Ok(OutcomeTokenError::EmptyName))
        );
    }

    #[test]
    fn set_metadata_enforces_caps() {
        let (env, client, admin) = setup();
        client.initialize(&admin, &symbol(&env, 4), &name(&env, 4));

        let over = symbol(&env, MAX_SYMBOL_LEN + 1);
        let res = client.try_set_metadata(&admin, &over, &name(&env, 4));
        assert_eq!(res, Err(Ok(OutcomeTokenError::SymbolTooLong)));

        // Existing metadata is untouched after a rejected update.
        assert_eq!(client.symbol(), symbol(&env, 4));
    }

    #[test]
    fn set_metadata_rejects_non_admin() {
        let (env, client, admin) = setup();
        client.initialize(&admin, &symbol(&env, 4), &name(&env, 4));

        let attacker = Address::generate(&env);
        let res = client.try_set_metadata(&attacker, &symbol(&env, 4), &name(&env, 4));
        assert_eq!(res, Err(Ok(OutcomeTokenError::Unauthorized)));
    }
}
