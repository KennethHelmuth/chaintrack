use crate::{
    analysis::build_batch_result,
    api::{
        detect_chains, fetch_btc_wallet, fetch_eth_wallet, fetch_usdt_tron_wallet,
        fetch_usdt_eth_wallet, fetch_sol_wallet, fetch_bnb_wallet,
    },
    export::export_batch,
    types::{BatchResult, Chain, WalletInfo},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
    Frame,
};
use reqwest::Client;
use tokio::sync::mpsc::UnboundedSender;

// ─── Colour Palette ──────────────────────────────────────────────────────────

const CYAN: Color = Color::Cyan;
const DARK_CYAN: Color = Color::Cyan;
const BG: Color = Color::Reset;
const SURFACE: Color = Color::Reset;
const SURFACE2: Color = Color::Reset;
const TEXT: Color = Color::White;
const DIM: Color = Color::DarkGray;
const GREEN: Color = Color::Green;
const RED: Color = Color::Red;
const AMBER: Color = Color::Yellow;
const PURPLE: Color = Color::Magenta;

// ─── App State ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    MainMenu,
    BtcInput,
    EthInput,
    UsdtTronInput,
    UsdtEthInput,
    SolInput,
    BnbInput,
    BatchInput,
    Loading,
    WalletResult,
    BatchResult,
    ExportConfirm,
    Error,
}

pub enum AppMessage {
    WalletLoaded(WalletInfo),
    BatchLoaded(BatchResult),
    FetchError(String),
    ExportDone(String),
}

pub struct App {
    pub should_quit: bool,
    pub screen: Screen,

    // Menu
    pub menu_state: ListState,
    pub menu_items: Vec<&'static str>,

    // Input
    pub input_buffer: String,
    pub input_cursor: usize,

    // Results
    pub current_wallet: Option<WalletInfo>,
    pub current_batch: Option<BatchResult>,
    pub table_state: TableState,
    pub scroll_offset: usize,

    // Status
    pub status_msg: String,
    pub error_msg: String,

    // HTTP client
    pub client: Client,

    // Async sender
    pub tx: UnboundedSender<AppMessage>,

    // Etherscan API key
    pub eth_key: String,

    // BSCscan API key
    pub bsc_key: String,

    // Export tracking
    pub last_export_msg: Option<String>,
}

impl App {
    pub fn new(tx: UnboundedSender<AppMessage>) -> Self {
        let mut menu_state = ListState::default();
        menu_state.select(Some(0));

        let eth_key = std::env::var("ETHERSCAN_KEY").unwrap_or_default();
        let bsc_key = std::env::var("BSCSCAN_KEY").unwrap_or_default();

        Self {
            should_quit: false,
            screen: Screen::MainMenu,
            menu_state,
            menu_items: vec![
                "  ₿  BTC Wallet Lookup",
                "  Ξ  ETH Wallet Lookup",
                "  ₮  USDT (Tron) Lookup",
                "  ♦  USDT (ETH) Lookup",
                "  ◎  SOL Wallet Lookup",
                "  ❖  BNB Wallet Lookup",
                "  ⋮  Batch Analysis",
                "  ↓  Export Last Results",
                "  ×  Quit",
            ],
            input_buffer: String::new(),
            input_cursor: 0,
            current_wallet: None,
            current_batch: None,
            table_state: TableState::default(),
            scroll_offset: 0,
            status_msg: "Welcome to ChainTrack — Use ↑↓ to navigate, Enter to select".to_string(),
            error_msg: String::new(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap(),
            tx,
            eth_key,
            bsc_key,
            last_export_msg: None,
        }
    }

    pub fn handle_message(&mut self, msg: AppMessage) {
        match msg {
            AppMessage::WalletLoaded(wallet) => {
                self.status_msg = format!(
                    "Loaded {} — {} txs",
                    wallet.address,
                    wallet.tx_count
                );
                self.current_wallet = Some(wallet);
                self.current_batch = None;
                self.table_state = TableState::default();
                self.table_state.select(Some(0));
                self.scroll_offset = 0;
                self.screen = Screen::WalletResult;
            }
            AppMessage::BatchLoaded(batch) => {
                self.status_msg = format!(
                    "Batch complete — {} wallets, {} linked pairs",
                    batch.wallets.len(),
                    batch.linked_pairs.len()
                );
                self.current_batch = Some(batch);
                self.current_wallet = None;
                self.table_state = TableState::default();
                self.table_state.select(Some(0));
                self.scroll_offset = 0;
                self.screen = Screen::BatchResult;
            }
            AppMessage::FetchError(e) => {
                self.error_msg = e;
                self.screen = Screen::Error;
            }
            AppMessage::ExportDone(msg) => {
                self.last_export_msg = Some(msg.clone());
                self.status_msg = msg;
                self.screen = Screen::ExportConfirm;
            }
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) {
        match self.screen {
            Screen::MainMenu => self.handle_menu_key(key).await,
            Screen::BtcInput | Screen::EthInput | Screen::UsdtTronInput | Screen::UsdtEthInput | Screen::SolInput | Screen::BnbInput | Screen::BatchInput => {
                self.handle_input_key(key).await
            }
            Screen::WalletResult => self.handle_result_key(key),
            Screen::BatchResult => self.handle_result_key(key),
            Screen::Loading => {} // ignore input while loading
            Screen::Error => {
                if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                    self.screen = Screen::MainMenu;
                    self.status_msg =
                        "Welcome to ChainTrack — Use ↑↓ to navigate, Enter to select".to_string();
                }
            }
            Screen::ExportConfirm => {
                if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                    self.screen = Screen::MainMenu;
                    self.status_msg =
                        "Welcome to ChainTrack — Use ↑↓ to navigate, Enter to select".to_string();
                }
            }
        }
    }

