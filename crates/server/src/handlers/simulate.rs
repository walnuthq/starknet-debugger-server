use crate::app_state::AppState;
use crate::db;
use axum::{extract::State, http::StatusCode, Extension, Json};
use blockifier::block_context::{BlockContext, FeeTokenAddresses, GasPrices};
use blockifier::state::cached_state::{CachedState, GlobalContractCache};
use blockifier::transaction::transaction_execution::Transaction;
use blockifier::transaction::transactions::ExecutableTransaction;
use forking::state::ForkStateReader;
use num_bigint::BigUint;
use num_traits::Num;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json;
use starknet::core::types::BlockId;
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{
    Calldata, Fee, InvokeTransaction as SAInvokeTransaction, InvokeTransactionV1,
    L1HandlerTransaction, Transaction as StarknetApiTransaction, TransactionHash,
    TransactionSignature, TransactionVersion,
};
use starknet_api::{calldata, contract_address, patricia_key, stark_felt};
use starknet_providers::{
    jsonrpc::{HttpTransport, JsonRpcClient},
    Provider,
};
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

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

#[derive(Serialize, Deserialize)]
pub struct StarkNetTransaction {
    // Define your transaction structure here;
    chain_id: String,
    wallet_address: String,
    calldata: Vec<String>,
    nonce: u32,
    max_fee: Option<u128>,
    version: u8,
    cairo_version: Option<String>,
}

#[derive(Serialize)]
pub struct SimulateResult {}

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
    block_id: BlockId,
    cache_dir: &str,
) -> CachedState<ForkStateReader> {
    CachedState::new(
        ForkStateReader::new(
            "https://ofsg.mainnet-juno.rpc.nethermind.io",
            block_id,
            Some(cache_dir),
        ),
        GlobalContractCache::default(),
    )
}

fn convert_to_hex(num_str: &str) -> String {
    let num = BigUint::from_str_radix(num_str, 10).unwrap();
    format!("{:x}", num)
}

pub async fn simulate(
    Extension(team): Extension<db::Team>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StarkNetTransaction>,
) -> Result<Json<SimulateResult>, StatusCode> {
    let rpc_client = create_rpc_client();

    let block_number = rpc_client.block_number().await.unwrap();

    // TODO: Insert into database
    let mut sim = db::Simulation::default();
    sim.team_id = team.id;
    sim.chain_id = payload.chain_id;
    sim.block_at = block_number as i32;
    sim.transaction_version = payload.version as i32;
    sim.nonce = payload.nonce as i32;
    sim.max_fee = match payload.max_fee {
        Some(max_fee) => max_fee.to_string(),
        None => String::from(""),
    };
    sim.cairo_version = match payload.cairo_version {
        Some(version) => version,
        None => String::from(""),
    };
    sim.wallet_address = payload.wallet_address;
    sim.calldata = payload.calldata;

    dbg!(sim.clone());

    // Insert into database
    // sqlx::query!(
    //     "INSERT INTO simulations (team_id, chain_id, block_at, transaction_version, nonce, max_fee, cairo_version, wallet_address, calldata) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    //     sim.team_id,
    //     sim.chain_id,
    //     sim.block_at,
    //     sim.transaction_version,
    //     sim.nonce,
    //     sim.max_fee,
    //     sim.cairo_version,
    //     sim.wallet_address,
    //     &sim.calldata,
    // ).execute(&state.db_pool).await.unwrap();

    // TODO(jainkunal): Execute the transaction in context of the block and get status
    let calldata_raw: Vec<StarkFelt> = sim
        .calldata
        .iter()
        .map(|x| stark_felt!(convert_to_hex(x).as_str()))
        .collect();

    let tx_raw = InvokeTransactionV1 {
        sender_address: contract_address!(sim.wallet_address.as_str()),
        nonce: Nonce(StarkFelt::from(payload.nonce)),
        calldata: Calldata(calldata_raw.into()),
        max_fee: Fee::default(),
        signature: TransactionSignature(vec![]),
    };

    let tx_hash = TransactionHash(StarkHash::default());
    let tx = Transaction::from_api(
        StarknetApiTransaction::Invoke(SAInvokeTransaction::V1(tx_raw)),
        tx_hash,
        None,
        None,
        None,
    )
    .unwrap();

    let block_context = BlockContext {
        chain_id: ChainId(sim.chain_id),
        block_number: BlockNumber(sim.block_at as u64),
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
    };
    // TODO: Don't use File cache
    let mut cached_fork_state = create_fork_cached_state_at(
        BlockId::Number(sim.block_at as u64),
        "/tmp/sn-debugger/cache",
    );

    let mut tx_state = CachedState::<_>::create_transactional(&mut cached_fork_state);
    let tx_info = tx.execute(&mut tx_state, &block_context, true, true);
    dbg!(tx_info);

    // TODO(jainkunal): Update status to DB

    // TODO: Return

    Ok(Json(SimulateResult {}))
}

fn create_rpc_client() -> JsonRpcClient<HttpTransport> {
    JsonRpcClient::new(HttpTransport::new(
        Url::parse(
            std::env::var("NODE_URL")
                .unwrap_or("https://ofsg.mainnet-juno.rpc.nethermind.io".to_string())
                .as_str(),
        )
        .unwrap(),
    ))
}

async fn query_node(method: &str, params: HashMap<&str, &str>) -> serde_json::Value {
    let json_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });

    let node_url = "https://ofsg.mainnet-juno.rpc.nethermind.io";

    let client = reqwest::Client::new();
    let res = client
        .post(node_url)
        .json(&json_payload)
        .send()
        .await
        .unwrap();

    let data: HashMap<String, serde_json::Value> = res.json().await.unwrap();

    data.get("result").unwrap().clone()
}
