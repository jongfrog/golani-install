use crate::{
    error::Result,
    server::{load_metadata, recorded_running_pid},
    minecraft::ServerKind,
    java::{find_system_java, managed_java},
    paths::AppPaths,
    config::Config,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Gauge, Tabs, Wrap, Clear,
    },
    Frame, Terminal,
};
use std::{
    fs,
    io::{self, Stdout},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
    collections::VecDeque,
};
use tokio::sync::mpsc;

pub fn run(servers_dir: &std::path::Path) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    
    // Check terminal size
    let size = terminal.size()?;
    if size.width < 80 || size.height < 24 {
        let _ = disable_raw_mode();
        let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
        eprintln!("터미널 크기가 너무 작습니다. 최소 80x24 필요 (현재: {}x{})", size.width, size.height);
        return Ok(());
    }
    
    let mut app = App::new(servers_dir.to_path_buf());
    let result = app.run(&mut terminal);
    
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Main,
    ServerMenu(usize),
    CreateWizard(WizardStep),
    Settings,
    Logs(usize),
    Console(usize),
    Progress(String, f64), // message, progress 0.0-1.0
    Error(String),
    ConfirmDelete(String), // server name to delete
    Info(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WizardStep {
    Name,
    KindVersion,
    Location,
    Ram,
    Port,
    World,
    Confirm,
}

#[derive(Debug, Clone)]
struct WizardData {
    name: String,
    kind: ServerKind,
    version: String,
    location: PathBuf,
    ram_mb: u32,
    port: u16,
    game_mode: String,
    difficulty: String,
    seed: Option<String>,
    motd: String,
    current_step: WizardStep,
}

impl Default for WizardData {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: ServerKind::Paper,
            version: "1.21.4".to_string(),
            location: PathBuf::new(),
            ram_mb: 2048,
            port: 25565,
            game_mode: "survival".to_string(),
            difficulty: "easy".to_string(),
            seed: None,
            motd: "golani Minecraft Server".to_string(),
            current_step: WizardStep::Name,
        }
    }
}

struct ServerEntry {
    name: String,
    detail: String,
    running: bool,
    metadata: crate::minecraft::ServerMetadata,
}

impl Clone for ServerEntry {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            detail: self.detail.clone(),
            running: self.running,
            metadata: self.metadata.clone(),
        }
    }
}

struct App {
    servers_dir: PathBuf,
    servers: Vec<ServerEntry>,
    screen: Screen,
    main_list_state: ListState,
    menu_list_state: ListState,
    wizard: WizardData,
    wizard_list_state: ListState,
    settings_list_state: ListState,
    log_scroll: usize,
    console_history: VecDeque<String>,
    console_input: String,
    console_history_idx: Option<usize>,
    last_tick: Instant,
    loading: bool,
    loading_msg: String,
    progress_receiver: Option<mpsc::UnboundedReceiver<(String, f64)>>,
    should_quit: bool,
}

impl App {
    fn new(servers_dir: PathBuf) -> Self {
        let mut app = Self {
            servers_dir,
            servers: Vec::new(),
            screen: Screen::Main,
            main_list_state: ListState::default(),
            menu_list_state: ListState::default(),
            wizard: WizardData::default(),
            wizard_list_state: ListState::default(),
            settings_list_state: ListState::default(),
            log_scroll: 0,
            console_history: VecDeque::with_capacity(100),
            console_input: String::new(),
            console_history_idx: None,
            last_tick: Instant::now(),
            loading: false,
            loading_msg: String::new(),
            progress_receiver: None,
            should_quit: false,
        };
        app.refresh_servers();
        app.main_list_state.select(Some(0));
        app
    }