    async fn handle_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.menu_state.selected().unwrap_or(0);
                self.menu_state
                    .select(Some(if i == 0 { self.menu_items.len() - 1 } else { i - 1 }));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.menu_state.selected().unwrap_or(0);
                self.menu_state
                    .select(Some((i + 1) % self.menu_items.len()));
            }
            KeyCode::Enter => {
                match self.menu_state.selected() {
                    Some(0) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        self.status_msg =
                            "Enter BTC wallet address and press Enter".to_string();
                        self.screen = Screen::BtcInput;
                    }
                    Some(1) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        if self.eth_key.is_empty() {
                            self.error_msg =
                                "ETHERSCAN_KEY environment variable is not set.\n\nSet it with:\n  export ETHERSCAN_KEY=your_key_here\n\nGet a free key at https://etherscan.io/apis"
                                    .to_string();
                            self.screen = Screen::Error;
                        } else {
                            self.status_msg =
                                "Enter ETH wallet address and press Enter".to_string();
                            self.screen = Screen::EthInput;
                        }
                    }
                    Some(2) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        self.status_msg =
                            "Enter Tron address to look up USDT (TRC-20)".to_string();
                        self.screen = Screen::UsdtTronInput;
                    }
                    Some(3) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        if self.eth_key.is_empty() {
                            self.error_msg =
                                "ETHERSCAN_KEY environment variable is not set.\n\nSet it with:\n  export ETHERSCAN_KEY=your_key_here"
                                    .to_string();
                            self.screen = Screen::Error;
                        } else {
                            self.status_msg =
                                "Enter Ethereum address to look up USDT (ERC-20)".to_string();
                            self.screen = Screen::UsdtEthInput;
                        }
                    }
                    Some(4) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        self.status_msg =
                            "Enter Solana address to look up SOL".to_string();
                        self.screen = Screen::SolInput;
                    }
                    Some(5) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        if self.bsc_key.is_empty() {
                            self.error_msg =
                                "BSCSCAN_KEY environment variable is not set.\n\nSet it with:\n  export BSCSCAN_KEY=your_key_here\n\nGet a free key at https://bscscan.com/apis"
                                    .to_string();
                            self.screen = Screen::Error;
                        } else {
                            self.status_msg =
                                "Enter BNB wallet address and press Enter".to_string();
                            self.screen = Screen::BnbInput;
                        }
                    }
                    Some(6) => {
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                        self.status_msg =
                            "Paste addresses (one per line), press Ctrl+D when done".to_string();
                        self.screen = Screen::BatchInput;
                    }
                    Some(7) => {
                        self.do_export().await;
                    }
                    Some(8) | _ => {
                        self.should_quit = true;
                    }
                }
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    async fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::MainMenu;
                self.status_msg =
                    "Welcome to ChainTrack — Use ↑↓ to navigate, Enter to select".to_string();
            }
            KeyCode::Enter => {
                let addr = self.input_buffer.trim().to_string();
                if addr.is_empty() {
                    return;
                }
                match self.screen {
                    Screen::BtcInput => {
                        self.start_btc_fetch(addr).await;
                    }
                    Screen::EthInput => {
                        self.start_eth_fetch(addr).await;
                    }
                    Screen::UsdtTronInput => {
                        self.start_usdt_tron_fetch(addr).await;
                    }
                    Screen::UsdtEthInput => {
                        self.start_usdt_eth_fetch(addr).await;
                    }
                    Screen::SolInput => {
                        self.start_sol_fetch(addr).await;
                    }
                    Screen::BnbInput => {
                        self.start_bnb_fetch(addr).await;
                    }
                    Screen::BatchInput => {
                        // In batch mode, Enter adds a newline
                        self.input_buffer.push('\n');
                        self.input_cursor = self.input_buffer.len();
                    }
                    _ => {}
                }
            }
            KeyCode::Char('d')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                // Ctrl+D submits batch
                if self.screen == Screen::BatchInput {
                    let text = self.input_buffer.trim().to_string();
                    if !text.is_empty() {
                        self.start_batch_fetch(text).await;
                    }
                }
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    let byte_idx = self.input_cursor - 1;
                    self.input_buffer.remove(byte_idx);
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input_buffer.len() {
                    self.input_buffer.remove(self.input_cursor);
                }
            }
            KeyCode::Left => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.input_cursor < self.input_buffer.len() {
                    self.input_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.input_cursor = 0;
            }
            KeyCode::End => {
                self.input_cursor = self.input_buffer.len();
            }
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.input_cursor, c);
                self.input_cursor += 1;
            }
            _ => {}
        }
    }

    fn handle_result_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.screen = Screen::MainMenu;
                self.status_msg =
                    "Welcome to ChainTrack — Use ↑↓ to navigate, Enter to select".to_string();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up();
            }
            KeyCode::PageDown => {
                for _ in 0..10 {
                    self.scroll_down();
                }
            }
            KeyCode::PageUp => {
                for _ in 0..10 {
                    self.scroll_up();
                }
            }
            _ => {}
        }
    }

    fn scroll_down(&mut self) {
        let max = self.max_rows();
        if self.scroll_offset + 1 < max {
            self.scroll_offset += 1;
            self.table_state.select(Some(self.scroll_offset));
        }
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            self.table_state.select(Some(self.scroll_offset));
        }
    }

    fn max_rows(&self) -> usize {
        if let Some(w) = &self.current_wallet {
            w.transactions.len()
        } else if let Some(b) = &self.current_batch {
            b.wallets.iter().map(|w| w.transactions.len()).sum()
        } else {
            0
        }
    }

    async fn start_btc_fetch(&mut self, addr: String) {
        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching BTC data for {}…", addr);
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_btc_wallet(&client, &addr).await {
                Ok(w) => { let _ = tx.send(AppMessage::WalletLoaded(w)); }
                Err(e) => { let _ = tx.send(AppMessage::FetchError(e.to_string())); }
            }
        });
    }

    async fn start_eth_fetch(&mut self, addr: String) {
        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching ETH data for {}…", addr);
        let client = self.client.clone();
        let tx = self.tx.clone();
        let key = self.eth_key.clone();
        tokio::spawn(async move {
            match fetch_eth_wallet(&client, &addr, &key).await {
                Ok(w) => { let _ = tx.send(AppMessage::WalletLoaded(w)); }
                Err(e) => { let _ = tx.send(AppMessage::FetchError(e.to_string())); }
            }
        });
    }

    async fn start_usdt_tron_fetch(&mut self, addr: String) {
        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching Tron USDT data for {}…", addr);
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_usdt_tron_wallet(&client, &addr).await {
                Ok(w) => { let _ = tx.send(AppMessage::WalletLoaded(w)); }
                Err(e) => { let _ = tx.send(AppMessage::FetchError(e.to_string())); }
            }
        });
    }

    async fn start_usdt_eth_fetch(&mut self, addr: String) {
        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching ETH USDT data for {}…", addr);
        let client = self.client.clone();
        let tx = self.tx.clone();
        let key = self.eth_key.clone();
        tokio::spawn(async move {
            match fetch_usdt_eth_wallet(&client, &addr, &key).await {
                Ok(w) => { let _ = tx.send(AppMessage::WalletLoaded(w)); }
                Err(e) => { let _ = tx.send(AppMessage::FetchError(e.to_string())); }
            }
        });
    }

    async fn start_sol_fetch(&mut self, addr: String) {
        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching SOL data for {}…", addr);
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match fetch_sol_wallet(&client, &addr).await {
                Ok(w) => { let _ = tx.send(AppMessage::WalletLoaded(w)); }
                Err(e) => { let _ = tx.send(AppMessage::FetchError(e.to_string())); }
            }
        });
    }

    async fn start_bnb_fetch(&mut self, addr: String) {
        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching BNB data for {}…", addr);
        let client = self.client.clone();
        let tx = self.tx.clone();
        let key = self.bsc_key.clone();
        tokio::spawn(async move {
            match fetch_bnb_wallet(&client, &addr, &key).await {
                Ok(w) => { let _ = tx.send(AppMessage::WalletLoaded(w)); }
                Err(e) => { let _ = tx.send(AppMessage::FetchError(e.to_string())); }
            }
        });
    }

    async fn start_batch_fetch(&mut self, text: String) {
        let addresses: Vec<String> = text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        if addresses.is_empty() {
            return;
        }

        self.screen = Screen::Loading;
        self.status_msg = format!("Fetching {} address list…", addresses.len());
        let client = self.client.clone();
        let tx = self.tx.clone();
        let key = self.eth_key.clone();
        let bsc_key_clone = self.bsc_key.clone();

        tokio::spawn(async move {
            let mut wallets: Vec<WalletInfo> = Vec::new();
            let mut errors: Vec<String> = Vec::new();

            for addr in &addresses {
                let detected = detect_chains(addr);
                if detected.is_empty() {
                    errors.push(format!("{}: unknown chain", addr));
                    continue;
                }
                for chain in detected {
                    match chain {
                        Chain::BTC => match fetch_btc_wallet(&client, addr).await {
                            Ok(w) => wallets.push(w),
                            Err(e) => errors.push(format!("{}: {}", addr, e)),
                        },
                        Chain::ETH => {
                            if key.is_empty() {
                                errors.push(format!("{}: ETHERSCAN_KEY not set", addr));
                            } else {
                                match fetch_eth_wallet(&client, addr, &key).await {
                                    Ok(w) => wallets.push(w),
                                    Err(e) => errors.push(format!("{}: {}", addr, e)),
                                }
                            }
                        },
                        Chain::USDT_TRON => match fetch_usdt_tron_wallet(&client, addr).await {
                            Ok(w) => wallets.push(w),
                            Err(e) => errors.push(format!("{}: {}", addr, e)),
                        },
                        Chain::USDT_ETH => {
                            if key.is_empty() {
                                errors.push(format!("{}: ETHERSCAN_KEY not set", addr));
                            } else {
                                match fetch_usdt_eth_wallet(&client, addr, &key).await {
                                    Ok(w) => wallets.push(w),
                                    Err(e) => errors.push(format!("{}: {}", addr, e)),
                                }
                            }
                        },
                        Chain::SOL => match fetch_sol_wallet(&client, addr).await {
                            Ok(w) => wallets.push(w),
                            Err(e) => errors.push(format!("{}: {}", addr, e)),
                        },
                        Chain::BNB => {
                            if bsc_key_clone.is_empty() {
                                errors.push(format!("{}: BSCSCAN_KEY not set", addr));
                            } else {
                                match fetch_bnb_wallet(&client, addr, &bsc_key_clone).await {
                                    Ok(w) => wallets.push(w),
                                    Err(e) => errors.push(format!("{}: {}", addr, e)),
                                }
                            }
                        }
                    }
                }
            }

            if wallets.is_empty() {
                let _ = tx.send(AppMessage::FetchError(format!(
                    "All lookups failed:\n{}",
                    errors.join("\n")
                )));
            } else {
                let batch = build_batch_result(wallets);
                let _ = tx.send(AppMessage::BatchLoaded(batch));
            }
        });
    }

    async fn do_export(&mut self) {
        let wallets: Vec<WalletInfo>;

        if let Some(batch) = &self.current_batch {
            wallets = batch.wallets.clone();
        } else if let Some(w) = &self.current_wallet {
            wallets = vec![w.clone()];
        } else {
            self.error_msg = "No results to export yet. Run a lookup first.".to_string();
            self.screen = Screen::Error;
            return;
        }

        let tx = self.tx.clone();
        tokio::spawn(async move {
            let batch = build_batch_result(wallets);
            match export_batch(&batch) {
                Ok(_) => {
                    let _ = tx.send(AppMessage::ExportDone(
                        "Exported to output.json and output.csv in current directory".to_string(),
                    ));
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FetchError(format!("Export failed: {}", e)));
                }
            }
        });
    }
}

