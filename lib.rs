#![doc = include_str!("../README.md")]

#[macro_use]
extern crate pbc_contract_codegen;

use create_type_spec_derive::CreateTypeSpec;
use read_write_rpc_derive::ReadWriteRPC;
use std::ops::Sub;

use defi_common::token_state::AbstractTokenState;
use pbc_contract_common::address::Address;
use pbc_contract_common::avl_tree_map::AvlTreeMap;
use pbc_contract_common::context::ContractContext;
use pbc_traits::ReadWriteState;
use read_write_state_derive::ReadWriteState;

/// Tokenomics structure to record intended distribution.
#[derive(ReadWriteState, CreateTypeSpec)]
pub struct Tokenomics {
    pub mining_rewards: u128,
    pub team_allocation: u128,
    pub reserve_fund: u128,
    pub public_allocation: u128,
}

/// MPC-20-v2 token contract compatible state.
#[state]
pub struct TokenState {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub owner: Address,
    pub total_supply: u128,
    pub balances: AvlTreeMap<Address, u128>,
    pub allowed: AvlTreeMap<AllowedAddress, u128>,
    pub tokenomics: Tokenomics,
    pub last_claim_times: AvlTreeMap<Address, u128>,
    pub upgrade_address: Option<Address>,
}

#[derive(ReadWriteState, CreateTypeSpec, Eq, Ord, PartialEq, PartialOrd)]
pub struct AllowedAddress {
    pub owner: Address,
    pub spender: Address,
}

trait BalanceMap<K: Ord, V> {
    fn insert_balance(&mut self, key: K, value: V);
}

impl<V: Sub<V, Output = V> + PartialEq + Copy + ReadWriteState, K: ReadWriteState + Ord>
    BalanceMap<K, V> for AvlTreeMap<K, V>
{
    fn insert_balance(&mut self, key: K, value: V) {
        let zero = value - value;
        if value == zero {
            self.remove(&key);
        } else {
            self.insert(key, value);
        }
    }
}

impl AbstractTokenState for TokenState {
    fn get_symbol(&self) -> &str {
        &self.symbol
    }

    fn update_balance(&mut self, owner: Address, amount: u128) {
        self.balances.insert_balance(owner, amount);
    }

    fn balance_of(&self, owner: &Address) -> u128 {
        self.balances.get(owner).unwrap_or(0)
    }

    fn allowance(&self, owner: &Address, spender: &Address) -> u128 {
        self.allowed
            .get(&AllowedAddress {
                owner: *owner,
                spender: *spender,
            })
            .unwrap_or(0)
    }

    fn update_allowance(&mut self, owner: Address, spender: Address, amount: u128) {
        self.allowed
            .insert_balance(AllowedAddress { owner, spender }, amount);
    }
}

#[init]
pub fn initialize(
    ctx: ContractContext,
    name: String,
    symbol: String,
    decimals: u8,
    total_supply: u128,
) -> TokenState {
    let tokenomics = Tokenomics {
        mining_rewards: total_supply * 30 / 100,
        team_allocation: total_supply * 20 / 100,
        reserve_fund: total_supply * 10 / 100,
        public_allocation: total_supply * 40 / 100,
    };

    let mut initial_state = TokenState {
        name,
        symbol,
        decimals,
        owner: ctx.sender,
        total_supply,
        balances: AvlTreeMap::new(),
        allowed: AvlTreeMap::new(),
        tokenomics,
        last_claim_times: AvlTreeMap::new(),
        upgrade_address: None,
    };

    initial_state.update_balance(ctx.sender, total_supply);
    initial_state
}

#[derive(ReadWriteRPC, CreateTypeSpec)]
pub struct Transfer {
    pub to: Address,
    pub amount: u128,
}

#[action(shortname = 0x01)]
pub fn transfer(
    context: ContractContext,
    mut state: TokenState,
    to: Address,
    amount: u128,
) -> TokenState {
    state.transfer(context.sender, to, amount);
    state
}

#[action(shortname = 0x02)]
pub fn bulk_transfer(
    context: ContractContext,
    mut state: TokenState,
    transfers: Vec<Transfer>,
) -> TokenState {
    for t in transfers {
        state.transfer(context.sender, t.to, t.amount);
    }
    state
}

#[action(shortname = 0x03)]
pub fn transfer_from(
    context: ContractContext,
    mut state: TokenState,
    from: Address,
    to: Address,
    amount: u128,
) -> TokenState {
    state.transfer_from(context.sender, from, to, amount);
    state
}

#[action(shortname = 0x04)]
pub fn bulk_transfer_from(
    context: ContractContext,
    mut state: TokenState,
    from: Address,
    transfers: Vec<Transfer>,
) -> TokenState {
    for t in transfers {
        state.transfer_from(context.sender, from, t.to, t.amount);
    }
    state
}

#[action(shortname = 0x05)]
pub fn approve(
    context: ContractContext,
    mut state: TokenState,
    spender: Address,
    amount: u128,
) -> TokenState {
    state.update_allowance(context.sender, spender, amount);
    state
}

#[action(shortname = 0x07)]
pub fn approve_relative(
    context: ContractContext,
    mut state: TokenState,
    spender: Address,
    delta: i128,
) -> TokenState {
    state.update_allowance_relative(context.sender, spender, delta);
    state
}

#[action(shortname = 0x06)]
pub fn claim_reward(context: ContractContext, mut state: TokenState) -> TokenState {
    let now: u128 = context.block_time.try_into().unwrap();
    let last_claim = state.last_claim_times.get(&context.sender).unwrap_or(0);

    if now < last_claim + 86400 {
        panic!("Claim not available yet. Please wait 24 hours between claims.");
    }

    if state.tokenomics.mining_rewards < 20 {
        panic!("Insufficient mining rewards remaining.");
    }

    let current_balance = state.balance_of(&context.sender);
    state.update_balance(context.sender, current_balance + 20);
    state.tokenomics.mining_rewards -= 20;
    state.last_claim_times.insert(context.sender, now);

    state
}

#[action(shortname = 0x08)]
pub fn burn(
    context: ContractContext,
    mut state: TokenState,
    from: Address,
    amount: u128,
) -> TokenState {
    if context.sender != state.owner {
        panic!("Only the owner can burn tokens.");
    }

    let from_balance = state.balance_of(&from);
    if from_balance < amount {
        panic!("Insufficient balance to burn.");
    }

    state.update_balance(from, from_balance - amount);
    state.total_supply -= amount;

    state
}

#[action(shortname = 0x09)]
pub fn upgrade(
    context: ContractContext,
    mut state: TokenState,
    new_address: Address,
) -> TokenState {
    if context.sender != state.owner {
        panic!("Only the owner can set the upgrade address.");
    }

    state.upgrade_address = Some(new_address);

    state
}