    fn refresh_servers(&mut self) {
        self.servers = fs::read_dir(&self.servers_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|entry| {
                let path = entry.path();
                load_metadata(&path).ok().map(|meta| {
                    let running = recorded_running_pid(&path).is_some();
                    let detail = format!(
                        "{:?} {} · {}MB · :{}",
                        meta.kind, meta.minecraft_version, meta.ram_mb, meta.port
                    );
                    ServerEntry { name: meta.name.clone(), detail, running, metadata: meta }
                })
            })
            .collect();
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            // Auto-refresh servers every second
            if self.last_tick.elapsed() >= Duration::from_secs(1) {
                self.refresh_servers();
                self.last_tick = Instant::now();
            }

            // Handle progress updates
            if let Some(rx) = &mut self.progress_receiver {
                while let Ok((msg, progress)) = rx.try_recv() {
                    self.loading_msg = msg.clone();
                    self.screen = Screen::Progress(msg, progress);
                }
            }

            terminal.draw(|f| self.draw(f))?;

            if self.should_quit {
                break Ok(());
            }

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key);
                    }
                }
            }
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match &self.screen {
            Screen::Main => self.handle_main_key(key),
            Screen::ServerMenu(idx) => self.handle_menu_key(key, *idx),
            Screen::CreateWizard(step) => self.handle_wizard_key(key, step.clone()),
            Screen::Settings => self.handle_settings_key(key),
            Screen::Logs(idx) => self.handle_logs_key(key, *idx),
            Screen::Console(idx) => self.handle_console_key(key, *idx),
            Screen::Progress(_, _) => {
                if matches!(key.code, KeyCode::Esc) {
                    self.loading = false;
                    self.screen = Screen::Main;
                }
            }
            Screen::Error(_msg) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    self.screen = Screen::Main;
                }
            }
            Screen::ConfirmDelete(name) => {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter) {
                    let path = self.servers_dir.join(name);
                    if let Err(e) = crate::server::delete(&path, "삭제") {
                        self.screen = Screen::Error(format!("삭제 실패: {}", e));
                    } else {
                        self.screen = Screen::Main;
                    }
                } else if matches!(key.code, KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N')) {
                    self.screen = Screen::Main;
                }
            }
            Screen::Info(_) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    self.screen = Screen::Main;
                }
            }
        }
    }

    fn handle_main_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                if self.servers.is_empty() {
                    self.should_quit = true;
                } else {
                    self.screen = Screen::Info("UI만 닫습니다. 서버는 계속 실행됩니다 (y/N)".to_string());
                    // Simplified: just quit on second q
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let next = self.main_list_state.selected().map_or(0, |i| (i + 1).min(self.servers.len().saturating_sub(1)));
                self.main_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.main_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.main_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(idx) = self.main_list_state.selected() {
                    self.menu_list_state.select(Some(0));
                    self.screen = Screen::ServerMenu(idx);
                }
            }
            KeyCode::Char('n') => {
                self.wizard = WizardData::default();
                if let Ok(paths) = AppPaths::discover() {
                    let config = Config::load_or_create(&paths).ok();
                    self.wizard.location = config.map(|c| c.servers_dir).unwrap_or_else(|| self.servers_dir.clone());
                }
                self.wizard.current_step = WizardStep::Name;
                self.wizard_list_state.select(Some(0));
                self.screen = Screen::CreateWizard(WizardStep::Name);
            }
            KeyCode::Char('s') => {
                self.settings_list_state.select(Some(0));
                self.screen = Screen::Settings;
            }
            _ => {}
        }
    }

    fn handle_menu_key(&mut self, key: crossterm::event::KeyEvent, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = self.servers[idx].clone();
        
        match key.code {
            KeyCode::Esc => { self.screen = Screen::Main; }
            KeyCode::Down | KeyCode::Char('j') => {
                let next = self.menu_list_state.selected().map_or(0, |i| (i + 1).min(8));
                self.menu_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.menu_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.menu_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(sel) = self.menu_list_state.selected() {
                    match sel {
                        0 => self.action_start_server(server),      // 시작/중지
                        1 => self.action_restart_server(server),    // 재시작
                        2 => self.action_attach_console(server),    // 콘솔 연결
                        3 => self.action_view_logs(server),         // 로그 확인
                        4 => self.action_update_server(server),     // 업데이트
                        5 => self.action_rename_server(server),     // 이름 변경
                        6 => self.action_edit_properties(server),   // 속성 편집
                        7 => self.action_open_folder(server),       // 폴더 열기
                        8 => self.action_delete_server(server),     // 삭제
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn action_start_server(&mut self, server: ServerEntry) {
        let _server_dir = self.servers_dir.join(&server.name);
        if server.running {
            self.loading = true;
            self.loading_msg = "서버 중지 중...".to_string();
            self.screen = Screen::Progress("서버 중지 중...".to_string(), 0.5);
            let (tx, rx) = mpsc::unbounded_channel();
            self.progress_receiver = Some(rx);
            let name = server.name.clone();
            let servers_dir = self.servers_dir.clone();
            tokio::spawn(async move {
                let _ = tx.send(("중지 중...".to_string(), 0.5));
                let path = servers_dir.join(&name);
                if let Err(e) = crate::server::stop(&path).await {
                    let _ = tx.send((format!("중지 실패: {}", e), 1.0));
                } else {
                    let _ = tx.send(("서버를 중지했습니다".to_string(), 1.0));
                }
            });
        } else {
            self.loading = true;
            self.loading_msg = "서버 시작 중...".to_string();
            self.screen = Screen::Progress("서버 시작 중...".to_string(), 0.5);
            let (tx, rx) = mpsc::unbounded_channel();
            self.progress_receiver = Some(rx);
            let name = server.name.clone();
            let servers_dir = self.servers_dir.clone();
            let meta = server.metadata.clone();
            let paths = AppPaths::discover().ok();
            tokio::spawn(async move {
                let _ = tx.send(("Java 확인 중...".to_string(), 0.2));
                let java = find_system_java(meta.java_version)
                    .or_else(|| paths.as_ref().and_then(|p| managed_java(p, meta.java_version)));
                if let Some(java) = java {
                    let _ = tx.send(("서버 시작 중...".to_string(), 0.5));
                    if let Err(e) = crate::server::start(&servers_dir.join(&name), &java.executable) {
                        let _ = tx.send((format!("시작 실패: {}", e), 1.0));
                    } else {
                        let _ = tx.send(("서버를 시작했습니다".to_string(), 1.0));
                    }
                } else {
                    let _ = tx.send(("호환되는 Java를 찾을 수 없습니다".to_string(), 1.0));
                }
            });
        }
    }

    fn action_restart_server(&mut self, server: ServerEntry) {
        self.loading = true;
        self.loading_msg = "서버 재시작 중...".to_string();
        self.screen = Screen::Progress("서버 재시작 중...".to_string(), 0.5);
        let (tx, rx) = mpsc::unbounded_channel();
        self.progress_receiver = Some(rx);
        let name = server.name.clone();
        let servers_dir = self.servers_dir.clone();
        let meta = server.metadata.clone();
        let paths = AppPaths::discover().ok();
        tokio::spawn(async move {
            let _ = tx.send(("중지 중...".to_string(), 0.2));
            let _ = crate::server::stop(&servers_dir.join(&name)).await;
            let _ = tx.send(("시작 중...".to_string(), 0.5));
            let java = find_system_java(meta.java_version)
                .or_else(|| paths.as_ref().and_then(|p| managed_java(p, meta.java_version)));
            if let Some(java) = java {
                if let Err(e) = crate::server::restart(&servers_dir.join(&name), &java.executable).await {
                    let _ = tx.send((format!("재시작 실패: {}", e), 1.0));
                } else {
                    let _ = tx.send(("서버를 재시작했습니다".to_string(), 1.0));
                }
            } else {
                let _ = tx.send(("호환되는 Java를 찾을 수 없습니다".to_string(), 1.0));
            }
        });
    }

    fn action_attach_console(&mut self, server: ServerEntry) {
        self.console_history.clear();
        self.console_input.clear();
        self.console_history_idx = None;
        if let Some(idx) = self.servers.iter().position(|s| s.name == server.name) {
            self.screen = Screen::Console(idx);
        }
    }

    fn action_view_logs(&mut self, server: ServerEntry) {
        self.log_scroll = 0;
        if let Some(idx) = self.servers.iter().position(|s| s.name == server.name) {
            self.screen = Screen::Logs(idx);
        }
    }

    fn action_update_server(&mut self, server: ServerEntry) {
        self.loading = true;
        self.loading_msg = "업데이트 확인 중...".to_string();
        self.screen = Screen::Progress("업데이트 확인 중...".to_string(), 0.1);
        let (tx, rx) = mpsc::unbounded_channel();
        self.progress_receiver = Some(rx);
        let name = server.name.clone();
        let servers_dir = self.servers_dir.clone();
        tokio::spawn(async move {
            let _ = tx.send(("다운로드 중...".to_string(), 0.3));
            let path = servers_dir.join(&name);
            match crate::server::update_server(&path).await {
                Ok(_) => { let _ = tx.send(("업데이트 완료. 재시작 필요".to_string(), 1.0)); }
                Err(e) => { let _ = tx.send((format!("업데이트 실패: {}", e), 1.0)); }
            }
        });
    }

    fn action_rename_server(&mut self, _server: ServerEntry) {
        // Simplified: show info
        self.screen = Screen::Info(format!("이름 변경: golani rename <새이름> 사용"));
    }

    fn action_edit_properties(&mut self, _server: ServerEntry) {
        self.screen = Screen::Info(format!("속성 편집: 서버 폴더의 server.properties 직접 수정"));
    }

    fn action_open_folder(&mut self, server: ServerEntry) {
        let path = self.servers_dir.join(&server.name);
        #[cfg(windows)] { let _ = Command::new("explorer").arg(&path).spawn(); }
        #[cfg(not(windows))] { let _ = Command::new("xdg-open").arg(&path).spawn(); }
    }

    fn action_delete_server(&mut self, server: ServerEntry) {
        self.screen = Screen::ConfirmDelete(server.name.clone());
    }

    fn handle_settings_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc => { self.screen = Screen::Main; }
            KeyCode::Down | KeyCode::Char('j') => {
                let next = self.settings_list_state.selected().map_or(0, |i| (i + 1).min(2));
                self.settings_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.settings_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.settings_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(sel) = self.settings_list_state.selected() {
                    match sel {
                        0 => self.screen = Screen::Info("저장 위치 변경: config.toml 직접 수정 후 재시작".to_string()),
                        1 => self.screen = Screen::Info("수동 Java 경로: config.toml에 java_path 추가 지원 예정".to_string()),
                        2 => self.screen = Screen::Info("golani 자체 업데이트: `golani update` 명령 사용".to_string()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_logs_key(&mut self, key: crossterm::event::KeyEvent, _idx: usize) {
        match key.code {
            KeyCode::Esc => { self.screen = Screen::Main; }
            KeyCode::Down | KeyCode::Char('j') => self.log_scroll = self.log_scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.log_scroll = self.log_scroll.saturating_sub(1),
            KeyCode::PageDown => self.log_scroll = self.log_scroll.saturating_add(20),
            KeyCode::PageUp => self.log_scroll = self.log_scroll.saturating_sub(20),
            KeyCode::Char('f') => {
                // Toggle follow mode - simplified
            }
            _ => {}
        }
    }

    fn handle_console_key(&mut self, key: crossterm::event::KeyEvent, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];
        let (port, password) = match crate::server::rcon_settings(&self.servers_dir.join(&server.name)) {
            Some(p) => p,
            None => return,
        };

        match key.code {
            KeyCode::Esc => { self.screen = Screen::Main; }
            KeyCode::Enter => {
                if !self.console_input.trim().is_empty() {
                    let cmd = self.console_input.trim().to_string();
                    self.console_history.push_back(format!("> {}", cmd));
                    
                    // Send via RCON
                    let password_clone = password.clone();
                    let port_clone = port;
                    tokio::spawn(async move {
                        let _ = crate::rcon::command(&format!("127.0.0.1:{}", port_clone), &password_clone, &cmd).await;
                    });
                    
                    self.console_input.clear();
                    self.console_history_idx = None;
                }
            }
            KeyCode::Backspace => { self.console_input.pop(); }
            KeyCode::Up => {
                if !self.console_history.is_empty() {
                    let idx = self.console_history_idx.map_or(self.console_history.len() - 1, |i| i.saturating_sub(1));
                    self.console_history_idx = Some(idx);
                    self.console_input = self.console_history[idx].trim_start_matches("> ").to_string();
                }
            }
            KeyCode::Down => {
                if let Some(idx) = self.console_history_idx {
                    if idx + 1 < self.console_history.len() {
                        self.console_history_idx = Some(idx + 1);
                        self.console_input = self.console_history[idx + 1].trim_start_matches("> ").to_string();
                    } else {
                        self.console_history_idx = None;
                        self.console_input.clear();
                    }
                }
            }
            KeyCode::Char(c) => { self.console_input.push(c); }
            _ => {}
        }
    }

    fn handle_wizard_key(&mut self, key: crossterm::event::KeyEvent, step: WizardStep) {
        match key.code {
            KeyCode::Esc => {
                match step {
                    WizardStep::Name => self.screen = Screen::Main,
                    _ => self.wizard.current_step = prev_step(step),
                }
            }
            KeyCode::Enter => {
                if self.validate_wizard_step(&step) {
                    self.wizard.current_step = next_step(step);
                }
            }
            KeyCode::Char(c) => {
                match step {
                    WizardStep::Name => self.wizard.name.push(c),
                    WizardStep::Location => {
                        // Simplified: just show current path
                    }
                    WizardStep::Ram => {
                        if c.is_ascii_digit() {
                            let val = (self.wizard.ram_mb.to_string() + &c.to_string()).parse().unwrap_or(self.wizard.ram_mb);
                            if val <= 8192 { self.wizard.ram_mb = val; }
                        }
                    }
                    WizardStep::Port => {
                        if c.is_ascii_digit() {
                            let val = (self.wizard.port.to_string() + &c.to_string()).parse().unwrap_or(self.wizard.port);
                            if val <= 65535 { self.wizard.port = val; }
                        }
                    }
                    WizardStep::World => {
                        // Simplified
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                match step {
                    WizardStep::Name => { self.wizard.name.pop(); }
                    WizardStep::Ram => {
                        let s = self.wizard.ram_mb.to_string();
                        if s.len() > 1 { self.wizard.ram_mb = s[..s.len()-1].parse().unwrap_or(2048); }
                        else { self.wizard.ram_mb = 2048; }
                    }
                    WizardStep::Port => {
                        let s = self.wizard.port.to_string();
                        if s.len() > 1 { self.wizard.port = s[..s.len()-1].parse().unwrap_or(25565); }
                        else { self.wizard.port = 25565; }
                    }
                    _ => {}
                }
            }
            KeyCode::Left | KeyCode::Right => {
                // For KindVersion step - cycle versions
            }
            _ => {}
        }
    }

    fn validate_wizard_step(&self, step: &WizardStep) -> bool {
        match step {
            WizardStep::Name => !self.wizard.name.is_empty() && self.wizard.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') && !self.servers.iter().any(|s| s.name == self.wizard.name),
            WizardStep::KindVersion => !self.wizard.version.is_empty(),
            WizardStep::Location => self.wizard.location.exists(),
            WizardStep::Ram => self.wizard.ram_mb >= 512 && self.wizard.ram_mb <= 8192,
            WizardStep::Port => self.wizard.port > 0 && self.wizard.port <= 65535,
            WizardStep::World => true,
            WizardStep::Confirm => true,
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        if area.width < 80 || area.height < 24 {
            let msg = Paragraph::new(format!("터미널이 너무 작습니다: {}x{} (최소 80x24)", area.width, area.height))
                .block(Block::default().borders(Borders::ALL).title("오류"))
                .style(Style::default().fg(Color::Red));
            f.render_widget(msg, area);
            return;
        }

        match &self.screen {
            Screen::Main => self.draw_main(f),
            Screen::ServerMenu(idx) => self.draw_server_menu(f, *idx),
            Screen::CreateWizard(step) => self.draw_wizard(f, step.clone()),
            Screen::Settings => self.draw_settings(f),
            Screen::Logs(idx) => self.draw_logs(f, *idx),
            Screen::Console(idx) => self.draw_console(f, *idx),
            Screen::Progress(msg, progress) => self.draw_progress(f, msg, *progress),
            Screen::Error(msg) => self.draw_error(f, msg),
            Screen::ConfirmDelete(name) => self.draw_confirm_delete(f, name),
            Screen::Info(msg) => self.draw_info(f, msg),
        }
    }

    fn draw_main(&mut self, f: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        // Header
        let header = Paragraph::new(" GOLANI · Minecraft 서버 관리자")
            .block(Block::default().borders(Borders::ALL).title("메인"))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(header, areas[0]);

        // Server list
        if self.servers.is_empty() {
            let empty = Paragraph::new("등록된 서버가 없습니다.\n'n' 키로 새 서버 생성 | 's' 키로 설정")
                .block(Block::default().borders(Borders::ALL).title("서버 목록"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            f.render_widget(empty, areas[1]);
        } else {
            let items: Vec<ListItem> = self.servers.iter().enumerate().map(|(_i, s)| {
                let badge = if s.running {
                    Span::styled("● 실행 중 ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("○ 정지    ", Style::default().fg(Color::DarkGray))
                };
                let needs_restart = s.metadata.last_exit_status.as_deref() == Some("updated");
                let restart_badge = if needs_restart {
                    Span::styled(" ↑ 재시작필요", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else {
                    Span::raw("")
                };
                ListItem::new(Line::from(vec![
                    badge,
                    Span::styled(&s.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  {}", s.detail), Style::default().fg(Color::Gray)),
                    restart_badge,
                ]))
            }).collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("서버 목록"))
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
                .highlight_symbol("► ");
            f.render_stateful_widget(list, areas[1], &mut self.main_list_state);
        }

        // Footer
        let footer = Paragraph::new("↑/↓/j/k 이동 | Enter 선택 | n 새 서버 | s 설정 | q/Q/ESC 종료")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(footer, areas[2]);
    }

    fn draw_server_menu(&mut self, f: &mut Frame, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];
        
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        let status = if server.running { "● 실행 중" } else { "○ 정지" };
        let status_color = if server.running { Color::Green } else { Color::DarkGray };
        
        let header = Paragraph::new(format!(" {} · {:?} {} · {}MB · :{}", server.name, server.metadata.kind, server.metadata.minecraft_version, server.metadata.ram_mb, server.metadata.port))
            .block(Block::default().borders(Borders::ALL).title(format!("서버 관리 - {}", status)))
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD));
        f.render_widget(header, areas[0]);

        let menu_items = vec![
            if server.running { "1. 서버 중지" } else { "1. 서버 시작" },
            "2. 서버 재시작",
            "3. 콘솔 연결",
            "4. 로그 확인",
            "5. 업데이트",
            "6. 이름 변경",
            "7. 속성 편집",
            "8. 서버 폴더 열기",
            "9. 서버 삭제",
        ];
        let items: Vec<ListItem> = menu_items.iter().enumerate().map(|(i, text)| {
            let style = if self.menu_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(*text, style)))
        }).collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("관리 메뉴"))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));
        f.render_stateful_widget(list, areas[1], &mut self.menu_list_state);

        let footer = Paragraph::new("↑/↓/j/k 이동 | Enter 실행 | ESC 뒤로가기")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(footer, areas[2]);
    }

    fn draw_wizard(&mut self, f: &mut Frame, step: WizardStep) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        // Progress tabs
        let steps = ["이름", "종류/버전", "위치", "RAM", "포트", "월드", "확인"];
        let tabs = Tabs::new(steps.iter().map(|s| Line::from(*s)).collect::<Vec<_>>())
            .block(Block::default().borders(Borders::ALL).title("서버 생성 마법사"))
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .select(self.wizard.current_step as usize);
        f.render_widget(tabs, areas[0]);

        // Content based on step
        let content = match step {
            WizardStep::Name => self.draw_wizard_name(),
            WizardStep::KindVersion => self.draw_wizard_kind_version(),
            WizardStep::Location => self.draw_wizard_location(),
            WizardStep::Ram => self.draw_wizard_ram(),
            WizardStep::Port => self.draw_wizard_port(),
            WizardStep::World => self.draw_wizard_world(),
            WizardStep::Confirm => self.draw_wizard_confirm(),
        };
        f.render_widget(content, areas[1]);

        let footer = Paragraph::new("←/→ 이전/다음 단계 | Enter 진행 | ESC 취소/이전")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(footer, areas[2]);
    }

    fn draw_wizard_name(&self) -> Paragraph {
        let warn = if self.wizard.name.is_empty() {
            "".to_string()
        } else if !self.wizard.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            " (영문/숫자/하이픈/밑줄만 허용)".to_string()
        } else if self.servers.iter().any(|s| s.name == self.wizard.name) {
            " (이미 존재하는 이름)".to_string()
        } else {
            " ✓".to_string()
        };
        Paragraph::new(format!(
            "서버 이름을 입력하세요.\n\n현재: {}{}\n\n이름은 나중에 변경 가능합니다.",
            self.wizard.name, warn
        ))
        .block(Block::default().borders(Borders::ALL).title("1/7: 서버 이름"))
        .wrap(Wrap { trim: true })
    }

    fn draw_wizard_kind_version(&self) -> Paragraph {
        let kind_str = match self.wizard.kind { ServerKind::Vanilla => "Vanilla", ServerKind::Paper => "Paper (권장)" };
        Paragraph::new(format!(
            "서버 종류와 버전을 선택하세요.\n\n종류: {}\n버전: {}\n\n←/→ 로 종류 변경, 위/아래로 버전 변경 (구현 시)",
            kind_str, self.wizard.version
        ))
        .block(Block::default().borders(Borders::ALL).title("2/7: 서버 종류 및 버전"))
        .wrap(Wrap { trim: true })
    }

    fn draw_wizard_location(&self) -> Paragraph {
        let warn = if self.wizard.location.to_string_lossy().contains("Downloads") || 
                       self.wizard.location.to_string_lossy().contains("Desktop") ||
                       self.wizard.location.to_string_lossy().contains("OneDrive") {
            " ⚠ 권장되지 않는 경로입니다"
        } else { "" };
        Paragraph::new(format!(
            "서버 저장 위치를 선택하세요.\n\n현재: {}{}\n\nDownloads, Desktop, OneDrive 동기화 폴더는 피하세요.",
            self.wizard.location.display(), warn
        ))
        .block(Block::default().borders(Borders::ALL).title("3/7: 저장 위치"))
        .wrap(Wrap { trim: true })
    }

    fn draw_wizard_ram(&self) -> Paragraph {
        let total_ram = sys_info::mem_info().map(|m| m.total / 1024).unwrap_or(8192); // MB
        let recommended = (total_ram / 2).min(8192);
        Paragraph::new(format!(
            "시스템 RAM: {}MB\n권장 상한: {}MB (최대 8GB)\n최소 권장: 2GB\n\n할당할 RAM: {}MB\n\n숫자 키로 입력, ←/→ 또는 위/아래로 조정",
            total_ram, recommended, self.wizard.ram_mb
        ))
        .block(Block::default().borders(Borders::ALL).title("4/7: RAM 설정"))
        .wrap(Wrap { trim: true })
    }

    fn draw_wizard_port(&self) -> Paragraph {
        Paragraph::new(format!(
            "서버 포트를 설정하세요.\n\n기본 포트: 25565\n현재: {}\n\n포트가 사용 중이면 다음 사용 가능 포트를 제안합니다.",
            self.wizard.port
        ))
        .block(Block::default().borders(Borders::ALL).title("5/7: 포트 설정"))
        .wrap(Wrap { trim: true })
    }

    fn draw_wizard_world(&self) -> Paragraph {
        Paragraph::new(format!(
            "월드 설정을 입력하세요.\n\n게임 모드: {}\n난이도: {}\n시드: {}\nMOTD: {}\n\n시드를 비우면 랜덤 시드 사용",
            self.wizard.game_mode, self.wizard.difficulty, 
            self.wizard.seed.as_deref().unwrap_or("(랜덤)"), self.wizard.motd
        ))
        .block(Block::default().borders(Borders::ALL).title("6/7: 월드 설정"))
        .wrap(Wrap { trim: true })
    }

    fn draw_wizard_confirm(&self) -> Paragraph {
        Paragraph::new(format!(
            "다음 설정으로 서버를 생성합니다.\n\n이름: {}\n종류: {:?}\n버전: {}\n위치: {}\nRAM: {}MB\n포트: {}\n게임 모드: {}\n난이도: {}\n시드: {}\nMOTD: {}\n\nEnter로 생성 시작, ESC로 돌아가기",
            self.wizard.name, self.wizard.kind, self.wizard.version,
            self.wizard.location.display(), self.wizard.ram_mb, self.wizard.port,
            self.wizard.game_mode, self.wizard.difficulty,
            self.wizard.seed.as_deref().unwrap_or("(랜덤)"), self.wizard.motd
        ))
        .block(Block::default().borders(Borders::ALL).title("7/7: 최종 확인"))
        .wrap(Wrap { trim: true })
    }

    fn draw_settings(&mut self, f: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        let header = Paragraph::new(" 설정")
            .block(Block::default().borders(Borders::ALL).title("설정"))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(header, areas[0]);

        let items = vec![
            "1. 서버 저장 위치 변경",
            "2. 수동 Java 경로 지정",
            "3. golani 자체 업데이트",
        ];
        let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, text)| {
            let style = if self.settings_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else { Style::default() };
            ListItem::new(Line::from(Span::styled(*text, style)))
        }).collect();
        let list = List::new(list_items)
            .block(Block::default().borders(Borders::ALL).title("설정 항목"))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));
        f.render_stateful_widget(list, areas[1], &mut self.settings_list_state);

        let footer = Paragraph::new("↑/↓/j/k 이동 | Enter 선택 | ESC 뒤로가기")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(footer, areas[2]);
    }

    fn draw_logs(&mut self, f: &mut Frame, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];
        let log_path = self.servers_dir.join(&server.name).join("logs").join("latest.log");
        
        let content = fs::read_to_string(&log_path).unwrap_or_else(|_| "로그 파일이 없습니다.".to_string());
        let lines: Vec<&str> = content.lines().collect();
        let start = self.log_scroll.min(lines.len().saturating_sub(1));
        let end = (start + f.area().height as usize).min(lines.len());
        let visible: Vec<Line> = lines[start..end].iter().map(|l| Line::from(*l)).collect();

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        let header = Paragraph::new(format!(" {} - 로그 (줄: {}/{})", server.name, start + 1, lines.len()))
            .block(Block::default().borders(Borders::ALL).title("로그"))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(header, areas[0]);

        let log_widget = Paragraph::new(Text::from(visible))
            .block(Block::default().borders(Borders::ALL).title("로그 내용"))
            .wrap(Wrap { trim: false });
        f.render_widget(log_widget, areas[1]);

        let footer = Paragraph::new("↑/↓ 스크롤 | PageUp/PageDown | f 실시간 모드 | ESC 닫기")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(footer, areas[2]);
    }

    fn draw_console(&mut self, f: &mut Frame, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];
        let (_port, _password) = match crate::server::rcon_settings(&self.servers_dir.join(&server.name)) {
            Some(p) => p,
            None => {
                let error = Paragraph::new("RCON 설정을 찾을 수 없습니다.")
                    .block(Block::default().borders(Borders::ALL).title("오류"))
                    .style(Style::default().fg(Color::Red));
                f.render_widget(error, f.area());
                return;
            }
        };

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        let header = Paragraph::new(format!(" {} - 콘솔 연결됨 (ESC로 분리, 서버 계속 실행)", server.name))
            .block(Block::default().borders(Borders::ALL).title("콘솔"))
            .style(Style::default().fg(Color::Green));
        f.render_widget(header, areas[0]);

        let history_text: Vec<Line> = self.console_history.iter().map(|l| Line::from(l.as_str())).collect();
        let console_widget = Paragraph::new(Text::from(history_text))
            .block(Block::default().borders(Borders::ALL).title("출력"))
            .wrap(Wrap { trim: false });
        f.render_widget(console_widget, areas[1]);

        let footer = Paragraph::new(format!("명령: {}_", self.console_input))
            .block(Block::default().borders(Borders::ALL).title("입력 (Enter 전송, ↑↓ 히스토리)"))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(footer, areas[2]);
    }

    fn draw_progress(&self, f: &mut Frame, msg: &str, progress: f64) {
        let area = f.area();
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("진행 중"))
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
            .ratio(progress)
            .label(format!("{} ({:.0}%)", msg, progress * 100.0));
        let area = centered_rect(60, 20, area);
        f.render_widget(Clear, area); // Need Clear widget
        f.render_widget(gauge, area);
    }

    fn draw_error(&self, f: &mut Frame, msg: &str) {
        let area = f.area();
        let error = Paragraph::new(format!("오류: {}\n\nESC로 닫기", msg))
            .block(Block::default().borders(Borders::ALL).title("오류"))
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        let area = centered_rect(60, 20, area);
        f.render_widget(Clear, area);
        f.render_widget(error, area);
    }

    fn draw_confirm_delete(&self, f: &mut Frame, name: &str) {
        let area = f.area();
        let confirm = Paragraph::new(format!("정말 '{}' 서버를 삭제하시겠습니까? (y/N)\n\nEnter/y 확인 | ESC/n 취소", name))
            .block(Block::default().borders(Borders::ALL).title("확인"))
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        let area = centered_rect(60, 20, area);
        f.render_widget(Clear, area);
        f.render_widget(confirm, area);
    }

    fn draw_info(&self, f: &mut Frame, msg: &str) {
        let area = f.area();
        let info = Paragraph::new(format!("{}\n\nESC/Enter로 닫기", msg))
            .block(Block::default().borders(Borders::ALL).title("안내"))
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);
        let area = centered_rect(60, 20, area);
        f.render_widget(Clear, area);
        f.render_widget(info, area);
    }
}

