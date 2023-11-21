use std::{collections::HashMap, sync::Arc};

use blockifier::{
    block_context::{BlockContext, FeeTokenAddresses, GasPrices},
    state::cached_state::{CachedState, GlobalContractCache},
};
use forking::state::ForkStateReader;
use starknet::core::types::BlockId;
use starknet_api::{
    block::{BlockNumber, BlockTimestamp},
    contract_address,
    core::{ChainId, ContractAddress, PatriciaKey},
    hash::StarkHash,
    patricia_key,
};

pub fn convert_array(arr: Vec<String>) -> Vec<String> {
    // Convert String to u64
    let num_transactions = arr[0].clone().parse::<i32>().unwrap();
    let mut converted_arr: Vec<String> = Vec::new();
    converted_arr.push(arr[0].clone());

    for i in 0..num_transactions {
        let contract_address = arr[4 * i as usize + 1].clone();
        let selector = arr[4 * i as usize + 2].clone();
        converted_arr.push(contract_address);
        converted_arr.push(selector);

        let calldata_len_current = arr[4 * i as usize + 4].clone().parse::<i32>().unwrap();
        converted_arr.push(arr[4 * i as usize + 4].clone());

        let start_index: usize = 4 * num_transactions as usize
            + 2
            + arr[4 * i as usize + 3].parse::<i32>().unwrap() as usize;
        let end_index: usize = start_index + calldata_len_current as usize;

        let args = &arr[start_index..end_index];
        converted_arr.extend_from_slice(args);
    }
    converted_arr
}

const TEST_SEQUENCER_ADDRESS: &str = "0x1000";
const DEFAULT_ETH_L1_GAS_PRICE: u128 = 1 * u128::pow(10, 9); // Given in units of Wei.
const DEFAULT_STRK_L1_GAS_PRICE: u128 = 1 * u128::pow(10, 9); // Given in units of STRK.

pub const OUTPUT_BUILTIN_NAME: &str = "output_builtin";
pub const HASH_BUILTIN_NAME: &str = "pedersen_builtin";
pub const RANGE_CHECK_BUILTIN_NAME: &str = "range_check_builtin";
pub const SIGNATURE_BUILTIN_NAME: &str = "ecdsa_builtin";
pub const BITWISE_BUILTIN_NAME: &str = "bitwise_builtin";
pub const EC_OP_BUILTIN_NAME: &str = "ec_op_builtin";
pub const KECCAK_BUILTIN_NAME: &str = "keccak_builtin";
pub const POSEIDON_BUILTIN_NAME: &str = "poseidon_builtin";
pub const SEGMENT_ARENA_BUILTIN_NAME: &str = "segment_arena_builtin";

fn default_resource_fee_costs() -> HashMap<String, f64> {
    const N_STEPS_FEE_WEIGHT: f64 = 0.01;

    HashMap::from([
        (
            blockifier::abi::constants::N_STEPS_RESOURCE.to_string(),
            N_STEPS_FEE_WEIGHT,
        ),
        (HASH_BUILTIN_NAME.to_string(), 32.0 * N_STEPS_FEE_WEIGHT),
        (
            RANGE_CHECK_BUILTIN_NAME.to_string(),
            16.0 * N_STEPS_FEE_WEIGHT,
        ),
        (
            SIGNATURE_BUILTIN_NAME.to_string(),
            2048.0 * N_STEPS_FEE_WEIGHT,
        ),
        (BITWISE_BUILTIN_NAME.to_string(), 64.0 * N_STEPS_FEE_WEIGHT),
        (POSEIDON_BUILTIN_NAME.to_string(), 32.0 * N_STEPS_FEE_WEIGHT),
        (OUTPUT_BUILTIN_NAME.to_string(), 0.0 * N_STEPS_FEE_WEIGHT),
        (EC_OP_BUILTIN_NAME.to_string(), 1024.0 * N_STEPS_FEE_WEIGHT),
        (KECCAK_BUILTIN_NAME.to_string(), 2048.0 * N_STEPS_FEE_WEIGHT),
    ])
}

pub fn create_fork_cached_state_at(
    chain_id: ChainId,
    block_id: BlockId,
    cache_dir: &str,
) -> CachedState<ForkStateReader> {
    let url = match chain_id.0.as_str() {
        "0x534e5f474f45524c49" => "https://ikah.goerli1-juno.rpc.nethermind.io",
        "0x534e5f4d41494e" => "https://ofsg.mainnet-juno.rpc.nethermind.io",
        _ => panic!("Invalid chain id"),
    };
    CachedState::new(
        ForkStateReader::new(url, block_id, Some(cache_dir)),
        GlobalContractCache::default(),
    )
}

pub fn get_block_context(chain_id: ChainId, block_at: BlockNumber) -> BlockContext {
    BlockContext {
        chain_id: chain_id,
        block_number: block_at,
        block_timestamp: BlockTimestamp::default(),
        sequencer_address: contract_address!(TEST_SEQUENCER_ADDRESS),
        fee_token_addresses: FeeTokenAddresses {
            eth_fee_token_address: contract_address!(
                "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
            ),
            strk_fee_token_address: contract_address!(
                "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
            ),
        },
        vm_resource_fee_cost: Arc::new(default_resource_fee_costs()),
        gas_prices: GasPrices {
            eth_l1_gas_price: DEFAULT_ETH_L1_GAS_PRICE,
            strk_l1_gas_price: DEFAULT_STRK_L1_GAS_PRICE,
        },
        invoke_tx_max_n_steps: 1_000_000,
        validate_max_n_steps: 1_000_000,
        max_recursion_depth: 50,
    }
}
