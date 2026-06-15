# ChainTrack — Blockchain Wallet Intelligence CLI

[![Rust](https://img.shields.io/badge/Language-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License MIT](https://img.shields.io/badge/License-MIT-blue)](LICENSE)
[![TLP:WHITE](https://img.shields.io/badge/TLP-WHITE-white?labelColor=grey)](https://www.first.org/tlp/)

## Overview

A terminal-based interactive blockchain wallet analysis tool built for threat intelligence analysts and OSINT researchers. Supports multi-chain wallet lookup, transaction history, batch analysis, and CSV/JSON export. Built with Rust and **ratatui**.

---

## Features

- **Interactive TUI**: Easy arrow key navigation, scrollable transaction tables, and clear visual prompts.
- **BTC Wallet Lookup**: Retrieves native balance, totals, and transaction history via Blockstream.info API (no API key required).
- **ETH Wallet Lookup**: Retrieves native balance and transaction history via Etherscan API.
- **USDT (Tron/TRC-20) Lookup**: Retrieves USDT token balance and transaction history via Tronscan API (no API key required).
- **USDT (ETH/ERC-20) Lookup**: Retrieves USDT token transfers and balance using Etherscan API.
- **SOL Wallet Lookup**: Retrieves native balance and transaction history via Solana public RPC (no API key required).
- **BNB Wallet Lookup**: Retrieves native balance and transaction history via BSCScan API.
- **Batch Analysis**: Parses multiple addresses concurrently, auto-detects the blockchain type per address, and combines the results.
- **Linked Wallet Detection**: Automatically cross-references and flags wallets that share common counterparties in their transaction histories.
- **Data Exporting**: Saves analysis reports to `output.json` and `output.csv` in the current directory.

---

## Installation

Ensure you have Rust and Cargo installed. Then run:

```bash
git clone https://github.com/KennethHelmuth/chaintrack
cd chaintrack
cargo build --release
./target/release/chaintrack
```

---

## Environment Variables

The application reads Etherscan and BSCScan API keys from environment variables. Set them in your shell before running:

```bash
export ETHERSCAN_KEY="your_etherscan_key_here"  # Free at https://etherscan.io/apis
export BSCSCAN_KEY="your_bscscan_key_here"      # Free at https://bscscan.com/apis
```

*Note: BTC, USDT (Tron), and SOL lookups do not require any API credentials.*

---

## Auto Chain Detection (Batch Mode)

In batch lookup mode, addresses are automatically categorized by their format:

- **BTC**: Starts with `1`, `3`, or `bc1`
- **ETH / BNB**: Starts with `0x` (analyzed concurrently across both networks)
- **Tron**: Starts with `T`
- **Solana**: Base58 encoded address, 43–44 characters

---

## Use Cases

- **Cryptocurrency Forensic Investigations**: Track wallets involved in fraud, scams, ransomware, or laundering operations.
- **Mapping Threat Infrastructure**: Outline threat actors' financial cash-out pipelines and network footprints.
- **Linked Wallet Discovery**: Find overlaps and associations between disparate addresses using shared counterparty heuristics.
- **IOC Report Generation**: Export parsed wallet information and transactional lists directly into threat intelligence platforms.

---

## Part of the Macs-Hit CTI Toolkit

Built by **Kenneth Helmuth** — [macs-hit.github.io](https://kennethhelmuth.github.io/macs-hit.github.io/) | [Medium](https://medium.com/@Real-macs_hit)

---

## Disclaimer

For research and defensive purposes only. TLP:WHITE.
