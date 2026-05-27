#![no_std]
/// # Callora Vault Contract — deposit/withdraw/deduct/distribute with pause circuit-breaker.
///
/// ## Pause Circuit Breaker
///
/// When the vault is paused:
/// - Deposits are blocked
/// - Single and batch deducts are blocked
/// - Owner withdrawals are ALLOWED (emergency recovery)
/// - Admin/owner configuration functions remain available
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

#[contracttype]
#[derive(Clone)]
pub struct DeductItem {
    pub amount: i128,
    pub request_id: Option<Symbol>,
}

#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,
    pub balance: i128,
    pub authorized_caller: Option<Address>,
    pub min_deposit: i128,
}

/// Payload for `withdraw` and `withdraw_to` events.
#[contracttype]
#[derive(Clone)]
pub struct WithdrawEventData {
    pub amount: i128,
    pub new_balance: i128,
}

/// Canonical storage keys for the Vault contract.
#[contracttype]
pub enum StorageKey {
    MetaKey,
    Admin,
    UsdcToken,
    Settlement,
    RevenuePool,
    /// Storage slot for `MAX_DEDUCT_KEY` (maximum allowed amount per deduct call).
    MaxDeduct,
    Paused,
    Metadata(String),
    PendingOwner,
    PendingAdmin,
    DepositorList,
}

