use std::collections::{HashMap, HashSet};

use crate::types::{BatchResult, LinkedPair, WalletInfo};

/// Detect linked wallets by shared counterparty addresses
pub fn detect_links(wallets: &[WalletInfo]) -> Vec<LinkedPair> {
    // Map: counterparty_address -> list of wallet addresses that interacted with it
    let mut counterparty_map: HashMap<String, Vec<String>> = HashMap::new();

    for wallet in wallets {
        for tx in &wallet.transactions {
            if let Some(cp) = &tx.counterparty {
                // Don't link wallets in the batch to themselves
                let is_batch_wallet = wallets.iter().any(|w| &w.address == cp);
                if !is_batch_wallet {
                    counterparty_map
                        .entry(cp.clone())
                        .or_default()
                        .push(wallet.address.clone());
                }
            }
        }
    }

    let mut pairs: Vec<LinkedPair> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for (counterparty, wallet_addrs) in &counterparty_map {
        if wallet_addrs.len() < 2 {
            continue;
        }
        // Deduplicate
        let unique: Vec<&String> = {
            let mut u: Vec<&String> = wallet_addrs.iter().collect();
            u.sort();
            u.dedup();
            u
        };
        for i in 0..unique.len() {
            for j in (i + 1)..unique.len() {
                let a = unique[i].clone();
                let b = unique[j].clone();
                let key = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                if seen.insert(key) {
                    pairs.push(LinkedPair {
                        wallet_a: a,
                        wallet_b: b,
                        shared_counterparty: counterparty.clone(),
                    });
                }
            }
        }
    }

    pairs
}

pub fn build_batch_result(wallets: Vec<WalletInfo>) -> BatchResult {
    let linked_pairs = detect_links(&wallets);
    BatchResult {
        wallets,
        linked_pairs,
    }
}