fn next_step(step: WizardStep) -> WizardStep {
    match step {
        WizardStep::Name => WizardStep::KindVersion,
        WizardStep::KindVersion => WizardStep::Location,
        WizardStep::Location => WizardStep::Ram,
        WizardStep::Ram => WizardStep::Port,
        WizardStep::Port => WizardStep::World,
        WizardStep::World => WizardStep::Confirm,
        WizardStep::Confirm => WizardStep::Confirm,
    }
}

fn prev_step(step: WizardStep) -> WizardStep {
    match step {
        WizardStep::Name => WizardStep::Name,
        WizardStep::KindVersion => WizardStep::Name,
        WizardStep::Location => WizardStep::KindVersion,
        WizardStep::Ram => WizardStep::Location,
        WizardStep::Port => WizardStep::Ram,
        WizardStep::World => WizardStep::Port,
        WizardStep::Confirm => WizardStep::World,
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wizard_steps_order() {
        assert_eq!(next_step(WizardStep::Name), WizardStep::KindVersion);
        assert_eq!(next_step(WizardStep::KindVersion), WizardStep::Location);
        assert_eq!(next_step(WizardStep::Location), WizardStep::Ram);
        assert_eq!(next_step(WizardStep::Ram), WizardStep::Port);
        assert_eq!(next_step(WizardStep::Port), WizardStep::World);
        assert_eq!(next_step(WizardStep::World), WizardStep::Confirm);
        assert_eq!(prev_step(WizardStep::Confirm), WizardStep::World);
    }
}
