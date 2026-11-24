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
        Block, Borders, List, ListItem, ListState, Paragraph, Gauge, Wrap, Clear,
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
use reqwest;
use serde_json;

pub fn run(servers_dir: &std::path::Path) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let size = terminal.size()?;
    if size.width < 80 || size.height < 24 {
        let _ = disable_raw_mode();
        let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
        eprintln!("터미널 크기가 너무 작습니다. 최소 80x24 필요 (현재: {}x{})", size.width, size.height);
        return Ok(());
    }

    draw_splash(&mut terminal)?;

    let mut app = App::new(servers_dir.to_path_buf());
    let result = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn draw_splash(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    use ratatui::widgets::{Block, Borders, Paragraph};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Line;
    use ratatui::layout::Alignment;

    let logo = vec![
        Line::from("░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗"),
        Line::from("██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░██║██║"),
        Line::from("██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║"),
        Line::from("██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║"),
        Line::from("╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║"),
        Line::from("░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝"),
    ];

    let version = env!("CARGO_PKG_VERSION");
    let info = vec![
        Line::from(""),
        Line::from(format!("  golani v{} - Minecraft 서버 관리 도구", version)),
        Line::from(""),
        Line::from("  로딩 중..."),
    ];

    let splash = Paragraph::new(logo.into_iter().chain(info).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title("시작"))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);

    terminal.draw(|f| f.render_widget(splash, f.area()))?;
    std::thread::sleep(std::time::Duration::from_millis(1500));
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Main,
    ServerMenu(usize),
    CreateWizard(WizardStep),
    Settings,
    ServerConsole(usize),
    Progress(String, f64),
    Error(String),
    ConfirmDelete(String),
    Info(String),
    QuitMenu,
    ProgramManagement,
    EditPath {
        field: EditField,
        current: String,
    },
    EditNumber {
        field: EditField,
        current: String,
        min: u32,
        max: u32,
        step: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EditField {
    ServersDir,
    JavaPath,
    DefaultRam,
    WebUiPort,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WizardStep {
    Name,
    SelectKind,
    SelectVersion,
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
    kind_list_state: ListState,
    version_list_state: ListState,
    settings_list_state: ListState,
    server_console_lines: VecDeque<String>,
    server_console_input: String,
    server_console_history_idx: Option<usize>,
    server_console_follow: bool,
    last_tick: Instant,
    loading: bool,
    loading_msg: String,
    progress_receiver: Option<mpsc::UnboundedReceiver<(String, f64, ProgressAction)>>,
    should_quit: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum ProgressAction {
    CreateServer { name: String },
    StartServer { name: String },
    StopServer { name: String },
    RestartServer { name: String },
    UpdateServer { name: String },
    None,
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
            kind_list_state: ListState::default(),
            version_list_state: ListState::default(),
            settings_list_state: ListState::default(),
            server_console_lines: VecDeque::with_capacity(5000),
            server_console_input: String::new(),
            server_console_history_idx: None,
            server_console_follow: true,
            last_tick: Instant::now(),
            loading: false,
            loading_msg: String::new(),
            progress_receiver: None,
            should_quit: false,
        };
        app.refresh_servers();
        if !app.servers.is_empty() {
            app.main_list_state.select(Some(0));
        }
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
                        "{:?} {} · {} · :{}",
                        meta.kind, meta.minecraft_version, format_ram(meta.ram_mb), meta.port
                    );
                    ServerEntry { name: meta.name.clone(), detail, running, metadata: meta }
                })
            })
            .collect();
    }

    fn get_total_items(&self) -> usize {
        self.servers.len() + 1
    }

    fn get_item_name(&self, idx: usize) -> &str {
        if idx < self.servers.len() {
            &self.servers[idx].name
        } else {
            "+ 서버 생성"
        }
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            if self.last_tick.elapsed() >= Duration::from_secs(1) {
                self.refresh_servers();
                self.last_tick = Instant::now();
            }

            let progress_rx = self.progress_receiver.take();
            if let Some(mut rx) = progress_rx {
                while let Ok((msg, progress, action)) = rx.try_recv() {
                    self.loading_msg = msg.clone();
                    self.screen = Screen::Progress(msg, progress);
                    if progress >= 1.0 {
                        self.loading = false;
                        // 진행 완료 시 연계된 화면으로 이동
                        self.screen = match action {
                            ProgressAction::CreateServer { name } => {
                                if let Some(idx) = self.servers.iter().position(|s| s.name == name) {
                                    self.menu_list_state.select(Some(0));
                                    Screen::ServerMenu(idx)
                                } else {
                                    Screen::Main
                                }
                            }
                            ProgressAction::StartServer { name } => {
                                if let Some(idx) = self.servers.iter().position(|s| s.name == name) {
                                    self.menu_list_state.select(Some(0));
                                    Screen::ServerMenu(idx)
                                } else {
                                    Screen::Main
                                }
                            }
                            ProgressAction::StopServer { name } => {
                                if let Some(idx) = self.servers.iter().position(|s| s.name == name) {
                                    self.menu_list_state.select(Some(0));
                                    Screen::ServerMenu(idx)
                                } else {
                                    Screen::Main
                                }
                            }
                            ProgressAction::RestartServer { name } => {
                                if let Some(idx) = self.servers.iter().position(|s| s.name == name) {
                                    self.menu_list_state.select(Some(0));
                                    Screen::ServerMenu(idx)
                                } else {
                                    Screen::Main
                                }
                            }
                            ProgressAction::UpdateServer { name } => {
                                if let Some(idx) = self.servers.iter().position(|s| s.name == name) {
                                    self.menu_list_state.select(Some(0));
                                    Screen::ServerMenu(idx)
                                } else {
                                    Screen::Main
                                }
                            }
                            ProgressAction::None => Screen::Main,
                        };
                    }
                }
                // receiver를 다시 저장
                self.progress_receiver = Some(rx);
            } else {
                self.progress_receiver = None;
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
            Screen::ProgramManagement => self.handle_program_management_key(key),
            Screen::ServerConsole(idx) => self.handle_server_console_key(key, *idx),
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
            Screen::ConfirmDelete(msg) => {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter) {
                    if msg.contains("golani 프로그램을 제거") {
                        // 프로그램 제거: 설정/데이터 유지하고 실행파일만 삭제 안내
                        self.screen = Screen::Info("golani 제거: `cargo uninstall golani` 또는 설치된 실행파일 수동 삭제\n(설정: ~/.golani, 데이터: ~/.golani/servers 유지)".to_string());
                    } else {
                        let path = self.servers_dir.join(&msg);
                        if let Err(e) = crate::server::delete(&path, "삭제") {
                            self.screen = Screen::Error(format!("삭제 실패: {}", e));
                        } else {
                            self.screen = Screen::Main;
                        }
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
            Screen::EditPath { ref field, ref current } => {
                match key.code {
                    KeyCode::Esc => self.screen = Screen::Settings,
                    KeyCode::Enter => {
                        if let Ok(paths) = AppPaths::discover() {
                            if let Ok(mut config) = Config::load_or_create(&paths) {
                                match *field {
                                    EditField::ServersDir => {
                                        config.servers_dir = PathBuf::from(current.clone());
                                    }
                                    EditField::JavaPath => {
                                        config.java_path = if current.is_empty() { None } else { Some(PathBuf::from(current.as_str())) };
                                    }
                                    _ => {}
                                }
                                let _ = config.save(&paths);
                            }
                        }
                        self.screen = Screen::Settings;
                    }
                    KeyCode::Char(c) => {
                        if !matches!(*field, EditField::JavaPath) || c != '/' && c != '\\' && c != ':' {
                            let mut new_current = (*current).clone();
                            new_current.push(c);
                            self.screen = Screen::EditPath { field: *field, current: new_current };
                        }
                    }
                    KeyCode::Backspace => {
                        if !current.is_empty() {
                            let mut new_current = (*current).clone();
                            new_current.pop();
                            self.screen = Screen::EditPath { field: *field, current: new_current };
                        }
                    }
                    _ => {}
                }
            }
            Screen::EditNumber { ref field, ref current, ref min, ref max, ref step } => {
                match key.code {
                    KeyCode::Esc => self.screen = Screen::Settings,
                    KeyCode::Enter => {
                        if let Ok(value) = (*current).parse::<u32>() {
                            if value >= *min && value <= *max {
                                if let Ok(paths) = AppPaths::discover() {
                                    if let Ok(mut config) = Config::load_or_create(&paths) {
                                        match *field {
                                            EditField::DefaultRam => {
                                                config.default_ram_mb = value;
                                            }
                                            EditField::WebUiPort => {
                                                config.web_ui_port = value as u16;
                                            }
                                            _ => {}
                                        }
                                        let _ = config.save(&paths);
                                    }
                                }
                                self.screen = Screen::Settings;
                            }
                        }
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        let mut new_current = (*current).clone();
                        new_current.push(c);
                        if let Ok(val) = new_current.parse::<u32>() {
                            if val <= *max {
                                self.screen = Screen::EditNumber { field: *field, current: new_current, min: *min, max: *max, step: *step };
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        if !current.is_empty() {
                            let mut new_current = (*current).clone();
                            new_current.pop();
                            self.screen = Screen::EditNumber { field: *field, current: new_current, min: *min, max: *max, step: *step };
                        }
                    }
                    KeyCode::Up => {
                        if let Ok(val) = (*current).parse::<u32>() {
                            let new_val = (val + *step).min(*max);
                            self.screen = Screen::EditNumber { field: *field, current: new_val.to_string(), min: *min, max: *max, step: *step };
                        }
                    }
                    KeyCode::Down => {
                        if let Ok(val) = (*current).parse::<u32>() {
                            let new_val = if val >= *min + *step { val - *step } else { *min };
                            self.screen = Screen::EditNumber { field: *field, current: new_val.to_string(), min: *min, max: *max, step: *step };
                        }
                    }
                    _ => {}
                }
            }
            Screen::QuitMenu => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        let next = self.menu_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                        self.menu_list_state.select(Some(next));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let next = self.menu_list_state.selected().map_or(0, |i| (i + 1).min(2));
                        self.menu_list_state.select(Some(next));
                    }
                    KeyCode::Enter => {
                        if let Some(sel) = self.menu_list_state.selected() {
                            match sel {
                                0 => self.screen = Screen::Main,
                                1 => { self.screen = Screen::Settings; self.settings_list_state.select(Some(0)); },
                                2 => self.should_quit = true,
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Esc => self.screen = Screen::Main,
                    _ => {}
                }
            }
        }
    }

    fn handle_main_key(&mut self, key: crossterm::event::KeyEvent) {
        let total = self.get_total_items();
        match key.code {
            KeyCode::Esc => {
                self.menu_list_state.select(Some(0));
                self.screen = Screen::QuitMenu;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let next = self.main_list_state.selected().map_or(0, |i| (i + 1).min(total - 1));
                self.main_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.main_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.main_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(idx) = self.main_list_state.selected() {
                    if idx < self.servers.len() {
                        self.menu_list_state.select(Some(0));
                        self.screen = Screen::ServerMenu(idx);
                    } else {
                        self.wizard = WizardData::default();
                        if let Ok(paths) = AppPaths::discover() {
                            let config = Config::load_or_create(&paths).ok();
                            self.wizard.location = config.map(|c| c.servers_dir).unwrap_or_else(|| self.servers_dir.clone());
                        }
                        self.wizard.current_step = WizardStep::Name;
                        self.wizard_list_state.select(Some(0));
                        self.screen = Screen::CreateWizard(WizardStep::Name);
                    }
                }
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
                let next = self.menu_list_state.selected().map_or(0, |i| (i + 1).min(7));
                self.menu_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.menu_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.menu_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(sel) = self.menu_list_state.selected() {
                    match sel {
                        0 => self.action_start_server(server),
                        1 => self.action_restart_server(server),
                        2 => self.action_server_console(server),
                        3 => self.action_update_server(server),
                        4 => self.action_rename_server(server),
                        5 => self.action_edit_properties(server),
                        6 => self.action_open_folder(server),
                        7 => self.action_delete_server(server),
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
                let _ = tx.send(("중지 중...".to_string(), 0.5, ProgressAction::StopServer { name: name.clone() }));
                let path = servers_dir.join(&name);
                if let Err(e) = crate::server::stop(&path).await {
                    let _ = tx.send((format!("중지 실패: {}", e), 1.0, ProgressAction::None));
                } else {
                    let _ = tx.send(("서버를 중지했습니다".to_string(), 1.0, ProgressAction::StopServer { name }));
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
            let config = paths.as_ref().and_then(|p| Config::load_or_create(p).ok());
            tokio::spawn(async move {
                            let _ = tx.send(("Java 확인 중...".to_string(), 0.2, ProgressAction::StartServer { name: name.clone() }));
                            let java = crate::java::find_java_with_config(
                                config.as_ref().and_then(|c| c.java_path.as_deref()),
                                paths.as_ref().unwrap(),
                                meta.java_version
                            );
                            if let Some(java) = java {
                                let _ = tx.send(("서버 시작 중...".to_string(), 0.5, ProgressAction::StartServer { name: name.clone() }));
                                if let Err(e) = crate::server::start(&servers_dir.join(&name), &java.executable).await {
                                    let _ = tx.send((format!("시작 실패: {}", e), 1.0, ProgressAction::None));
                                } else {
                                    let _ = tx.send(("서버를 시작했습니다".to_string(), 1.0, ProgressAction::StartServer { name }));
                                }
                            } else {
                                let _ = tx.send(("호환되는 Java를 찾을 수 없습니다".to_string(), 1.0, ProgressAction::None));
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
        let config = paths.as_ref().and_then(|p| Config::load_or_create(p).ok());
        tokio::spawn(async move {
            let _ = tx.send(("중지 중...".to_string(), 0.2, ProgressAction::StopServer { name: name.clone() }));
            let _ = crate::server::stop(&servers_dir.join(&name)).await;
            let _ = tx.send(("시작 중...".to_string(), 0.5, ProgressAction::StartServer { name: name.clone() }));
            let java = crate::java::find_java_with_config(
                config.as_ref().and_then(|c| c.java_path.as_deref()),
                paths.as_ref().unwrap(),
                meta.java_version
            );
            if let Some(java) = java {
                if let Err(e) = crate::server::restart(&servers_dir.join(&name), &java.executable).await {
                    let _ = tx.send((format!("재시작 실패: {}", e), 1.0, ProgressAction::None));
                } else {
                    let _ = tx.send(("서버를 재시작했습니다".to_string(), 1.0, ProgressAction::RestartServer { name }));
                }
            } else {
                let _ = tx.send(("호환되는 Java를 찾을 수 없습니다".to_string(), 1.0, ProgressAction::None));
            }
        });
    }

    fn action_server_console(&mut self, server: ServerEntry) {
        self.server_console_lines.clear();
        self.server_console_input.clear();
        self.server_console_history_idx = None;
        self.server_console_follow = true;
        // 기존 로그 파일 내용 로드
        let log_path = self.servers_dir.join(&server.name).join("logs").join("latest.log");
        if let Ok(content) = fs::read_to_string(&log_path) {
            for line in content.lines() {
                self.server_console_lines.push_back(line.to_string());
            }
            // 최대 5000줄 유지
            while self.server_console_lines.len() > 5000 {
                self.server_console_lines.pop_front();
            }
        }
        if let Some(idx) = self.servers.iter().position(|s| s.name == server.name) {
            self.screen = Screen::ServerConsole(idx);
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
            let _ = tx.send(("다운로드 중...".to_string(), 0.3, ProgressAction::UpdateServer { name: name.clone() }));
            let path = servers_dir.join(&name);
            match crate::server::update_server(&path).await {
                Ok(_) => { let _ = tx.send(("업데이트 완료. 재시작 필요".to_string(), 1.0, ProgressAction::UpdateServer { name })); }
                Err(e) => { let _ = tx.send((format!("업데이트 실패: {}", e), 1.0, ProgressAction::None)); }
            }
        });
    }

    fn action_rename_server(&mut self, _server: ServerEntry) {
        self.screen = Screen::Info("이름 변경: golani rename <새이름> 사용".to_string());
    }

    fn action_edit_properties(&mut self, _server: ServerEntry) {
        self.screen = Screen::Info("속성 편집: 서버 폴더의 server.properties 직접 수정".to_string());
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
                let next = self.settings_list_state.selected().map_or(0, |i| (i + 1).min(7));
                self.settings_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.settings_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.settings_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(sel) = self.settings_list_state.selected() {
                    match sel {
                                            0 => self.edit_servers_dir(),
                                            1 => self.edit_java_path(),
                                            2 => self.edit_default_ram(),
                                            3 => self.toggle_web_ui(),
                                            4 => self.edit_web_ui_port(),
                                            5 => self.toggle_firewall(),
                                            6 => self.screen = Screen::ProgramManagement,
                                            _ => {}
                                        }
                }
            }
            _ => {}
        }
    }

    fn handle_program_management_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc => { self.screen = Screen::Settings; }
            KeyCode::Down | KeyCode::Char('j') => {
                let next = self.menu_list_state.selected().map_or(0, |i| (i + 1).min(2));
                self.menu_list_state.select(Some(next));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next = self.menu_list_state.selected().map_or(0, |i| i.saturating_sub(1));
                self.menu_list_state.select(Some(next));
            }
            KeyCode::Enter => {
                if let Some(sel) = self.menu_list_state.selected() {
                    match sel {
                        0 => {
                            // 프로그램 업데이트 - TUI에서 직접 업데이트 실행
                            self.screen = Screen::Progress("업데이트 확인 중...".to_string(), 0.1);
                            let (tx, rx) = mpsc::unbounded_channel();
                            self.progress_receiver = Some(rx);
                            tokio::spawn(async move {
                                let _ = tx.send(("업데이트 확인 중...".to_string(), 0.1, ProgressAction::None));
                                // golani update 로직 수행
                                let client = reqwest::Client::builder()
                                    .user_agent("golani/0.1.20 (https://golani.kro.kr)")
                                    .build();
                                let release = async {
                                    if let Ok(client) = client {
                                        let resp = client.get("https://api.github.com/repos/jongfrog/golani-install/releases/latest").send().await.ok()?;
                                        let resp = resp.error_for_status().ok()?;
                                        resp.json::<serde_json::Value>().await.ok()
                                    } else {
                                        None
                                    }
                                }.await;
                                if let Some(release) = release {
                                    let tag_name = release["tag_name"].as_str().unwrap_or("unknown");
                                    if tag_name != "v0.1.20" && tag_name != "0.1.20" {
                                        let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };
                                        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
                                        let asset_name = format!("golani-{}-{}.zip", os, arch);
                                        if let Some(asset) = release["assets"].as_array().and_then(|a| a.iter().find(|a| a["name"].as_str().unwrap_or("").contains(&format!("golani-{}-{}", os, arch)))) {
                                            let download_url = asset["browser_download_url"].as_str().unwrap_or("");
                                            let _ = tx.send(("다운로드 중...".to_string(), 0.5, ProgressAction::None));
                                            if let Ok(resp) = reqwest::Client::new().get(download_url).send().await {
                                                if let Ok(bytes) = resp.bytes().await {
                                                    let current_exe = std::env::current_exe().unwrap();
                                                    let temp_exe = current_exe.with_extension("exe.new");
                                                    if std::fs::write(&temp_exe, &bytes).is_ok() {
                                                        let _ = tx.send(("다운로드 완료! 수동 교체 필요".to_string(), 1.0, ProgressAction::None));
                                                        return;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                let _ = tx.send(("업데이트 없음 또는 실패".to_string(), 1.0, ProgressAction::None));
                            });
                        }
                        1 => {
                            // 프로그램 제거
                            self.screen = Screen::ConfirmDelete("golani 프로그램을 제거하시겠습니까? (설정과 데이터는 유지됩니다)".to_string());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn get_config(&self) -> Option<Config> {
        AppPaths::discover().ok().and_then(|p| Config::load_or_create(&p).ok())
    }

    fn edit_servers_dir(&mut self) {
        self.screen = Screen::EditPath {
            field: EditField::ServersDir,
            current: self.get_config().map(|c| c.servers_dir.display().to_string()).unwrap_or_default(),
        };
    }

    fn edit_java_path(&mut self) {
        self.screen = Screen::EditPath {
            field: EditField::JavaPath,
            current: self.get_config().and_then(|c| c.java_path.as_ref().map(|p| p.display().to_string())).unwrap_or_default(),
        };
    }

    fn edit_default_ram(&mut self) {
        self.screen = Screen::EditNumber {
            field: EditField::DefaultRam,
            current: self.get_config().map(|c| c.default_ram_mb.to_string()).unwrap_or("2048".to_string()),
            min: 512,
            max: 65536,
            step: 512,
        };
    }

    fn toggle_web_ui(&mut self) {
        if let Ok(paths) = AppPaths::discover() {
            if let Ok(mut config) = Config::load_or_create(&paths) {
                config.web_ui_enabled = !config.web_ui_enabled;
                let _ = config.save(&paths);
            }
        }
        self.refresh_servers();
    }

    fn edit_web_ui_port(&mut self) {
        self.screen = Screen::EditNumber {
            field: EditField::WebUiPort,
            current: self.get_config().map(|c| c.web_ui_port.to_string()).unwrap_or("8788".to_string()),
            min: 1024,
            max: 65535,
            step: 1,
        };
    }

    fn toggle_firewall(&mut self) {
        if let Ok(paths) = AppPaths::discover() {
            if let Ok(mut config) = Config::load_or_create(&paths) {
                config.firewall_managed = !config.firewall_managed;
                let _ = config.save(&paths);
            }
        }
        self.refresh_servers();
    }

    fn handle_server_console_key(&mut self, key: crossterm::event::KeyEvent, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];
        let (port, password) = match crate::server::rcon_settings(&self.servers_dir.join(&server.name)) {
            Some(p) => p,
            None => return,
        };

        match key.code {
            KeyCode::Esc => {
                if idx < self.servers.len() {
                    self.menu_list_state.select(Some(0));
                    self.screen = Screen::ServerMenu(idx);
                } else {
                    self.screen = Screen::Main;
                }
            }
            KeyCode::Char('f') => {
                self.server_console_follow = !self.server_console_follow;
            }
            KeyCode::Enter => {
                if !self.server_console_input.trim().is_empty() {
                    let cmd = self.server_console_input.trim().to_string();
                    self.server_console_lines.push_back(format!("> {}", cmd));

                    let password_clone = password.clone();
                    let port_clone = port;
                    tokio::spawn(async move {
                        let _ = crate::rcon::command(&format!("127.0.0.1:{}", port_clone), &password_clone, &cmd).await;
                    });

                    self.server_console_input.clear();
                    self.server_console_history_idx = None;
                }
            }
            KeyCode::Backspace => { self.server_console_input.pop(); }
            KeyCode::Up => {
                if !self.server_console_lines.is_empty() {
                    let idx = self.server_console_history_idx.map_or(self.server_console_lines.len() - 1, |i| i.saturating_sub(1));
                    self.server_console_history_idx = Some(idx);
                    if let Some(line) = self.server_console_lines.get(idx) {
                        if line.starts_with("> ") {
                            self.server_console_input = line.trim_start_matches("> ").to_string();
                        }
                    }
                }
            }
            KeyCode::Down => {
                if let Some(idx) = self.server_console_history_idx {
                    if idx + 1 < self.server_console_lines.len() {
                        self.server_console_history_idx = Some(idx + 1);
                        if let Some(line) = self.server_console_lines.get(idx + 1) {
                            if line.starts_with("> ") {
                                self.server_console_input = line.trim_start_matches("> ").to_string();
                            }
                        }
                    } else {
                        self.server_console_history_idx = None;
                        self.server_console_input.clear();
                    }
                }
            }
            KeyCode::PageUp => {
                // 스크롤 업 - follow 모드 비활성화
                self.server_console_follow = false;
            }
            KeyCode::PageDown => {
                // 스크롤 다운 - 마지막이면 follow 모드 활성화
                if self.server_console_history_idx.is_none() {
                    self.server_console_follow = true;
                }
            }
            KeyCode::Char(c) => { self.server_console_input.push(c); }
            _ => {}
        }
    }

    fn handle_wizard_key(&mut self, key: crossterm::event::KeyEvent, step: WizardStep) {
        match key.code {
            KeyCode::Esc => {
                match step {
                    WizardStep::Name => self.screen = Screen::Main,
                    _ => {
                        self.wizard.current_step = prev_step(step);
                        self.screen = Screen::CreateWizard(self.wizard.current_step);
                    }
                }
            }
            KeyCode::Enter => {
                if self.validate_wizard_step(&step) {
                    match step {
                        WizardStep::Confirm => {
                            self.create_server_from_wizard();
                        }
                        WizardStep::SelectKind => {
                            if let Some(idx) = self.kind_list_state.selected() {
                                self.wizard.kind = match idx {
                                    0 => ServerKind::Vanilla,
                                    1 => ServerKind::Paper,
                                    _ => ServerKind::Paper,
                                };
                            }
                            self.wizard.current_step = next_step(step);
                            self.screen = Screen::CreateWizard(self.wizard.current_step);
                            self.version_list_state.select(Some(0));
                        }
                        WizardStep::SelectVersion => {
                            if let Some(idx) = self.version_list_state.selected() {
                                let versions = self.get_versions_for_kind(&self.wizard.kind);
                                if let Some(v) = versions.get(idx) {
                                    self.wizard.version = v.clone();
                                }
                            }
                            self.wizard.current_step = next_step(step);
                            self.screen = Screen::CreateWizard(self.wizard.current_step);
                        }
                        _ => {
                            self.wizard.current_step = next_step(step);
                            self.screen = Screen::CreateWizard(self.wizard.current_step);
                            if self.wizard.current_step == WizardStep::SelectKind {
                                self.kind_list_state.select(Some(0));
                            } else if self.wizard.current_step == WizardStep::SelectVersion {
                                self.version_list_state.select(Some(0));
                            }
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                match step {
                    WizardStep::Name => self.wizard.name.push(c),
                    WizardStep::Location => {},
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
                    WizardStep::World => {},
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
            KeyCode::Up | KeyCode::Down => {
                match step {
                    WizardStep::SelectKind => {
                        let next = match key.code {
                            KeyCode::Up => self.kind_list_state.selected().map_or(0, |i| i.saturating_sub(1)),
                            KeyCode::Down => (self.kind_list_state.selected().map_or(0, |i| i + 1)).min(1),
                            _ => self.kind_list_state.selected().unwrap_or(0),
                        };
                        self.kind_list_state.select(Some(next));
                    }
                    WizardStep::SelectVersion => {
                        let versions = self.get_versions_for_kind(&self.wizard.kind);
                        if !versions.is_empty() {
                            let next = match key.code {
                                KeyCode::Up => self.version_list_state.selected().map_or(0, |i| i.saturating_sub(1)),
                                KeyCode::Down => (self.version_list_state.selected().map_or(0, |i| i + 1)).min(versions.len() - 1),
                                _ => self.version_list_state.selected().unwrap_or(0),
                            };
                            self.version_list_state.select(Some(next));
                        }
                    }
                    WizardStep::Ram => {
                        match key.code {
                            KeyCode::Up => {
                                if self.wizard.ram_mb + 512 <= 8192 { self.wizard.ram_mb += 512; }
                            }
                            KeyCode::Down => {
                                if self.wizard.ram_mb > 512 { self.wizard.ram_mb -= 512; }
                            }
                            _ => {}
                        }
                    }
                    WizardStep::Port => {
                        match key.code {
                            KeyCode::Up => {
                                if self.wizard.port < 65535 { self.wizard.port += 1; }
                            }
                            KeyCode::Down => {
                                if self.wizard.port > 1 { self.wizard.port -= 1; }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Left | KeyCode::Right => {
                match step {
                    WizardStep::Ram => {
                        if self.wizard.ram_mb > 512 { self.wizard.ram_mb -= 512; }
                    }
                    WizardStep::Port => {
                        if self.wizard.port > 1 { self.wizard.port -= 1; }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn validate_wizard_step(&self, step: &WizardStep) -> bool {
        match step {
            WizardStep::Name => !self.wizard.name.is_empty() && !self.servers.iter().any(|s| s.name == self.wizard.name),
            WizardStep::SelectKind => self.kind_list_state.selected().is_some(),
            WizardStep::SelectVersion => self.version_list_state.selected().is_some(),
            WizardStep::Location => self.wizard.location.exists(),
            WizardStep::Ram => self.wizard.ram_mb >= 512 && self.wizard.ram_mb <= 8192,
            WizardStep::Port => self.wizard.port > 0 && self.wizard.port <= 65535,
            WizardStep::World => true,
            WizardStep::Confirm => true,
        }
    }

    fn get_versions_for_kind(&self, kind: &ServerKind) -> Vec<String> {
        match kind {
            ServerKind::Vanilla => vec![
                "1.21.4".to_string(), "1.21.3".to_string(), "1.21.2".to_string(),
                "1.21.1".to_string(), "1.21".to_string(), "1.20.6".to_string(),
                "1.20.4".to_string(), "1.20.2".to_string(), "1.20.1".to_string(),
                "1.19.4".to_string(), "1.19.2".to_string(), "1.18.2".to_string(),
                "1.17.1".to_string(), "1.16.5".to_string(),
            ],
            ServerKind::Paper => vec![
                "1.21.4".to_string(), "1.21.3".to_string(), "1.21.2".to_string(),
                "1.21.1".to_string(), "1.21".to_string(), "1.20.6".to_string(),
                "1.20.4".to_string(), "1.20.2".to_string(), "1.20.1".to_string(),
                "1.19.4".to_string(), "1.19.2".to_string(), "1.18.2".to_string(),
                "1.17.1".to_string(), "1.16.5".to_string(),
            ],
        }
    }

    fn create_server_from_wizard(&mut self) {
        self.loading = true;
        self.loading_msg = "서버 생성 중...".to_string();
        self.screen = Screen::Progress("서버 생성 중...".to_string(), 0.1);

        let (tx, rx) = mpsc::unbounded_channel();
        self.progress_receiver = Some(rx);

        let name = self.wizard.name.clone();
        let kind = self.wizard.kind;
        let version = self.wizard.version.clone();
        let ram_mb = self.wizard.ram_mb;
        let port = self.wizard.port;
        let game_mode = self.wizard.game_mode.clone();
        let difficulty = self.wizard.difficulty.clone();
        let seed = self.wizard.seed.clone();
        let motd = self.wizard.motd.clone();
        let servers_dir = self.servers_dir.clone();

        tokio::spawn(async move {
            let _ = tx.send(("설정 검증 중...".to_string(), 0.1, ProgressAction::CreateServer { name: name.clone() }));

            let api = crate::minecraft::MinecraftApi::new();
            if let Err(e) = api {
                let _ = tx.send((format!("API 초기화 실패: {}", e), 1.0, ProgressAction::None));
                return;
            }
            let api = api.unwrap();

            let _ = tx.send(("서버 파일 다운로드 중...".to_string(), 0.3, ProgressAction::CreateServer { name: name.clone() }));

            let result = crate::minecraft::create_server(
                &api,
                &servers_dir,
                &name,
                kind,
                &version,
                ram_mb,
                port,
                &motd,
                &game_mode,
                &difficulty,
                seed.as_deref(),
            ).await;

            match result {
                Ok(metadata) => {
                    let _ = tx.send((format!("서버 생성 완료: {} (포트: {})", metadata.name, metadata.port), 1.0, ProgressAction::CreateServer { name }));
                }
                Err(e) => {
                    let _ = tx.send((format!("생성 실패: {}", e), 1.0, ProgressAction::None));
                }
            }
        });
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
            Screen::ProgramManagement => self.draw_program_management(f),
            Screen::ServerConsole(idx) => self.draw_server_console(f, *idx),
            Screen::Progress(msg, progress) => self.draw_progress(f, msg, *progress),
            Screen::Error(msg) => self.draw_error(f, msg),
            Screen::ConfirmDelete(name) => self.draw_confirm_delete(f, name),
            Screen::Info(msg) => self.draw_error(f, msg),
            Screen::QuitMenu => self.draw_quit_menu(f),
            Screen::EditPath { .. } => self.draw_edit_path(f),
            Screen::EditNumber { .. } => self.draw_edit_number(f),
        }
    }

    fn draw_main(&mut self, f: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .margin(1)
            .split(f.area());

        let header = Paragraph::new(" GOLANI · Minecraft 서버 관리자 ")
            .block(Block::default().borders(Borders::ALL).title(" 메인 ").border_style(Style::default().fg(Color::Cyan)))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        f.render_widget(header, areas[0]);

        let total = self.get_total_items();
        let names: Vec<String> = (0..total).map(|i| self.get_item_name(i).to_string()).collect();
        let items: Vec<ListItem> = names.iter().enumerate().map(|(i, name)| {
            let style = if self.main_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else { Style::default().fg(Color::White) };
            
            // 서버 상태 아이콘 추가
            let display_name = if i < self.servers.len() {
                let status = if self.servers[i].running { " ●" } else { " ○" };
                format!("  {}  {}", name.as_str(), status)
            } else {
                format!("  {}  ", name.as_str())
            };
            
            ListItem::new(Line::from(Span::styled(display_name, style)))
        }).collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" 서버 목록 ").border_style(Style::default().fg(Color::DarkGray)))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");
        f.render_stateful_widget(list, areas[1], &mut self.main_list_state);

        let footer = Paragraph::new(" ↑/↓  이동    Enter  선택    ESC  메뉴 ")
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(footer, areas[2]);
    }

    fn draw_quit_menu(&mut self, f: &mut Frame) {
        let area = f.area();
        let items = vec!["  계속하기  ", "  설정  ", "  나가기  "];
        let items_widget: Vec<ListItem> = items.iter().enumerate().map(|(i, text)| {
            let style = if self.menu_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(*text, style)))
        }).collect();

        let list = List::new(items_widget)
            .block(Block::default().borders(Borders::ALL).title(" 메뉴 ").border_style(Style::default().fg(Color::Cyan)))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");
        let area = centered_rect(50, 40, area);
        f.render_widget(Clear, area);
        f.render_stateful_widget(list, area, &mut self.menu_list_state);
    }

    fn draw_server_menu(&mut self, f: &mut Frame, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .margin(1)
            .split(f.area());

        let status = if server.running { "● 실행 중" } else { "○ 정지" };
        let status_color = if server.running { Color::Green } else { Color::DarkGray };

        let header = Paragraph::new(format!(" {}    {}", server.name, status))
            .block(Block::default().borders(Borders::ALL).title(" 서버 관리 ").border_style(Style::default().fg(status_color)))
            .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        f.render_widget(header, areas[0]);

        let menu_items = vec![
            if server.running { "  서버 중지  " } else { "  서버 시작  " },
            "  서버 재시작  ",
            "  콘솔/로그  ",
            "  업데이트  ",
            "  이름 변경  ",
            "  속성 편집  ",
            "  폴더 열기  ",
            "  서버 삭제  ",
        ];

        let items: Vec<ListItem> = menu_items.iter().enumerate().map(|(i, text)| {
            let style = if self.menu_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(*text, style)))
        }).collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!(" {} 상세 ", server.name)).border_style(Style::default().fg(Color::DarkGray)))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");
        f.render_stateful_widget(list, areas[1], &mut self.menu_list_state);

        let footer = Paragraph::new(" ↑/↓  이동    Enter  선택    ESC  뒤로가기 ")
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(footer, areas[2]);
    }

    fn draw_wizard(&mut self, f: &mut Frame, step: WizardStep) {
        let area = centered_rect(80, 80, f.area());
        f.render_widget(Clear, area);
        let widget = match step {
            WizardStep::Name => self.draw_wizard_name(),
            WizardStep::SelectKind => self.draw_wizard_select_kind(),
            WizardStep::SelectVersion => self.draw_wizard_select_version(),
            WizardStep::Location => self.draw_wizard_location(),
            WizardStep::Ram => self.draw_wizard_ram(),
            WizardStep::Port => self.draw_wizard_port(),
            WizardStep::World => self.draw_wizard_world(),
            WizardStep::Confirm => self.draw_wizard_confirm(),
        };
        f.render_widget(widget, area);
    }

    fn draw_wizard_name(&self) -> Paragraph {
        let warn = if self.wizard.name.is_empty() {
            "".to_string()
        } else if self.servers.iter().any(|s| s.name == self.wizard.name) {
            " (이미 존재하는 이름)".to_string()
        } else {
            " ✓".to_string()
        };
        Paragraph::new(format!(
            "\n서버 이름을 입력하세요.\n\n현재: {}{}\n\n한글/영문/숫자/하이픈/밑줄 허용\n이름은 나중에 변경 가능합니다.\n",
            self.wizard.name, warn
        ))
        .block(Block::default().borders(Borders::ALL).title(" 1/8: 서버 이름 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_select_kind(&self) -> Paragraph {
        let kinds = vec!["Vanilla (공식)", "Paper (최적화, 플러그인 지원)"];
        let kind_lines: Vec<String> = kinds.iter().enumerate().map(|(i, k)| {
            if i == self.kind_list_state.selected().unwrap_or(0) {
                format!("► {}", k)
            } else {
                format!("  {}", k)
            }
        }).collect();

        Paragraph::new(format!(
            "\n서버 종류를 선택하세요.\n\n{}\n\n위/아래: 이동 | Enter: 선택 | ESC: 돌아가기\n",
            kind_lines.join("\n")
        ))
        .block(Block::default().borders(Borders::ALL).title(" 2/8: 서버 종류 선택 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_select_version(&self) -> Paragraph {
        let versions = self.get_versions_for_kind(&self.wizard.kind);
        let version_lines: Vec<String> = versions.iter().enumerate().map(|(i, v)| {
            let marker = if i == self.version_list_state.selected().unwrap_or(0) { "► " } else { "  " };
            let current = if *v == self.wizard.version { " (현재)" } else { "" };
            format!("{}{}{}", marker, v, current)
        }).collect();

        let kind_name = match self.wizard.kind {
            ServerKind::Vanilla => "Vanilla",
            ServerKind::Paper => "Paper",
        };

        Paragraph::new(format!(
            "\n{} 버전을 선택하세요.\n\n{}\n\n위/아래: 이동 | Enter: 선택 | ESC: 돌아가기\n",
            kind_name,
            version_lines.join("\n")
        ))
        .block(Block::default().borders(Borders::ALL).title(" 3/8: 버전 선택 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_location(&self) -> Paragraph {
        let warn = if self.wizard.location.to_string_lossy().contains("Downloads") ||
                       self.wizard.location.to_string_lossy().contains("Desktop") ||
                       self.wizard.location.to_string_lossy().contains("OneDrive") {
            " ⚠ 권장되지 않는 경로입니다"
        } else { "" };
        Paragraph::new(format!(
            "\n서버 저장 위치를 선택하세요.\n\n현재: {}{}\n\nDownloads, Desktop, OneDrive 동기화 폴더는 피하세요.\nEnter: 다음\n",
            self.wizard.location.display(), warn
        ))
        .block(Block::default().borders(Borders::ALL).title(" 4/8: 저장 위치 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_ram(&self) -> Paragraph {
        let total_ram_mb = sys_info::mem_info().map(|m| m.total / 1024).unwrap_or(8192);
        let total_ram_gb = total_ram_mb as f32 / 1024.0;
        let recommended_mb = (total_ram_mb / 2).min(8192);
        let recommended_gb = recommended_mb as f32 / 1024.0;
        Paragraph::new(format!(
            "\n시스템 RAM: {:.1}GB\n권장 상한: {:.1}GB (최대 8GB)\n최소 권장: 2GB\n\n할당할 RAM: {:.1}GB\n\n←/→: 0.5GB 감소/증가 | 위/아래: 0.5GB 조정 | Enter: 다음\n",
            total_ram_gb, recommended_gb, self.wizard.ram_mb as f32 / 1024.0
        ))
        .block(Block::default().borders(Borders::ALL).title(" 5/8: RAM 설정 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_port(&self) -> Paragraph {
        Paragraph::new(format!(
            "\n서버 포트를 설정하세요.\n\n기본 포트: 25565\n현재: {}\n\n포트가 사용 중이면 다음 사용 가능 포트를 제안합니다.\n←/→: 감소/증가 | 위/아래: 조정 | Enter: 다음\n",
            self.wizard.port
        ))
        .block(Block::default().borders(Borders::ALL).title(" 6/8: 포트 설정 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_world(&self) -> Paragraph {
        Paragraph::new(format!(
            "\n월드 설정을 입력하세요.\n\n게임 모드: {}\n난이도: {}\n시드: {}\nMOTD: {}\n\nEnter: 다음\n",
            self.wizard.game_mode, self.wizard.difficulty,
            self.wizard.seed.as_deref().unwrap_or("(랜덤)"), self.wizard.motd
        ))
        .block(Block::default().borders(Borders::ALL).title(" 7/8: 월드 설정 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_wizard_confirm(&self) -> Paragraph {
        Paragraph::new(format!(
            "\n다음 설정으로 서버를 생성합니다.\n\n이름: {}\n종류: {:?}\n버전: {}\n위치: {}\nRAM: {:.1}GB\n포트: {}\n게임 모드: {}\n난이도: {}\n시드: {}\nMOTD: {}\n\nEnter로 생성 시작, ESC로 돌아가기\n",
            self.wizard.name, self.wizard.kind, self.wizard.version,
            self.wizard.location.display(), self.wizard.ram_mb as f32 / 1024.0, self.wizard.port,
            self.wizard.game_mode, self.wizard.difficulty,
            self.wizard.seed.as_deref().unwrap_or("(랜덤)"), self.wizard.motd
        ))
        .block(Block::default().borders(Borders::ALL).title(" 8/8: 최종 확인 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White))
    }

    fn draw_settings(&mut self, f: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .margin(1)
            .split(f.area());

        let header = Paragraph::new(" 설정 ")
            .block(Block::default().borders(Borders::ALL).title(" 설정 ").border_style(Style::default().fg(Color::Cyan)))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        f.render_widget(header, areas[0]);

        let paths = AppPaths::discover().ok();
        let config = paths.as_ref().and_then(|p| Config::load_or_create(p).ok());

        let items = vec![
            format!("  1. 서버 저장 위치: {}", config.as_ref().map(|c| c.servers_dir.display().to_string()).unwrap_or_else(|| "미설정".to_string())),
            format!("  2. Java 경로: {}", config.as_ref().and_then(|c| c.java_path.as_ref().map(|p| p.display().to_string())).unwrap_or_else(|| "자동 감지".to_string())),
            format!("  3. 기본 RAM: {}", format_ram(config.as_ref().map(|c| c.default_ram_mb).unwrap_or(2048))),
            format!("  4. Web UI: {}", if config.as_ref().map(|c| c.web_ui_enabled).unwrap_or(true) { "활성화" } else { "비활성화" }),
            format!("  5. Web UI 포트: {}", config.as_ref().map(|c| c.web_ui_port.to_string()).unwrap_or("8788".to_string())),
            format!("  6. 방화벽 자동 관리: {}", if config.as_ref().map(|c| c.firewall_managed).unwrap_or(true) { "활성화" } else { "비활성화" }),
            "  7. 프로그램 관리".to_string(),
        ];
        let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, text)| {
            let style = if self.settings_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(text.as_str(), style)))
        }).collect();

        let list = List::new(list_items)
            .block(Block::default().borders(Borders::ALL).title(" 설정 항목 ").border_style(Style::default().fg(Color::DarkGray)))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");
        f.render_stateful_widget(list, areas[1], &mut self.settings_list_state);

        let footer = Paragraph::new(" ↑/↓  이동    Enter  선택/편집    ESC  뒤로가기 ")
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(footer, areas[2]);
    }

    fn draw_program_management(&mut self, f: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .margin(1)
            .split(f.area());

        let version = env!("CARGO_PKG_VERSION");
        let header = Paragraph::new(format!(
            " golani v{} - Minecraft 서버 관리 도구\n\nRust + ratatui + axum\nGitHub: https://github.com/jongfrog/golani-install",
            version
        ))
        .block(Block::default().borders(Borders::ALL).title(" 프로그램 관리 ").border_style(Style::default().fg(Color::Green)))
        .style(Style::default().fg(Color::Green))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        f.render_widget(header, areas[0]);

        let items = vec![
            "  1. 프로그램 업데이트".to_string(),
            "  2. 프로그램 제거".to_string(),
        ];
        let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, text)| {
            let style = if self.menu_list_state.selected() == Some(i) {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(text.as_str(), style)))
        }).collect();

        let list = List::new(list_items)
            .block(Block::default().borders(Borders::ALL).title(" 프로그램 관리 ").border_style(Style::default().fg(Color::DarkGray)))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");
        f.render_stateful_widget(list, areas[1], &mut self.menu_list_state);

        let footer = Paragraph::new(" ↑/↓  이동    Enter  선택    ESC  뒤로가기 ")
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(footer, areas[2]);
    }

    fn draw_edit_path(&self, f: &mut Frame) {
        let area = centered_rect(80, 40, f.area());
        f.render_widget(Clear, area);

        let field_name = match self.get_edit_field_name() {
            EditField::ServersDir => "서버 저장 위치",
            EditField::JavaPath => "Java 경로",
            _ => "경로",
        };

        let widget = Paragraph::new(format!(
            "\n{} 경로를 입력하세요.\n\n현재: {}\n\nESC: 취소 | Enter: 저장\n",
            field_name,
            self.get_edit_current()
        ))
        .block(Block::default().borders(Borders::ALL).title(" 경로 편집 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));
        f.render_widget(widget, area);
    }

    fn draw_edit_number(&self, f: &mut Frame) {
        let area = centered_rect(60, 30, f.area());
        f.render_widget(Clear, area);

        let (field_name, unit) = match self.get_edit_field_name() {
            EditField::DefaultRam => ("기본 RAM", "GB"),
            EditField::WebUiPort => ("Web UI 포트", ""),
            _ => ("값", ""),
        };

        let (min, max, step) = self.get_edit_number_params();

        let widget = Paragraph::new(format!(
            "\n{}를 설정하세요.\n\n현재: {}{}\n범위: {} ~ {}{}\n단계: {}\n\n←/→: 조정 | 위/아래: 단계 조정 | Enter: 저장 | ESC: 취소\n",
            field_name,
            self.get_edit_current(),
            unit,
            min, max, unit,
            step
        ))
        .block(Block::default().borders(Borders::ALL).title(" 숫자 편집 ").border_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));
        f.render_widget(widget, area);
    }

    fn get_edit_field_name(&self) -> EditField {
        match &self.screen {
            Screen::EditPath { field, .. } => *field,
            Screen::EditNumber { field, .. } => *field,
            _ => EditField::ServersDir,
        }
    }

    fn get_edit_current(&self) -> String {
        match &self.screen {
            Screen::EditPath { current, .. } => current.clone(),
            Screen::EditNumber { current, .. } => current.clone(),
            _ => String::new(),
        }
    }

    fn get_edit_number_params(&self) -> (u32, u32, u32) {
        match &self.screen {
            Screen::EditNumber { min, max, step, .. } => (*min, *max, *step),
            _ => (512, 65536, 512),
        }
    }

    fn draw_server_console(&mut self, f: &mut Frame, idx: usize) {
        if idx >= self.servers.len() { return; }
        let server = &self.servers[idx];

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .margin(1)
            .split(f.area());

        let follow_indicator = if self.server_console_follow { " ● 실시간" } else { " ▮ 일시정지" };
        let header = Paragraph::new(format!(" {} - 서버 콘솔{} (F:팔로우 토글, ESC:나가기) ", server.name, follow_indicator))
            .block(Block::default().borders(Borders::ALL).title(" 콘솔/로그 ").border_style(Style::default().fg(Color::Green)))
            .style(Style::default().fg(Color::Green))
            .alignment(Alignment::Center);
        f.render_widget(header, areas[0]);

        // 보이는 줄 계산
        let available_height = areas[1].height.saturating_sub(2) as usize; // borders
        let lines: Vec<&str> = self.server_console_lines.iter().map(|s| s.as_str()).collect();
        
        let start = if self.server_console_follow || self.server_console_history_idx.is_none() {
            lines.len().saturating_sub(available_height)
        } else {
            self.server_console_history_idx.unwrap_or(0).saturating_sub(available_height / 2)
        };
        
        let end = (start + available_height).min(lines.len());
        let visible: Vec<Line> = lines[start..end].iter().map(|l| Line::from(*l)).collect();

        let console_widget = Paragraph::new(Text::from(visible))
            .block(Block::default().borders(Borders::ALL).title(" 출력 ").border_style(Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White));
        f.render_widget(console_widget, areas[1]);

        let footer = Paragraph::new(format!(" 명령: {}_  (Enter:전송, ↑↓:히스토리, PgUp/PgDn:스크롤, F:실시간토글) ", self.server_console_input))
            .block(Block::default().borders(Borders::ALL).title(" 입력 ").border_style(Style::default().fg(Color::Yellow)))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(footer, areas[2]);
    }

    fn draw_progress(&self, f: &mut Frame, msg: &str, progress: f64) {
        let area = f.area();
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" 진행 중 ").border_style(Style::default().fg(Color::Cyan)))
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
            .ratio(progress)
            .label(format!(" {} ({:.0}%) ", msg, progress * 100.0));
        let area = centered_rect(70, 25, area);
        f.render_widget(Clear, area);
        f.render_widget(gauge, area);
    }

    fn draw_error(&self, f: &mut Frame, msg: &str) {
        let area = f.area();
        let error = Paragraph::new(format!("\n오류: {}\n\nESC로 닫기\n", msg))
            .block(Block::default().borders(Borders::ALL).title(" 오류 ").border_style(Style::default().fg(Color::Red)))
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        let area = centered_rect(70, 30, area);
        f.render_widget(Clear, area);
        f.render_widget(error, area);
    }

    fn draw_confirm_delete(&self, f: &mut Frame, name: &str) {
        let area = f.area();
        let confirm = Paragraph::new(format!("\n정말 '{}' 서버를 삭제하시겠습니까? (y/N)\n\nEnter/y 확인 | ESC/n 취소\n", name))
            .block(Block::default().borders(Borders::ALL).title(" 확인 ").border_style(Style::default().fg(Color::Yellow)))
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        let area = centered_rect(70, 30, area);
        f.render_widget(Clear, area);
        f.render_widget(confirm, area);
    }
}

fn next_step(step: WizardStep) -> WizardStep {
    match step {
        WizardStep::Name => WizardStep::SelectKind,
        WizardStep::SelectKind => WizardStep::SelectVersion,
        WizardStep::SelectVersion => WizardStep::Location,
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
        WizardStep::SelectKind => WizardStep::Name,
        WizardStep::SelectVersion => WizardStep::SelectKind,
        WizardStep::Location => WizardStep::SelectVersion,
        WizardStep::Ram => WizardStep::Location,
        WizardStep::Port => WizardStep::Ram,
        WizardStep::World => WizardStep::Port,
        WizardStep::Confirm => WizardStep::World,
    }
}

fn format_ram(mb: u32) -> String {
    if mb >= 1024 {
        format!("{:.1}GB", mb as f32 / 1024.0)
    } else {
        format!("{}MB", mb)
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