// ─── Drawing ─────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    // Clear screen first in the buffer to prevent stale character artifacts
    f.render_widget(Clear, f.area());

    // Background
    let bg = Block::default().style(Style::default().bg(BG));
    f.render_widget(bg, f.area());

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // body
            Constraint::Length(3), // status bar
        ])
        .split(area);

    draw_header(f, chunks[0]);
    draw_body(f, app, chunks[1]);
    draw_status(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, area: ratatui::layout::Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled("⛓  ", Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD)),
        Span::styled(
            "CHAIN",
            Style::default()
                .fg(CYAN)
                .bg(SURFACE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "TRACK",
            Style::default()
                .fg(TEXT)
                .bg(SURFACE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  •  Blockchain Wallet Intelligence",
            Style::default().fg(DIM).bg(SURFACE),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DARK_CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .alignment(Alignment::Center);
    f.render_widget(title, area);
}

fn draw_body(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    match app.screen {
        Screen::MainMenu => draw_main_menu(f, app, area),
        Screen::BtcInput => draw_input(f, app, area, "BTC", "₿"),
        Screen::EthInput => draw_input(f, app, area, "ETH", "Ξ"),
        Screen::UsdtTronInput => draw_input(f, app, area, "USDT (Tron)", "₮"),
        Screen::UsdtEthInput => draw_input(f, app, area, "USDT (ETH)", "♦"),
        Screen::SolInput => draw_input(f, app, area, "SOL", "◎"),
        Screen::BnbInput => draw_input(f, app, area, "BNB", "❖"),
        Screen::BatchInput => draw_batch_input(f, app, area),
        Screen::Loading => draw_loading(f, app, area),
        Screen::WalletResult => draw_wallet_result(f, app, area),
        Screen::BatchResult => draw_batch_result(f, app, area),
        Screen::ExportConfirm => draw_export_confirm(f, app, area),
        Screen::Error => draw_error(f, app, area),
    }
}

fn draw_main_menu(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(area);

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Min(0),
            Constraint::Percentage(15),
        ])
        .split(chunks[1]);

    let items: Vec<ListItem> = app
        .menu_items
        .iter()
        .enumerate()
        .map(|(i, &label)| {
            let selected = app.menu_state.selected() == Some(i);
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT).bg(SURFACE)
            };
            ListItem::new(Line::from(Span::styled(
                format!(" {} ", label),
                style,
            )))
        })
        .collect();

    let mut state = app.menu_state.clone();
    let list = List::new(items)
        .block(
            Block::default()
                .title(Span::styled(
                    " ◈ Main Menu ",
                    Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(CYAN).bg(SURFACE))
                .style(Style::default().bg(SURFACE).fg(TEXT)),
        )
        .highlight_symbol("▶ ")
        .highlight_style(Style::default().fg(Color::Black).bg(CYAN).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, vert[1], &mut state);

    // Side hints
    let hints = Paragraph::new(vec![
        Line::from(Span::styled("NAVIGATION", Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled("↑↓", Style::default().fg(AMBER).bg(SURFACE)),
            Span::styled("  Navigate", Style::default().fg(DIM).bg(SURFACE)),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(AMBER).bg(SURFACE)),
            Span::styled("  Select", Style::default().fg(DIM).bg(SURFACE)),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().fg(AMBER).bg(SURFACE)),
            Span::styled("     Quit", Style::default().fg(DIM).bg(SURFACE)),
        ]),
        Line::from(""),
        Line::from(Span::styled("ENV VARS", Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("ETHERSCAN_KEY", Style::default().fg(DIM).bg(SURFACE))),
        Line::from(if std::env::var("ETHERSCAN_KEY").is_ok() {
            Span::styled("  ✓ Set", Style::default().fg(GREEN).bg(SURFACE))
        } else {
            Span::styled("  ✗ Not set", Style::default().fg(RED).bg(SURFACE))
        }),
        Line::from(""),
        Line::from(Span::styled("BSCSCAN_KEY", Style::default().fg(DIM).bg(SURFACE))),
        Line::from(if std::env::var("BSCSCAN_KEY").is_ok() {
            Span::styled("  ✓ Set", Style::default().fg(GREEN).bg(SURFACE))
        } else {
            Span::styled("  ✗ Not set", Style::default().fg(RED).bg(SURFACE))
        }),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DARK_CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .wrap(Wrap { trim: true });
    f.render_widget(hints, chunks[0]);
}

fn draw_input(f: &mut Frame, app: &App, area: ratatui::layout::Rect, chain: &str, icon: &str) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Percentage(30),
        ])
        .split(area);

    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(vert[1]);

    let title_color = match chain {
        "BTC" => AMBER,
        _ => PURPLE,
    };

    let input_block = Block::default()
        .title(Span::styled(
            format!(" {} {} Wallet Address ", icon, chain),
            Style::default().fg(title_color).bg(SURFACE).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(title_color).bg(SURFACE))
        .style(Style::default().bg(SURFACE).fg(TEXT));

    let input_text = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(TEXT).bg(SURFACE))
        .block(input_block);
    f.render_widget(input_text, horiz[1]);

    // Cursor
    f.set_cursor_position((
        horiz[1].x + 1 + app.input_cursor as u16,
        horiz[1].y + 1,
    ));

    // Hint
    let hint = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(AMBER).bg(BG)),
            Span::styled(" — submit  ", Style::default().fg(DIM).bg(BG)),
            Span::styled("Esc", Style::default().fg(AMBER).bg(BG)),
            Span::styled(" — back to menu", Style::default().fg(DIM).bg(BG)),
        ]),
    ])
    .alignment(Alignment::Center);

    let hint_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(vert[2]);

    f.render_widget(hint, hint_area[1]);
}

