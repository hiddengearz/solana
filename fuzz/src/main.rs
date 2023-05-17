use {
    bincode::serialize,
    crossbeam_channel::unbounded,
    futures_util::StreamExt,
    honggfuzz::fuzz,
    log::*,
    reqwest::{self, header::CONTENT_TYPE},
    serde_json::{json, Value},
    solana_account_decoder::UiAccount,
    solana_client::{
        connection_cache::ConnectionCache,
        tpu_client::{TpuClient, TpuClientConfig},
    },
    solana_pubsub_client::nonblocking::pubsub_client::PubsubClient,
    solana_rpc_client::rpc_client::RpcClient,
    solana_rpc_client_api::{
        client_error::{ErrorKind as ClientErrorKind, Result as ClientResult},
        config::{RpcAccountInfoConfig, RpcSignatureSubscribeConfig},
        request::RpcError,
        response::{Response as RpcResponse, RpcSignatureResult, SlotUpdate},
    },
    solana_sdk::{
        commitment_config::CommitmentConfig,
        hash::Hash,
        pubkey::Pubkey,
        rent::Rent,
        signature::{Keypair, Signature, Signer},
        system_transaction,
        transaction::Transaction,
    },
    solana_streamer::socket::SocketAddrSpace,
    solana_test_validator::TestValidator,
    solana_tpu_client::tpu_client::DEFAULT_TPU_CONNECTION_POOL_SIZE,
    solana_transaction_status::TransactionStatus,
    std::{
        collections::HashSet,
        net::UdpSocket,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread::sleep,
        time::{Duration, Instant},
    },
    tokio::runtime::Runtime,
};

macro_rules! json_req {
    ($method: expr, $params: expr) => {{
        json!({
           "jsonrpc": "2.0",
           "id": 1,
           "method": $method,
           "params": $params,
        })
    }}
}

fn post_rpc(request: Value, rpc_url: &str) -> Value {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(rpc_url)
        .header(CONTENT_TYPE, "application/json")
        .body(request.to_string())
        .send()
        .unwrap();
    serde_json::from_str(&response.text().unwrap()).unwrap()
}

fn main() {
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();
    let test_validator =
        TestValidator::with_no_fees(mint_pubkey, None, SocketAddrSpace::Unspecified);
    let rpc_client = Arc::new(RpcClient::new_with_commitment(
        test_validator.rpc_url(),
        CommitmentConfig::processed(),
    ));

    loop {
        let connection_cache = ConnectionCache::new(DEFAULT_TPU_CONNECTION_POOL_SIZE);
        fuzz!(|data: Vec<u8>| {
            let recent_blockhash = rpc_client.as_ref().get_latest_blockhash().unwrap();
            let tx = system_transaction::transfer(
                &mint_keypair,
                &Pubkey::new_unique(),
                42,
                recent_blockhash,
            );
            let success = match connection_cache {
                ConnectionCache::Quic(cache) => TpuClient::new_with_connection_cache(
                    rpc_client.clone(),
                    &test_validator.rpc_pubsub_url(),
                    TpuClientConfig::default(),
                    cache,
                )
                .unwrap()
                .send_wire_transaction(data),
                ConnectionCache::Udp(cache) => TpuClient::new_with_connection_cache(
                    rpc_client.clone(),
                    &test_validator.rpc_pubsub_url(),
                    TpuClientConfig::default(),
                    cache,
                )
                .unwrap()
                .send_transaction(&tx),
            };
            assert!(success);
        });
    }

    /*
    let timeout = Duration::from_secs(5);
    let now = Instant::now();
    let signatures = vec![tx.signatures[0]];
    loop {
        assert!(now.elapsed() < timeout);
        let statuses = rpc_client.get_signature_statuses(&signatures).unwrap();
        if statuses.value.get(0).is_some() {
            return;
        }
    }
    */
}
