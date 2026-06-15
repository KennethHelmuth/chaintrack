use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub txid: String,
    pub date: Option<DateTime<Utc>>,
    pub amount_satoshis: i64,   // for BTC; in wei for ETH (scaled)
    pub amount_display: String, // human-readable
    pub counterparty: Option<String>,
    pub chain: Chain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Chain {
    BTC,
    ETH,
    USDT_TRON,
    USDT_ETH,
    SOL,
    BNB,
}

impl std::fmt::Display for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Chain::BTC => write!(f, "BTC"),
            Chain::ETH => write!(f, "ETH"),
            Chain::USDT_TRON => write!(f, "USDT (Tron)"),
            Chain::USDT_ETH => write!(f, "USDT (ETH)"),
            Chain::SOL => write!(f, "SOL"),
            Chain::BNB => write!(f, "BNB"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub address: String,
    pub chain: Chain,
    pub balance: f64,
    pub balance_display: String,
    pub total_received: f64,
    pub total_sent: f64,
    pub tx_count: u64,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone)]
pub struct LinkedPair {
    pub wallet_a: String,
    pub wallet_b: String,
    pub shared_counterparty: String,
}

#[derive(Debug, Clone)]
pub struct BatchResult {
    pub wallets: Vec<WalletInfo>,
    pub linked_pairs: Vec<LinkedPair>,
}
