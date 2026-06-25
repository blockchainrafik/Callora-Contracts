use crate::{CalloraSettlement, CalloraSettlementClient, DeveloperBalance, SettlementError};
use soroban_sdk::{testutils::Address as _, Address, Env, InvokeError};

/// Matches when the contract panics with NotInitialized.
/// Works for both Result-returning and non-Result-returning functions.
fn is_not_initialized<T, U>(result: Result<Result<T, U>, Result<soroban_sdk::Error, InvokeError>>) -> bool {
    match result {
        // Non-Result function path: error decoded as Error in Ok slot
        Err(Ok(err)) => err.is_type(soroban_sdk::xdr::ScErrorType::Contract)
            && err.get_code() == SettlementError::NotInitialized as u32,
        // Result-returning function path: InvokeError
        Err(Err(InvokeError::Contract(code))) => code == SettlementError::NotInitialized as u32,
        _ => false,
    }
}

/// For get_all_developer_balances which returns Result<Vec, SettlementError>.
fn is_not_initialized_result(result: Result<Result<soroban_sdk::Vec<DeveloperBalance>, soroban_sdk::ConversionError>, Result<SettlementError, InvokeError>>) -> bool {
    match result {
        Err(Ok(SettlementError::NotInitialized)) => true,
        Err(Err(InvokeError::Contract(code))) => code == SettlementError::NotInitialized as u32,
        _ => false,
    }
}

#[test]
fn test_get_admin_uninitialized() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_admin()));
}

#[test]
fn test_get_vault_uninitialized() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_vault()));
}

#[test]
fn test_get_global_pool_uninitialized() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_global_pool()));
}

#[test]
fn test_get_developer_balance_uninitialized() {
    let env = Env::default();
    let dev = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_developer_balance(&dev)));
}

#[test]
fn test_get_all_developer_balances_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);
    let dummy = Address::generate(&env);

    // get_all_developer_balances calls get_admin internally, which returns NotInitialized
    assert!(is_not_initialized_result(
        client.try_get_all_developer_balances(&dummy)
    ));
}

#[test]
fn test_get_developer_balance_returns_zero_when_not_stored() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let dev = Address::generate(&env);

    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.init(&admin, &vault);

    let balance = client.get_developer_balance(&dev);
    assert_eq!(balance, 0);
}