fn draw_batch_input(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(area);

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Percentage(10),
        ])
        .split(chunks[1]);

    let input_block = Block::default()
        .title(Span::styled(
            " ⋮ Batch Analysis — Paste addresses (one per line) ",
            Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(CYAN).bg(SURFACE))
        .style(Style::default().bg(SURFACE).fg(TEXT));

    // Count lines
    let line_count = app.input_buffer.lines().count();
    let counted_text = format!(
        "{}\n\n[{} address(es) entered]",
        app.input_buffer,
        line_count
    );

    let input_text = Paragraph::new(counted_text.as_str())
        .style(Style::default().fg(TEXT).bg(SURFACE))
        .block(input_block)
        .wrap(Wrap { trim: false });
    f.render_widget(input_text, vert[1]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(AMBER).bg(BG)),
        Span::styled(" — new line  ", Style::default().fg(DIM).bg(BG)),
        Span::styled("Ctrl+D", Style::default().fg(AMBER).bg(BG)),
        Span::styled(" — submit  ", Style::default().fg(DIM).bg(BG)),
        Span::styled("Esc", Style::default().fg(AMBER).bg(BG)),
        Span::styled(" — cancel", Style::default().fg(DIM).bg(BG)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(hint, vert[2]);
}

fn draw_loading(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(5),
            Constraint::Percentage(40),
        ])
        .split(area);

    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vert[1]);

    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let tick = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        / 100) as usize;
    let spinner = spinner_chars[tick % spinner_chars.len()];

    let loading = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {}  ", spinner),
                Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.status_msg.as_str(),
                Style::default().fg(TEXT).bg(SURFACE),
            ),
        ]),
        Line::from(""),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .alignment(Alignment::Center);

    f.render_widget(loading, horiz[1]);
}

