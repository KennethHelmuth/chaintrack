use anyhow::Result;
use std::fs::File;
use std::io::Write;

use crate::types::{BatchResult, WalletInfo};

/// Export a single wallet result or batch to JSON
pub fn export_json(wallets: &[WalletInfo], linked: &[crate::types::LinkedPair]) -> Result<()> {
    let obj = serde_json::json!({
        "wallets": wallets,
        "linked_pairs": linked.iter().map(|lp| serde_json::json!({
            "wallet_a": lp.wallet_a,
            "wallet_b": lp.wallet_b,
            "shared_counterparty": lp.shared_counterparty
        })).collect::<Vec<_>>()
    });
    let mut f = File::create("output.json")?;
    f.write_all(serde_json::to_string_pretty(&obj)?.as_bytes())?;
    Ok(())
}

/// Export to CSV: one row per transaction across all wallets
pub fn export_csv(wallets: &[WalletInfo]) -> Result<()> {
    let mut wtr = csv::Writer::from_path("output.csv")?;
    wtr.write_record(["wallet", "chain", "txid", "date", "amount", "counterparty"])?;
    for wallet in wallets {
        for tx in &wallet.transactions {
            let date_str = tx
                .date
                .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "pending".to_string());
            wtr.write_record([
                &wallet.address,
                &wallet.chain.to_string(),
                &tx.txid,
                &date_str,
                &tx.amount_display,
                tx.counterparty.as_deref().unwrap_or("-"),
            ])?;
        }
    }
    wtr.flush()?;
    Ok(())
}

pub fn export_batch(batch: &BatchResult) -> Result<()> {
    export_json(&batch.wallets, &batch.linked_pairs)?;
    export_csv(&batch.wallets)?;
    Ok(())
}
