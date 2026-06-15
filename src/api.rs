use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::types::{Chain, Transaction, WalletInfo};

// ─── Blockstream.info BTC structs ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BtcAddressInfo {
    address: String,
    chain_stats: BtcStats,
    mempool_stats: BtcStats,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BtcStats {
    funded_txo_count: u64,
    funded_txo_sum: u64,
    spent_txo_count: u64,
    spent_txo_sum: u64,
    tx_count: u64,
}

#[derive(Debug, Deserialize)]
struct BtcTx {
    txid: String,
    status: BtcTxStatus,
    vin: Vec<BtcVin>,
    vout: Vec<BtcVout>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BtcTxStatus {
    confirmed: bool,
    block_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct BtcVin {
    prevout: Option<BtcVout>,
}

#[derive(Debug, Deserialize)]
struct BtcVout {
    value: u64,
    scriptpubkey_address: Option<String>,
}

pub async fn fetch_btc_wallet(client: &Client, address: &str) -> Result<WalletInfo> {
    let base = "https://blockstream.info/api";

    // Fetch address summary
    let info_url = format!("{}/address/{}", base, address);
    let info: BtcAddressInfo = client
        .get(&info_url)
        .send()
        .await?
        .json()
        .await
        .map_err(|e| anyhow!("BTC address parse error: {}", e))?;

    let total_received = (info.chain_stats.funded_txo_sum + info.mempool_stats.funded_txo_sum) as f64 / 1e8;
    let total_sent = (info.chain_stats.spent_txo_sum + info.mempool_stats.spent_txo_sum) as f64 / 1e8;
    let balance = total_received - total_sent;
    let tx_count = info.chain_stats.tx_count + info.mempool_stats.tx_count;

    // Fetch tx list (up to 25 most recent)
    let txs_url = format!("{}/address/{}/txs", base, address);
    let raw_txs: Vec<BtcTx> = client
        .get(&txs_url)
        .send()
        .await?
        .json()
        .await
        .unwrap_or_default();

    let mut transactions: Vec<Transaction> = raw_txs
        .iter()
        .map(|tx| {
            let date: Option<DateTime<Utc>> = tx
                .status
                .block_time
                .and_then(|ts| Utc.timestamp_opt(ts, 0).single());

            // Calculate net amount for this address
            let input_sum: i64 = tx
                .vin
                .iter()
                .filter_map(|v| v.prevout.as_ref())
                .filter(|o| o.scriptpubkey_address.as_deref() == Some(address))
                .map(|o| o.value as i64)
                .sum();

            let output_sum: i64 = tx
                .vout
                .iter()
                .filter(|o| o.scriptpubkey_address.as_deref() == Some(address))
                .map(|o| o.value as i64)
                .sum();

            let net: i64 = output_sum - input_sum;

            // Counterparty: first output address that isn't ours
            let counterparty = tx
                .vout
                .iter()
                .find(|o| o.scriptpubkey_address.as_deref() != Some(address))
                .and_then(|o| o.scriptpubkey_address.clone());

            let btc_net = net as f64 / 1e8;
            let sign = if btc_net >= 0.0 { "+" } else { "" };
            let amount_display = format!("{}{:.8} BTC", sign, btc_net);

            Transaction {
                txid: tx.txid.clone(),
                date,
                amount_satoshis: net,
                amount_display,
                counterparty,
                chain: Chain::BTC,
            }
        })
        .collect();

    transactions.sort_by(|a, b| a.date.cmp(&b.date));

    let first_seen = transactions.iter().filter_map(|t| t.date).min();
    let last_seen = transactions.iter().filter_map(|t| t.date).max();

    Ok(WalletInfo {
        address: address.to_string(),
        chain: Chain::BTC,
        balance,
        balance_display: format!("{:.8} BTC", balance),
        total_received,
        total_sent,
        tx_count,
        first_seen,
        last_seen,
        transactions,
    })
}

// ─── Etherscan ETH structs ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EthBalanceResponse {
    status: String,
    result: String,
}

#[derive(Debug, Deserialize)]
struct EthTxListResponse {
    status: String,
    result: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct EthTx {
    hash: String,
    #[serde(rename = "timeStamp")]
    timestamp: String,
    value: String,
    from: String,
    to: String,
    #[serde(rename = "isError")]
    is_error: String,
}

pub async fn fetch_eth_wallet(client: &Client, address: &str, api_key: &str) -> Result<WalletInfo> {
    let base = "https://api.etherscan.io/api";

    // Balance
    let bal_url = format!(
        "{}?module=account&action=balance&address={}&tag=latest&apikey={}",
        base, address, api_key
    );
    let bal_resp: EthBalanceResponse = client.get(&bal_url).send().await?.json().await
        .map_err(|e| anyhow!("ETH balance parse error: {}", e))?;
    if bal_resp.status != "1" {
        return Err(anyhow!("Etherscan balance error for {}", address));
    }
    let balance_wei: u128 = bal_resp.result.parse().unwrap_or(0);
    let balance = balance_wei as f64 / 1e18;

    // TX list (last 100)
    let tx_url = format!(
        "{}?module=account&action=txlist&address={}&startblock=0&endblock=99999999&page=1&offset=100&sort=asc&apikey={}",
        base, address, api_key
    );
    let tx_resp: EthTxListResponse = client.get(&tx_url).send().await?.json().await
        .map_err(|e| anyhow!("ETH tx parse error: {}", e))?;

    let raw_txs: Vec<EthTx> = if tx_resp.status == "1" {
        serde_json::from_value(tx_resp.result).unwrap_or_default()
    } else {
        vec![]
    };

    let mut total_received: f64 = 0.0;
    let mut total_sent: f64 = 0.0;

    let mut transactions: Vec<Transaction> = raw_txs
        .iter()
        .filter(|tx| tx.is_error == "0")
        .map(|tx| {
            let ts: i64 = tx.timestamp.parse().unwrap_or(0);
            let date = Utc.timestamp_opt(ts, 0).single();

            let value_wei: u128 = tx.value.parse().unwrap_or(0);
            let value_eth = value_wei as f64 / 1e18;

            let addr_lower = address.to_lowercase();
            let is_incoming = tx.to.to_lowercase() == addr_lower;

            let (net_eth, counterparty) = if is_incoming {
                total_received += value_eth;
                (value_eth, Some(tx.from.clone()))
            } else {
                total_sent += value_eth;
                (-value_eth, Some(tx.to.clone()))
            };

            let sign = if net_eth >= 0.0 { "+" } else { "" };
            let amount_display = format!("{}{:.6} ETH", sign, net_eth);

            Transaction {
                txid: tx.hash.clone(),
                date,
                amount_satoshis: (net_eth * 1e9) as i64, // gwei for sorting
                amount_display,
                counterparty,
                chain: Chain::ETH,
            }
        })
        .collect();

    transactions.sort_by(|a, b| a.date.cmp(&b.date));

    let first_seen = transactions.iter().filter_map(|t| t.date).min();
    let last_seen = transactions.iter().filter_map(|t| t.date).max();
    let tx_count = transactions.len() as u64;

    Ok(WalletInfo {
        address: address.to_string(),
        chain: Chain::ETH,
        balance,
        balance_display: format!("{:.6} ETH", balance),
        total_received,
        total_sent,
        tx_count,
        first_seen,
        last_seen,
        transactions,
    })
}

// ─── TRON USDT structs ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TronAccountResponse {
    #[serde(rename = "trc20TokenBalances")]
    trc20_token_balances: Option<Vec<TronTokenBalance>>,
}

#[derive(Debug, Deserialize)]
struct TronTokenBalance {
    #[serde(rename = "tokenId")]
    token_id: String,
    balance: String,
}

#[derive(Debug, Deserialize)]
struct TronGridResponse {
    data: Vec<TronGridData>,
}

#[derive(Debug, Deserialize)]
struct TronGridData {
    trc20: Option<Vec<std::collections::HashMap<String, String>>>,
}

#[derive(Debug, Deserialize)]
struct TronTxResponse {
    data: Option<Vec<TronTx>>,
}

#[derive(Debug, Deserialize)]
struct TronTx {
    hash: String,
    timestamp: i64,
    #[serde(rename = "ownerAddress")]
    owner_address: String,
    #[serde(rename = "toAddress")]
    to_address: Option<String>,
    amount: String,
    #[serde(rename = "contractData")]
    contract_data: Option<TronContractData>,
}

#[derive(Debug, Deserialize)]
struct TronContractData {
    amount: Option<serde_json::Value>,
}

pub async fn fetch_usdt_tron_wallet(client: &Client, address: &str) -> Result<WalletInfo> {
    let mut balance = 0.0;
    // 1. Try accountv2 (Tronscan)
    let info_url = format!("https://apilist.tronscanapi.com/api/accountv2?address={}", address);
    if let Ok(resp) = client.get(&info_url).send().await {
        if resp.status().is_success() {
            if let Ok(info) = resp.json::<TronAccountResponse>().await {
                if let Some(balances) = info.trc20_token_balances {
                    for token in balances {
                        if token.token_id == "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t" {
                            let raw_bal: f64 = token.balance.parse().unwrap_or(0.0);
                            balance = raw_bal / 1e6;
                        }
                    }
                }
            }
        } else if resp.status().as_u16() == 401 {
            // Fallback to TronGrid if unauthorized
            let grid_url = format!("https://api.trongrid.io/v1/accounts/{}", address);
            if let Ok(grid_resp) = client.get(&grid_url).send().await {
                if grid_resp.status().is_success() {
                    if let Ok(grid_info) = grid_resp.json::<TronGridResponse>().await {
                        if let Some(first_data) = grid_info.data.first() {
                            if let Some(trc20_list) = &first_data.trc20 {
                                for token_map in trc20_list {
                                    if let Some(bal_str) = token_map.get("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t") {
                                        let raw_bal: f64 = bal_str.parse().unwrap_or(0.0);
                                        balance = raw_bal / 1e6;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Fetch USDT TRC-20 transactions
    let txs_url = format!(
        "https://apilist.tronscanapi.com/api/transaction?address={}&trc20Id=TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t",
        address
    );
    let tx_resp: TronTxResponse = client
        .get(&txs_url)
        .send()
        .await?
        .json()
        .await
        .map_err(|e| anyhow!("Tron tx parse error: {}", e))?;

    let raw_txs = tx_resp.data.unwrap_or_default();
    let mut total_received = 0.0;
    let mut total_sent = 0.0;

    let mut transactions: Vec<Transaction> = raw_txs
        .iter()
        .map(|tx| {
            let date = Utc.timestamp_opt(tx.timestamp / 1000, 0).single();

            let raw_amount = tx.contract_data.as_ref()
                .and_then(|cd| cd.amount.as_ref())
                .and_then(|val| {
                    if let Some(num) = val.as_f64() {
                        Some(num)
                    } else if let Some(s) = val.as_str() {
                        s.parse::<f64>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| tx.amount.parse::<f64>().unwrap_or(0.0));

            let val_usdt = raw_amount / 1e6;

            let addr_lower = address.to_lowercase();
            let is_incoming = tx.owner_address.to_lowercase() != addr_lower;

            let (net_usdt, counterparty) = if is_incoming {
                total_received += val_usdt;
                (val_usdt, Some(tx.owner_address.clone()))
            } else {
                total_sent += val_usdt;
                (-val_usdt, tx.to_address.clone())
            };

            let sign = if net_usdt >= 0.0 { "+" } else { "" };
            let amount_display = format!("{}{:.2} USDT", sign, net_usdt);

            Transaction {
                txid: tx.hash.clone(),
                date,
                amount_satoshis: (net_usdt * 1e6) as i64,
                amount_display,
                counterparty,
                chain: Chain::USDT_TRON,
            }
        })
        .collect();

    transactions.sort_by(|a, b| a.date.cmp(&b.date));

    let first_seen = transactions.iter().filter_map(|t| t.date).min();
    let last_seen = transactions.iter().filter_map(|t| t.date).max();
    let tx_count = transactions.len() as u64;

    Ok(WalletInfo {
        address: address.to_string(),
        chain: Chain::USDT_TRON,
        balance,
        balance_display: format!("{:.2} USDT", balance),
        total_received,
        total_sent,
        tx_count,
        first_seen,
        last_seen,
        transactions,
    })
}

// ─── ETH / BNB ERC-20 USDT structs ───────────────────────────────────────────

pub async fn fetch_usdt_eth_wallet(client: &Client, address: &str, api_key: &str) -> Result<WalletInfo> {
    let base = "https://api.etherscan.io/api";

    // USDT Balance
    let bal_url = format!(
        "{}?module=account&action=tokenbalance&contractaddress=0xdAC17F958D2ee523a2206206994597C13D831ec7&address={}&tag=latest&apikey={}",
        base, address, api_key
    );
    let bal_resp: EthBalanceResponse = client.get(&bal_url).send().await?.json().await
        .map_err(|e| anyhow!("USDT ERC-20 balance parse error: {}", e))?;
    if bal_resp.status != "1" {
        return Err(anyhow!("Etherscan USDT balance error for {}", address));
    }
    let balance_raw: f64 = bal_resp.result.parse().unwrap_or(0.0);
    let balance = balance_raw / 1e6; // USDT has 6 decimals

    // USDT Transfers list
    let tx_url = format!(
        "{}?module=account&action=tokentx&contractaddress=0xdAC17F958D2ee523a2206206994597C13D831ec7&address={}&page=1&offset=100&sort=asc&apikey={}",
        base, address, api_key
    );
    let tx_resp: EthTxListResponse = client.get(&tx_url).send().await?.json().await
        .map_err(|e| anyhow!("USDT ETH tx parse error: {}", e))?;

    let raw_txs: Vec<EthTx> = if tx_resp.status == "1" {
        serde_json::from_value(tx_resp.result).unwrap_or_default()
    } else {
        vec![]
    };

    let mut total_received = 0.0;
    let mut total_sent = 0.0;

    let mut transactions: Vec<Transaction> = raw_txs
        .iter()
        .map(|tx| {
            let ts: i64 = tx.timestamp.parse().unwrap_or(0);
            let date = Utc.timestamp_opt(ts, 0).single();

            let val_raw: f64 = tx.value.parse().unwrap_or(0.0);
            let val_usdt = val_raw / 1e6;

            let addr_lower = address.to_lowercase();
            let is_incoming = tx.to.to_lowercase() == addr_lower;

            let (net_usdt, counterparty) = if is_incoming {
                total_received += val_usdt;
                (val_usdt, Some(tx.from.clone()))
            } else {
                total_sent += val_usdt;
                (-val_usdt, Some(tx.to.clone()))
            };

            let sign = if net_usdt >= 0.0 { "+" } else { "" };
            let amount_display = format!("{}{:.2} USDT", sign, net_usdt);

            Transaction {
                txid: tx.hash.clone(),
                date,
                amount_satoshis: (net_usdt * 1e6) as i64,
                amount_display,
                counterparty,
                chain: Chain::USDT_ETH,
            }
        })
        .collect();

    transactions.sort_by(|a, b| a.date.cmp(&b.date));

    let first_seen = transactions.iter().filter_map(|t| t.date).min();
    let last_seen = transactions.iter().filter_map(|t| t.date).max();
    let tx_count = transactions.len() as u64;

    Ok(WalletInfo {
        address: address.to_string(),
        chain: Chain::USDT_ETH,
        balance,
        balance_display: format!("{:.2} USDT", balance),
        total_received,
        total_sent,
        tx_count,
        first_seen,
        last_seen,
        transactions,
    })
}

// ─── Solana structs ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct SolRpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct SolBalanceResponse {
    result: Option<SolBalanceResult>,
}

#[derive(Debug, Deserialize)]
struct SolBalanceResult {
    value: u64,
}

#[derive(Debug, Deserialize)]
struct SolSignaturesResponse {
    result: Option<Vec<SolSignatureInfo>>,
}

#[derive(Debug, Deserialize)]
struct SolSignatureInfo {
    signature: String,
    #[serde(rename = "blockTime")]
    block_time: Option<i64>,
}

pub async fn fetch_sol_wallet(client: &Client, address: &str) -> Result<WalletInfo> {
    let rpc_url = "https://api.mainnet-beta.solana.com";

    // 1. Get Balance
    let bal_payload = SolRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "getBalance",
        params: serde_json::json!([address]),
    };

    let bal_res: SolBalanceResponse = client
        .post(rpc_url)
        .json(&bal_payload)
        .send()
        .await?
        .json()
        .await
        .map_err(|e| anyhow!("SOL balance RPC error: {}", e))?;

    let balance_lamports = bal_res.result.map(|r| r.value).unwrap_or(0);
    let balance = balance_lamports as f64 / 1e9;

    // 2. Get Signatures
    let sig_payload = SolRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "getSignaturesForAddress",
        params: serde_json::json!([address, { "limit": 25 }]),
    };

    let sig_res: SolSignaturesResponse = client
        .post(rpc_url)
        .json(&sig_payload)
        .send()
        .await?
        .json()
        .await
        .map_err(|e| anyhow!("SOL signatures RPC error: {}", e))?;

    let raw_sigs = sig_res.result.unwrap_or_default();

    let mut transactions: Vec<Transaction> = raw_sigs
        .iter()
        .map(|sig| {
            let date = sig.block_time.and_then(|t| Utc.timestamp_opt(t, 0).single());
            Transaction {
                txid: sig.signature.clone(),
                date,
                amount_satoshis: 0,
                amount_display: "SOL Tx".to_string(),
                counterparty: None,
                chain: Chain::SOL,
            }
        })
        .collect();

    transactions.sort_by(|a, b| a.date.cmp(&b.date));

    let first_seen = transactions.iter().filter_map(|t| t.date).min();
    let last_seen = transactions.iter().filter_map(|t| t.date).max();
    let tx_count = transactions.len() as u64;

    Ok(WalletInfo {
        address: address.to_string(),
        chain: Chain::SOL,
        balance,
        balance_display: format!("{:.4} SOL", balance),
        total_received: 0.0,
        total_sent: 0.0,
        tx_count,
        first_seen,
        last_seen,
        transactions,
    })
}

// ─── BNB structs ─────────────────────────────────────────────────────────────

pub async fn fetch_bnb_wallet(client: &Client, address: &str, api_key: &str) -> Result<WalletInfo> {
    let base = "https://api.bscscan.com/api";

    // Balance
    let bal_url = format!(
        "{}?module=account&action=balance&address={}&tag=latest&apikey={}",
        base, address, api_key
    );
    let bal_resp: EthBalanceResponse = client.get(&bal_url).send().await?.json().await
        .map_err(|e| anyhow!("BNB balance parse error: {}", e))?;
    if bal_resp.status != "1" {
        return Err(anyhow!("BSCscan balance error for {}", address));
    }
    let balance_wei: u128 = bal_resp.result.parse().unwrap_or(0);
    let balance = balance_wei as f64 / 1e18;

    // TX list (last 100)
    let tx_url = format!(
        "{}?module=account&action=txlist&address={}&startblock=0&endblock=99999999&page=1&offset=100&sort=asc&apikey={}",
        base, address, api_key
    );
    let tx_resp: EthTxListResponse = client.get(&tx_url).send().await?.json().await
        .map_err(|e| anyhow!("BNB tx parse error: {}", e))?;

    let raw_txs: Vec<EthTx> = if tx_resp.status == "1" {
        serde_json::from_value(tx_resp.result).unwrap_or_default()
    } else {
        vec![]
    };

    let mut total_received: f64 = 0.0;
    let mut total_sent: f64 = 0.0;

    let mut transactions: Vec<Transaction> = raw_txs
        .iter()
        .filter(|tx| tx.is_error == "0")
        .map(|tx| {
            let ts: i64 = tx.timestamp.parse().unwrap_or(0);
            let date = Utc.timestamp_opt(ts, 0).single();

            let value_wei: u128 = tx.value.parse().unwrap_or(0);
            let value_bnb = value_wei as f64 / 1e18;

            let addr_lower = address.to_lowercase();
            let is_incoming = tx.to.to_lowercase() == addr_lower;

            let (net_bnb, counterparty) = if is_incoming {
                total_received += value_bnb;
                (value_bnb, Some(tx.from.clone()))
            } else {
                total_sent += value_bnb;
                (-value_bnb, Some(tx.to.clone()))
            };

            let sign = if net_bnb >= 0.0 { "+" } else { "" };
            let amount_display = format!("{}{:.6} BNB", sign, net_bnb);

            Transaction {
                txid: tx.hash.clone(),
                date,
                amount_satoshis: (net_bnb * 1e9) as i64, // gwei for sorting
                amount_display,
                counterparty,
                chain: Chain::BNB,
            }
        })
        .collect();

    transactions.sort_by(|a, b| a.date.cmp(&b.date));

    let first_seen = transactions.iter().filter_map(|t| t.date).min();
    let last_seen = transactions.iter().filter_map(|t| t.date).max();
    let tx_count = transactions.len() as u64;

    Ok(WalletInfo {
        address: address.to_string(),
        chain: Chain::BNB,
        balance,
        balance_display: format!("{:.6} BNB", balance),
        total_received,
        total_sent,
        tx_count,
        first_seen,
        last_seen,
        transactions,
    })
}

/// Detect chain type by address format
#[allow(dead_code)]
pub fn detect_chain(address: &str) -> Option<Chain> {
    let addr = address.trim();
    let addr_lower = addr.to_lowercase();
    if addr_lower.starts_with("0x") && addr_lower.len() == 42 {
        Some(Chain::ETH)
    } else if addr_lower.starts_with('1') || addr_lower.starts_with('3') || addr_lower.starts_with("bc1") {
        Some(Chain::BTC)
    } else if addr.starts_with('T') && addr.len() == 34 {
        Some(Chain::USDT_TRON)
    } else if addr.len() >= 43 && addr.len() <= 44 && is_base58(addr) {
        Some(Chain::SOL)
    } else {
        None
    }
}

pub fn detect_chains(address: &str) -> Vec<Chain> {
    let addr = address.trim();
    let addr_lower = addr.to_lowercase();
    if addr_lower.starts_with('1') || addr_lower.starts_with('3') || addr_lower.starts_with("bc1") {
        vec![Chain::BTC]
    } else if addr_lower.starts_with("0x") && addr_lower.len() == 42 {
        vec![Chain::ETH, Chain::BNB]
    } else if addr.starts_with('T') && addr.len() == 34 {
        vec![Chain::USDT_TRON]
    } else if addr.len() >= 43 && addr.len() <= 44 && is_base58(addr) {
        vec![Chain::SOL]
    } else {
        vec![]
    }
}

fn is_base58(s: &str) -> bool {
    s.chars().all(|c| {
        c.is_ascii_alphanumeric() && c != '0' && c != 'O' && c != 'I' && c != 'l'
    })
}