fn draw_wallet_result(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let wallet = match &app.current_wallet {
        Some(w) => w,
        None => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // summary card
            Constraint::Min(0),     // tx table
        ])
        .split(area);

    let chain_color = match wallet.chain {
        Chain::BTC => AMBER,
        Chain::ETH => PURPLE,
        Chain::USDT_TRON => GREEN,
        Chain::USDT_ETH => GREEN,
        Chain::SOL => CYAN,
        Chain::BNB => AMBER,
    };

    let first = wallet
        .first_seen
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string());
    let last = wallet
        .last_seen
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string());

    let summary_rows = vec![
        Row::new(vec![
            Cell::from("Address").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(wallet.address.as_str()).style(Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("Chain").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(wallet.chain.to_string()).style(Style::default().fg(chain_color).bg(SURFACE).add_modifier(Modifier::BOLD)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("Balance").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(wallet.balance_display.as_str()).style(Style::default().fg(GREEN).bg(SURFACE).add_modifier(Modifier::BOLD)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("Total Received").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(format!("{:.8}", wallet.total_received)).style(Style::default().fg(TEXT).bg(SURFACE)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("Total Sent").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(format!("{:.8}", wallet.total_sent)).style(Style::default().fg(TEXT).bg(SURFACE)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("TX Count").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(wallet.tx_count.to_string()).style(Style::default().fg(TEXT).bg(SURFACE)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("First Seen").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(first).style(Style::default().fg(TEXT).bg(SURFACE)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
        Row::new(vec![
            Cell::from("Last Seen").style(Style::default().fg(DIM).bg(SURFACE)),
            Cell::from(last).style(Style::default().fg(TEXT).bg(SURFACE)),
        ]).style(Style::default().fg(TEXT).bg(SURFACE)),
    ];

    let summary_table = Table::new(
        summary_rows,
        [Constraint::Length(18), Constraint::Min(0)],
    )
    .block(
        Block::default()
            .title(Span::styled(
                format!(" ◈ Wallet Summary — {} ", wallet.chain),
                Style::default().fg(chain_color).bg(SURFACE).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(chain_color).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    );
    f.render_widget(summary_table, chunks[0]);

    // ── TX table ──
    let tx_rows: Vec<Row> = wallet
        .transactions
        .iter()
        .enumerate()
        .map(|(i, tx)| {
            let date_str = tx
                .date
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "pending".to_string());

            let amount_color = if tx.amount_satoshis >= 0 { GREEN } else { RED };
            let cp = tx.counterparty.as_deref().unwrap_or("—");
            let short_txid = if tx.txid.len() > 16 {
                format!("{}…{}", &tx.txid[..8], &tx.txid[tx.txid.len() - 8..])
            } else {
                tx.txid.clone()
            };

            let row_bg = if i % 2 == 0 { SURFACE } else { SURFACE2 };
            let style = Style::reset().bg(row_bg).fg(TEXT);

            Row::new(vec![
                Cell::from(short_txid).style(Style::default().fg(DIM).bg(row_bg)),
                Cell::from(date_str).style(Style::default().fg(TEXT).bg(row_bg)),
                Cell::from(tx.amount_display.as_str())
                    .style(Style::default().fg(amount_color).bg(row_bg).add_modifier(Modifier::BOLD)),
                Cell::from(truncate(cp, 40)).style(Style::default().fg(DIM).bg(row_bg)),
            ])
            .style(style)
        })
        .collect();

    let selected_style = Style::reset()
        .bg(Color::Rgb(0, 60, 70))
        .fg(CYAN)
        .add_modifier(Modifier::BOLD);

    let mut table_state = app.table_state.clone();
    let tx_table = Table::new(
        tx_rows,
        [
            Constraint::Length(20),
            Constraint::Length(18),
            Constraint::Length(22),
            Constraint::Min(0),
        ],
    )
    .header(
        Row::new(vec!["TX ID", "Date (UTC)", "Amount", "Counterparty"])
            .style(Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(Span::styled(
                format!(
                    " ◈ Transaction History ({} txs — ↑↓/PgUp/PgDn to scroll, Esc to back) ",
                    wallet.transactions.len()
                ),
                Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DARK_CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .row_highlight_style(selected_style)
    .highlight_symbol("▶ ");

    f.render_stateful_widget(tx_table, chunks[1], &mut table_state);

    // Scrollbar
    let total = wallet.transactions.len();
    if total > 0 {
        let mut sb_state = ScrollbarState::new(total).position(app.scroll_offset);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(DARK_CYAN).bg(SURFACE));
        f.render_stateful_widget(sb, chunks[1].inner(Margin { vertical: 1, horizontal: 0 }), &mut sb_state);
    }
}

fn draw_batch_result(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let batch = match &app.current_batch {
        Some(b) => b,
        None => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if batch.linked_pairs.is_empty() { 0 } else { (batch.linked_pairs.len() as u16 + 2).min(8) }),
            Constraint::Min(0),
        ])
        .split(area);

    // ── Linked pairs panel ──
    if !batch.linked_pairs.is_empty() {
        let link_items: Vec<ListItem> = batch
            .linked_pairs
            .iter()
            .map(|lp| {
                ListItem::new(Line::from(vec![
                    Span::styled("⚠ LINKED  ", Style::default().fg(RED).bg(SURFACE).add_modifier(Modifier::BOLD)),
                    Span::styled(truncate(&lp.wallet_a, 20), Style::default().fg(CYAN).bg(SURFACE)),
                    Span::styled(" ↔ ", Style::default().fg(AMBER).bg(SURFACE)),
                    Span::styled(truncate(&lp.wallet_b, 20), Style::default().fg(CYAN).bg(SURFACE)),
                    Span::styled("  via ", Style::default().fg(DIM).bg(SURFACE)),
                    Span::styled(truncate(&lp.shared_counterparty, 20), Style::default().fg(RED).bg(SURFACE)),
                ]))
            })
            .collect();

        let links = List::new(link_items).block(
            Block::default()
                .title(Span::styled(
                    " ⚠ Potentially Linked Wallets ",
                    Style::default().fg(RED).bg(SURFACE).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(RED).bg(SURFACE))
                .style(Style::default().bg(SURFACE).fg(TEXT)),
        );
        f.render_widget(links, chunks[0]);
    }

    // ── Wallets summary table ──
    let table_area = if batch.linked_pairs.is_empty() { area } else { chunks[1] };

    let all_rows: Vec<Row> = batch
        .wallets
        .iter()
        .flat_map(|w| {
            let chain_color = match w.chain {
                Chain::BTC => AMBER,
                Chain::ETH => PURPLE,
                Chain::USDT_TRON => GREEN,
                Chain::USDT_ETH => GREEN,
                Chain::SOL => CYAN,
                Chain::BNB => AMBER,
            };
            let first = w.first_seen.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".to_string());
            let last = w.last_seen.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".to_string());

            let header_row_bg = Color::Rgb(20, 28, 40);

            // Wallet header row
            let header_row = Row::new(vec![
                Cell::from(format!("{}", w.chain)).style(Style::default().fg(chain_color).bg(header_row_bg).add_modifier(Modifier::BOLD)),
                Cell::from(truncate(&w.address, 24)).style(Style::default().fg(CYAN).bg(header_row_bg).add_modifier(Modifier::BOLD)),
                Cell::from(w.balance_display.as_str()).style(Style::default().fg(GREEN).bg(header_row_bg).add_modifier(Modifier::BOLD)),
                Cell::from(format!("{} txs", w.tx_count)).style(Style::default().fg(TEXT).bg(header_row_bg)),
                Cell::from(format!("{} → {}", first, last)).style(Style::default().fg(DIM).bg(header_row_bg)),
                Cell::from("").style(Style::default().bg(header_row_bg)),
            ])
            .style(Style::reset().bg(header_row_bg).fg(TEXT));

            // TX rows
            let tx_rows: Vec<Row> = w.transactions.iter().enumerate().map(|(i, tx)| {
                let date_str = tx.date.map(|d| d.format("%m-%d %H:%M").to_string()).unwrap_or_else(|| "pending".to_string());
                let amount_color = if tx.amount_satoshis >= 0 { GREEN } else { RED };
                let short_txid = if tx.txid.len() > 12 { format!("{}…", &tx.txid[..12]) } else { tx.txid.clone() };
                let row_bg = if i % 2 == 0 { SURFACE } else { SURFACE2 };
                let style = Style::reset().bg(row_bg).fg(TEXT);
                Row::new(vec![
                    Cell::from("  ↳").style(Style::default().fg(DIM).bg(row_bg)),
                    Cell::from(short_txid).style(Style::default().fg(DIM).bg(row_bg)),
                    Cell::from(tx.amount_display.as_str()).style(Style::default().fg(amount_color).bg(row_bg)),
                    Cell::from(date_str).style(Style::default().fg(DIM).bg(row_bg)),
                    Cell::from(truncate(tx.counterparty.as_deref().unwrap_or("—"), 30)).style(Style::default().fg(DIM).bg(row_bg)),
                    Cell::from("").style(Style::default().bg(row_bg)),
                ]).style(style)
            }).collect();

            std::iter::once(header_row).chain(tx_rows.into_iter()).collect::<Vec<Row>>()
        })
        .collect();

    let mut ts = app.table_state.clone();
    let table = Table::new(
        all_rows,
        [
            Constraint::Length(6),
            Constraint::Length(26),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Min(0),
            Constraint::Length(0),
        ],
    )
    .header(
        Row::new(vec!["Chain", "Address / TX ID", "Balance / Amount", "TXs / Date", "Date Range / Counterparty", ""])
            .style(Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(Span::styled(
                format!(
                    " ◈ Batch Results — {} wallets (↑↓/PgUp/PgDn, Esc to back) ",
                    batch.wallets.len()
                ),
                Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DARK_CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .row_highlight_style(Style::reset().bg(Color::Rgb(0, 60, 70)).fg(CYAN));

    f.render_stateful_widget(table, table_area, &mut ts);
}

fn draw_export_confirm(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(7),
            Constraint::Percentage(35),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vert[1]);

    let msg = app
        .last_export_msg
        .as_deref()
        .unwrap_or("Export complete");

    let popup = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled("  ✓  Export Successful", Style::default().fg(GREEN).bg(SURFACE).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(format!("  {}", msg), Style::default().fg(TEXT).bg(SURFACE))),
        Line::from(""),
        Line::from(Span::styled("  Press Enter or Esc to return", Style::default().fg(DIM).bg(SURFACE))),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(GREEN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    );
    f.render_widget(popup, horiz[1]);
}

fn draw_error(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(0),
            Constraint::Percentage(30),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(vert[1]);

    let popup = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled("  ✗  Error", Style::default().fg(RED).bg(SURFACE).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(format!("  {}", app.error_msg), Style::default().fg(TEXT).bg(SURFACE))),
        Line::from(""),
        Line::from(Span::styled("  Press Enter or Esc to return", Style::default().fg(DIM).bg(SURFACE))),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(RED).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .wrap(Wrap { trim: true });
    f.render_widget(popup, horiz[1]);
}

fn draw_status(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let status = Paragraph::new(Line::from(vec![
        Span::styled(" ◈ ", Style::default().fg(CYAN).bg(SURFACE)),
        Span::styled(app.status_msg.as_str(), Style::default().fg(TEXT).bg(SURFACE)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DARK_CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    );
    f.render_widget(status, chunks[0]);

    let screen_name = match app.screen {
        Screen::MainMenu => "MAIN MENU",
        Screen::BtcInput => "BTC INPUT",
        Screen::EthInput => "ETH INPUT",
        Screen::UsdtTronInput => "USDT (TRON) INPUT",
        Screen::UsdtEthInput => "USDT (ETH) INPUT",
        Screen::SolInput => "SOL INPUT",
        Screen::BnbInput => "BNB INPUT",
        Screen::BatchInput => "BATCH INPUT",
        Screen::Loading => "LOADING…",
        Screen::WalletResult => "WALLET RESULT",
        Screen::BatchResult => "BATCH RESULT",
        Screen::ExportConfirm => "EXPORT DONE",
        Screen::Error => "ERROR",
    };

    let mode = Paragraph::new(Line::from(vec![
        Span::styled(" ⛓ ChainTrack  ", Style::default().fg(DIM).bg(SURFACE)),
        Span::styled(screen_name, Style::default().fg(CYAN).bg(SURFACE).add_modifier(Modifier::BOLD)),
        Span::styled(" ", Style::default().bg(SURFACE)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DARK_CYAN).bg(SURFACE))
            .style(Style::default().bg(SURFACE).fg(TEXT)),
    )
    .alignment(Alignment::Right);
    f.render_widget(mode, chunks[1]);
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
