use crate::{CalloraSettlement, CalloraSettlementClient, SettlementError};
use soroban_sdk::{testutils::Address as _, Address, Env, InvokeError};

trait ErrorCodeEq {
    fn code_eq(&self, expected: SettlementError) -> bool;
}

impl ErrorCodeEq for soroban_sdk::Error {
    fn code_eq(&self, expected: SettlementError) -> bool {
        *self == soroban_sdk::Error::from_contract_error(expected as u32)
    }
}

impl ErrorCodeEq for SettlementError {
    fn code_eq(&self, expected: SettlementError) -> bool {
        *self == expected
    }
}

fn is_not_initialized<T, C, E: ErrorCodeEq>(
    result: Result<Result<T, C>, Result<E, InvokeError>>,
) -> bool {
    match result {
        Err(Ok(err)) => err.code_eq(SettlementError::NotInitialized),
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
    assert!(is_not_initialized(
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