pub const DEFAULT_MAX_DEDUCT: i128 = i128::MAX;
pub const DEFAULT_MIN_DEPOSIT: i128 = 1;
pub const MAX_BATCH_SIZE: u32 = 50;
pub const MAX_METADATA_LEN: u32 = 256;
pub const MAX_OFFERING_ID_LEN: u32 = 64;

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize the vault. Exactly-once; panics if called again.
    ///
    /// # Parameters
    /// - `owner` — vault owner; must sign the transaction.
    /// - `usdc_token` — USDC token contract address; must not be the vault itself.
    /// - `initial_balance` — optional starting balance (defaults to 0). The vault
    ///   must already hold at least this many USDC stroops on-ledger.
    /// - `authorized_caller` — optional address permitted to call `deduct`/`batch_deduct`.
    ///   Must not be the vault address.
    /// - `min_deposit` — minimum deposit amount (defaults to 1, must be > 0).
    /// - `revenue_pool` — optional revenue pool address; informational only.
    ///   Must not be the vault address.
    /// - `max_deduct` — maximum single deduction (defaults to `i128::MAX`, must be > 0).
    ///   Must be >= `min_deposit`.
    ///
    /// # Panics
    /// - `"vault already initialized"` — called more than once.
    /// - `"usdc_token cannot be vault address"` — self-referential token.
    /// - `"revenue_pool cannot be vault address"` — self-referential pool.
    /// - `"authorized_caller cannot be vault address"` — self-referential caller.
    /// - `"initial balance must be non-negative"` — negative initial balance.
    /// - `"min_deposit must be positive"` — `min_deposit <= 0`.
    /// - `"max_deduct must be positive"` — `max_deduct <= 0`.
    /// - `"min_deposit cannot exceed max_deduct"` — constraint violation.
    /// - `"initial_balance exceeds on-ledger USDC balance"` — vault underfunded.
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: Option<i128>,
        authorized_caller: Option<Address>,
        min_deposit: Option<i128>,
        revenue_pool: Option<Address>,
        max_deduct: Option<i128>,
    ) -> VaultMeta {
        owner.require_auth();
        let inst = env.storage().instance();
        if inst.has(&StorageKey::MetaKey) {
            panic!("vault already initialized");
        }
        assert!(
            usdc_token != env.current_contract_address(),
            "usdc_token cannot be vault address"
        );
        if let Some(p) = &revenue_pool {
            assert!(
                p != &env.current_contract_address(),
                "revenue_pool cannot be vault address"
            );
        }
        if let Some(ac) = &authorized_caller {
            assert!(
                ac != &env.current_contract_address(),
                "authorized_caller cannot be vault address"
            );
        }
        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");
        let min_d = min_deposit.unwrap_or(DEFAULT_MIN_DEPOSIT);
        assert!(min_d > 0, "min_deposit must be positive");
        let max_d = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);
        assert!(max_d > 0, "max_deduct must be positive");
        assert!(min_d <= max_d, "min_deposit cannot exceed max_deduct");
        if balance > 0 {
            let on_chain =
                token::Client::new(&env, &usdc_token).balance(&env.current_contract_address());
            assert!(
                on_chain >= balance,
                "initial_balance exceeds on-ledger USDC balance"
            );
        }
        let meta = VaultMeta {
            owner: owner.clone(),
            balance,
            authorized_caller,
            min_deposit: min_d,
        };
        inst.set(&StorageKey::MetaKey, &meta);
        inst.set(&StorageKey::UsdcToken, &usdc_token);
        inst.set(&StorageKey::Admin, &owner);
        if let Some(p) = revenue_pool {
            inst.set(&StorageKey::RevenuePool, &p);
        }
        inst.set(&StorageKey::MaxDeduct, &max_d);
        env.events()
            .publish((Symbol::new(&env, "init"), owner.clone()), balance);
        meta
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Return full vault state. Panics if vault is not initialized.
    pub fn get_meta(env: Env) -> VaultMeta {
        env.storage()
            .instance()
            .get(&StorageKey::MetaKey)
            .unwrap_or_else(|| panic!("vault not initialized"))
    }

    /// Return the current tracked USDC balance. Panics if vault is not initialized.
    pub fn balance(env: Env) -> i128 {
        Self::get_meta(env).balance
    }

    /// Return the current admin address. Panics if vault is not initialized.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .expect("vault not initialized")
    }

    /// Return the USDC token contract address. Panics if vault is not initialized.
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized")
    }

    /// Return the configured `MAX_DEDUCT_KEY` value.
    /// Returns `i128::MAX` (no cap) if not explicitly set.
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::MaxDeduct)
            .unwrap_or(DEFAULT_MAX_DEDUCT)
    }

    /// Return the configured settlement address.
    /// Panics with `"settlement address not set"` if `set_settlement` has not been called.
    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .unwrap_or_else(|| panic!("settlement address not set"))
    }

    /// Return the configured revenue pool address, or `None` if not set.
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::RevenuePool)
    }

    /// Return `(usdc_token, settlement, revenue_pool)` in one call.
    /// Useful for operators verifying deployment configuration.
    pub fn get_contract_addresses(env: Env) -> (Option<Address>, Option<Address>, Option<Address>) {
        let inst = env.storage().instance();
        (
            inst.get(&StorageKey::UsdcToken),
            inst.get(&StorageKey::Settlement),
            inst.get(&StorageKey::RevenuePool),
        )
    }

    /// Return `true` if the vault is currently paused, `false` otherwise.
    /// Returns `false` before the first `pause()` call (safe default).
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused)
            .unwrap_or(false)
    }

    /// Return `true` if `caller` is the owner or an allowed depositor.
    /// Panics if vault is not initialized.
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        let meta = Self::get_meta(env.clone());
        if caller == meta.owner {
            return true;
        }
        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env));
        list.contains(&caller)
    }

    #[allow(dead_code)]
    fn migrate(env: &Env) {
        let inst = env.storage().instance();
        if !inst.has(&StorageKey::Admin) {
            if let Some(meta) = inst.get::<_, VaultMeta>(&StorageKey::MetaKey) {
                inst.set(&StorageKey::Admin, &meta.owner);
            }
        }
    }

    /// Return stored offering metadata, or `None` if not set.
    pub fn get_metadata(env: Env, offering_id: String) -> Option<String> {
        env.storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id))
    }

    // -----------------------------------------------------------------------
    // Mutating functions
    // -----------------------------------------------------------------------

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let cur = Self::get_admin(env.clone());
        if caller != cur {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        env.events()
            .publish((Symbol::new(&env, "admin_nominated"), cur, new_admin), ());
    }

    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .expect("no admin transfer pending");
        pending.require_auth();
        let cur = Self::get_admin(env.clone());
        env.storage().instance().set(&StorageKey::Admin, &pending);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((Symbol::new(&env, "admin_accepted"), cur, pending), ());
    }

    pub fn require_owner(env: Env, caller: Address) {
        let meta = Self::get_meta(env.clone());
        assert!(caller == meta.owner, "unauthorized: owner only");
    }

    pub fn set_authorized_caller(env: Env, new_caller: Option<Address>) {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        let old = meta.authorized_caller.clone();
        meta.authorized_caller = new_caller.clone();
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.events().publish(
            (
                Symbol::new(&env, "set_authorized_caller"),
                meta.owner.clone(),
            ),
            (old, new_caller),
        );
    }

    /// Set `MAX_DEDUCT_KEY` (owner only).
    ///
    /// # Panics
    /// - `"max_deduct must be positive"` when `max_deduct <= 0`.
    /// - `"vault not initialized"` if called before `init`.
    pub fn set_max_deduct(env: Env, max_deduct: i128) {
        let meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(max_deduct > 0, "max_deduct must be positive");
        let old = Self::get_max_deduct(env.clone());
        env.storage()
            .instance()
            .set(&StorageKey::MaxDeduct, &max_deduct);
        env.events().publish(
            (Symbol::new(&env, "set_max_deduct"), meta.owner),
            (old, max_deduct),
        );
    }

    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());

        match depositor {
            Some(d) => {
                let mut list: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&StorageKey::DepositorList)
                    .unwrap_or(Vec::new(&env));
                if !list.contains(&d) {
                    list.push_back(d);
                }
                env.storage()
                    .instance()
                    .set(&StorageKey::DepositorList, &list);
            }
            None => {
                env.storage()
                    .instance()
                    .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
            }
        }
    }

    pub fn clear_allowed_depositors(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
    }

    fn require_authorized_deduct_caller(env: Env, caller: &Address) {
        let meta = Self::get_meta(env.clone());
        let owner = meta.owner.clone();
        let auth = match meta.authorized_caller {
            Some(ac) => *caller == ac || *caller == owner,
            None => *caller == owner,
        };
        assert!(auth, "unauthorized caller");
    }

    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }
}
