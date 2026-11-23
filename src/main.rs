use std::io::{IsTerminal, Write};

use golani::{
    java::{find_system_java, install_managed_java, required_version, find_java_with_config},
    minecraft::{create_server, MinecraftApi, ServerKind},
    server::{
        delete, latest_log_tail, load_metadata, rcon_settings, recorded_running_pid, rename,
        restart, start, stop, update_server,
    },
    Application,
};

#[tokio::main]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(arguments).await {
        eprintln!("golani를 시작할 수 없습니다: {error}");
        std::process::exit(1);
    }
}

async fn handle_settings(
    config: &golani::config::Config,
    paths: &golani::paths::AppPaths,
    args: &[String],
) -> golani::error::Result<()> {
    if args.is_empty() {
        println!("=== golani 설정 ===");
        println!("서버 저장 위치: {}", config.servers_dir.display());
        if let Some(java_path) = &config.java_path {
            println!("수동 Java 경로: {}", java_path.display());
        } else {
            println!("수동 Java 경로: (자동 감지)");
        }
        println!("\n사용법:");
        println!("  golani settings path <새_경로>     # 서버 저장 위치 변경 (마이그레이션 포함)");
        println!("  golani settings java <경로>        # 수동 Java 경로 지정");
        println!("  golani settings java --clear       # 수동 Java 경로 해제 (자동 감지 사용)");
        return Ok(());
    }

    match args[0].as_str() {
        "path" => {
            if args.len() < 2 {
                eprintln!("사용법: golani settings path <새_경로>");
                return Ok(());
            }
            let new_path = std::path::PathBuf::from(&args[1]);
            if !new_path.is_absolute() {
                eprintln!("절대 경로를 입력하세요.");
                return Ok(());
            }
            
            // 마이그레이션 확인
            println!("서버 저장 위치를 변경합니다:");
            println!("  기존: {}", config.servers_dir.display());
            println!("  신규: {}", new_path.display());
            println!("\n기존 서버 데이터가 새로운 위치로 이동됩니다. 계속하시겠습니까? (y/N)");
            
            use std::io::{self, Write};
            print!("> ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("취소되었습니다.");
                return Ok(());
            }
            
            // 마이그레이션 수행
            println!("데이터 이동 중...");
            let old_dir = config.servers_dir.clone();
            if old_dir.exists() {
                std::fs::create_dir_all(&new_path)?;
                for entry in std::fs::read_dir(&old_dir)? {
                    let entry = entry?;
                    let dest = new_path.join(entry.file_name());
                    std::fs::rename(entry.path(), &dest)?;
                    println!("  이동: {}", entry.file_name().to_string_lossy());
                }
            }
            
            // 설정 업데이트
            let mut new_config = config.clone();
            new_config.servers_dir = new_path.clone();
            new_config.save(paths)?;
            
            println!("서버 저장 위치가 변경되었습니다: {}", new_path.display());
            println!("golani를 재시작하면 새 위치가 적용됩니다.");
            Ok(())
        }
        "java" => {
            if args.len() < 2 {
                eprintln!("사용법: golani settings java <경로> | --clear");
                return Ok(());
            }
            if args[1] == "--clear" {
                let mut new_config = config.clone();
                new_config.java_path = None;
                new_config.save(paths)?;
                println!("수동 Java 경로가 해제되었습니다. 자동 감지를 사용합니다.");
                Ok(())
            } else {
                let java_path = std::path::PathBuf::from(&args[1]);
                if !java_path.exists() {
                    eprintln!("지정된 경로가 존재하지 않습니다: {}", java_path.display());
                    return Ok(());
                }
                // java 실행 파일 검증
                let java_exe = if cfg!(windows) { java_path.join("java.exe") } else { java_path.join("java") };
                if !java_exe.exists() {
                    eprintln!("Java 실행 파일을 찾을 수 없습니다: {}", java_exe.display());
                    return Ok(());
                }
                let mut new_config = config.clone();
                new_config.java_path = Some(java_path.clone());
                new_config.save(paths)?;
                println!("수동 Java 경로가 설정되었습니다: {}", java_path.display());
                Ok(())
            }
        }
        _ => {
            eprintln!("알 수 없는 설정: {}", args[0]);
            eprintln!("사용법: golani settings [path|java]");
            Ok(())
        }
    }
}

async fn handle_self_update(_args: &[String]) -> golani::error::Result<()> {
    println!("golani 자체 업데이트를 확인합니다...");
    
    const GITHUB_API: &str = "https://api.github.com/repos/nousresearch/golani/releases/latest";
    const USER_AGENT: &str = "golani/0.1.0 (https://golani.kro.kr)";
    
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;
    
    let response = client.get(GITHUB_API).send().await?;
    if !response.status().is_success() {
        return Err(golani::error::Error::ServerCreation("GitHub API 요청 실패".into()));
    }
    
    let release: serde_json::Value = response.json().await?;
    let tag_name = release["tag_name"].as_str().unwrap_or("unknown");
    
    println!("최신 버전: {}", tag_name);
    println!("현재 버전: 0.1.0");
    
    // 버전 비교 (간단한 문자열 비교)
    if tag_name == "v0.1.0" || tag_name == "0.1.0" {
        println!("이미 최신 버전입니다.");
        return Ok(());
    }
    
    // 플랫폼별 자산 찾기
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    let asset_pattern = format!("golani-{}-{}", os, arch);
    
    let empty_vec: Vec<serde_json::Value> = Vec::new();
    let assets = release["assets"].as_array().unwrap_or(&empty_vec);
    let asset = assets.iter().find(|a| {
        a["name"].as_str().unwrap_or("").contains(&asset_pattern)
    });
    
    let asset = match asset {
        Some(a) => a,
        None => {
            return Err(golani::error::Error::ServerCreation(
                format!("플랫폼 {}-{}에 맞는 자산을 찾을 수 없습니다", os, arch)
            ));
        }
    };
    
    let download_url = asset["browser_download_url"].as_str().unwrap();
    let asset_name = asset["name"].as_str().unwrap();
    
    println!("다운로드 중: {}", asset_name);
    
    // 현재 실행 파일 경로
    let current_exe = std::env::current_exe()?;
    let backup_exe = current_exe.with_extension("exe.backup");
    
    // 백업 생성
    std::fs::copy(&current_exe, &backup_exe)?;
    println!("기존 실행 파일 백업: {}", backup_exe.display());
    
    // 다운로드
    let response = client.get(download_url).send().await?;
    let bytes = response.bytes().await?;
    
    // 임시 파일에 저장
    let temp_exe = current_exe.with_extension("exe.new");
    std::fs::write(&temp_exe, &bytes)?;
    
    // 교체 (Windows에서는 실행 중인 파일 교체 불가이므로 안내)
    #[cfg(windows)]
    {
        println!("Windows에서는 실행 중인 파일 교체가 제한됩니다.");
        println!("다음 명령으로 수동 교체하세요:");
        println!("  move /Y \"{}\" \"{}\"", temp_exe.display(), current_exe.display());
        println!("또는 프로그램을 종료한 후:");
        println!("  copy /Y \"{}\" \"{}\"", temp_exe.display(), current_exe.display());
    }
    
    #[cfg(not(windows))]
    {
        std::fs::rename(&temp_exe, &current_exe)?;
        println!("업데이트 완료! 골라니를 재시작하세요.");
    }
    
    // 백업 정리 (Windows에서 수동 교체 후)
    let _ = std::fs::remove_file(&backup_exe);
    
    Ok(())
}

async fn run(arguments: Vec<String>) -> golani::error::Result<()> {
    if arguments.first().map(String::as_str) == Some("versions") {
        let api = MinecraftApi::new()?;
        let versions = match arguments.get(1).map(String::as_str) {
            Some("vanilla") => api.vanilla_versions().await?,
            Some("paper") => api.paper_versions().await?,
            _ => {
                eprintln!("사용법: golani versions <vanilla|paper>");
                return Ok(());
            }
        };
        for version in versions {
            println!("{version}");
        }
        return Ok(());
    }
    match Application::initialize() {
        Ok(application) => {
            if arguments.first().map(String::as_str) == Some("attach") {
                let name = arguments.get(1).ok_or_else(|| golani::error::Error::ServerCreation("서버 이름이 필요합니다".into()))?;
                let server_dir = application.config.servers_dir.join(name);
                let (port, password) = rcon_settings(&server_dir).ok_or_else(|| golani::error::Error::ServerCreation("RCON 설정을 찾을 수 없습니다".into()))?;
                if recorded_running_pid(&server_dir).is_none() { return Err(golani::error::Error::ServerCreation("실행 중인 서버를 찾을 수 없습니다".into())); }

                println!("콘솔 연결됨. ESC로 연결 해제 (서버는 계속 실행), ↑↓로 명령 히스토리");
                println!("{}", "─".repeat(60));

                // 실시간 로그 스트리밍 태스크 시작
                let server_dir_clone = server_dir.clone();
                let log_handle = tokio::task::spawn_blocking(move || {
                    let _ = golani::server::follow_log(&server_dir_clone, |line| {
                        println!("{}", line);
                    }, || false);
                });

                // 대화형 명령 입력 루프
                use crossterm::event::{self, Event, KeyCode, KeyEventKind};
                use std::collections::VecDeque;

                let mut history: VecDeque<String> = VecDeque::with_capacity(100);
                let mut history_index: Option<usize> = None;
                let mut input_buffer = String::new();
                let mut should_exit = false;

                crossterm::terminal::enable_raw_mode()?;
                let _guard = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);

                while !should_exit {
                    // 키 이벤트 폴링 (논블로킹)
                    if event::poll(std::time::Duration::from_millis(50))? {
                        if let Event::Key(key) = event::read()? {
                            if key.kind == KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Esc => {
                                        should_exit = true;
                                    }
                                    KeyCode::Enter => {
                                        let cmd = input_buffer.trim().to_string();
                                        if !cmd.is_empty() {
                                            history.push_back(cmd.clone());
                                            if history.len() > 100 { history.pop_front(); }
                                            history_index = None;
                                            input_buffer.clear();

                                            // RCON으로 명령 전송
                                            let password_clone = password.clone();
                                            let port_clone = port;
                                            let cmd_clone = cmd.clone();
                                            tokio::spawn(async move {
                                                if let Ok(output) = golani::rcon::command(&format!("127.0.0.1:{}", port_clone), &password_clone, &cmd_clone).await {
                                                    if !output.is_empty() {
                                                        println!("\r{output}");
                                                    } else {
                                                        println!("\r명령을 전송했습니다.");
                                                    }
                                                } else {
                                                    eprintln!("\r명령 전송 실패");
                                                }
                                            });
                                        } else {
                                            input_buffer.clear();
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        if !input_buffer.is_empty() {
                                            input_buffer.pop();
                                            print!("\r> {}\x1b[K", input_buffer);
                                            std::io::stdout().flush()?;
                                        }
                                    }
                                    KeyCode::Up => {
                                        if !history.is_empty() {
                                            let idx = history_index.map_or(history.len() - 1, |i| i.saturating_sub(1));
                                            history_index = Some(idx);
                                            input_buffer = history[idx].clone();
                                            print!("\r> {}\x1b[K", input_buffer);
                                            std::io::stdout().flush()?;
                                        }
                                    }
                                    KeyCode::Down => {
                                        if let Some(idx) = history_index {
                                            if idx + 1 < history.len() {
                                                history_index = Some(idx + 1);
                                                input_buffer = history[idx + 1].clone();
                                            } else {
                                                history_index = None;
                                                input_buffer.clear();
                                            }
                                            print!("\r> {}\x1b[K", input_buffer);
                                            std::io::stdout().flush()?;
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        input_buffer.push(c);
                                        print!("\r> {}\x1b[K", input_buffer);
                                        std::io::stdout().flush()?;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                crossterm::terminal::disable_raw_mode()?;
                crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;

                log_handle.abort();
                println!("\n콘솔 연결을 해제했습니다. 서버는 계속 실행됩니다.");
                return Ok(());
            }
            if arguments.first().map(String::as_str) == Some("delete") {
                let name = arguments.get(1).ok_or_else(|| {
                    golani::error::Error::ServerCreation("서버 이름이 필요합니다".into())
                })?;
                let confirmation = arguments.get(2).map(String::as_str).unwrap_or("");
                delete(&application.config.servers_dir.join(name), confirmation)?;
                println!("서버를 삭제했습니다.");
                return Ok(());
            }
            if arguments.first().map(String::as_str) == Some("web") {
                println!("Web UI를 시작합니다: http://127.0.0.1:8788");
                golani::web::serve(golani::web::WebState {
                    servers_dir: application.config.servers_dir.clone(),
                    java_dir: application.paths.java_dir(),
                })
                .await?;
                return Ok(());
            }
            if arguments.first().map(String::as_str) == Some("rename") {
                let old = arguments.get(1).ok_or_else(|| {
                    golani::error::Error::ServerCreation("현재 서버 이름이 필요합니다".into())
                })?;
                let new = arguments.get(2).ok_or_else(|| {
                    golani::error::Error::ServerCreation("새 서버 이름이 필요합니다".into())
                })?;
                let destination = rename(&application.config.servers_dir.join(old), new)?;
                println!(
                    "서버 이름을 변경했습니다: {}",
                    destination
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                return Ok(());
            }
            if arguments.first().map(String::as_str) == Some("java") {
                let version = arguments.get(1).map(String::as_str).unwrap_or("1.21.1");
                let required = required_version(version);
                let java = match find_system_java(required) {
                    Some(java) => java,
                    None => install_managed_java(&application.paths, required).await?,
                };
                println!(
                    "Java {} 준비 완료: {}",
                    java.feature,
                    java.executable.display()
                );
                return Ok(());
            }
            if arguments.first().map(String::as_str) == Some("settings") {
                return handle_settings(&application.config, &application.paths, &arguments[1..]).await;
            }
            if arguments.first().map(String::as_str) == Some("update") {
                return handle_self_update(&arguments[1..]).await;
            }
            if matches!(
                arguments.first().map(String::as_str),
                Some("start" | "stop" | "restart" | "status" | "logs" | "update")
            ) {
                let name = arguments.get(1).ok_or_else(|| {
                    golani::error::Error::ServerCreation("서버 이름이 필요합니다".into())
                })?;
                let server_dir = application.config.servers_dir.join(name);
                match arguments[0].as_str() {
                    "status" => match recorded_running_pid(&server_dir) {
                        Some(pid) => println!("실행 중 (PID: {pid})"),
                        None => println!("정지"),
                    },
                    "stop" => {
                        stop(&server_dir).await?;
                        println!("서버를 중지했습니다.");
                    }
                    "logs" => {
                        let follow = arguments.iter().any(|a| a == "-f" || a == "--follow");
                        let lines = arguments
                            .iter()
                            .filter(|a| !a.starts_with('-'))
                            .nth(2)
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(100);

                        if follow {
                            println!("실시간 로그 모드 (Ctrl+C로 종료)");
                            println!("{}", "─".repeat(60));
                            let _ = golani::server::follow_log(&server_dir, |line| {
                                println!("{}", line);
                            }, || {
                                // Ctrl+C 감지를 위해 별도 처리 필요
                                false
                            });
                        } else {
                            println!("{}", latest_log_tail(&server_dir, lines)?);
                        }
                    }
                    "start" => {
                                            let metadata = load_metadata(&server_dir)?;
                                            let java = find_java_with_config(
                                                application.config.java_path.as_deref(),
                                                &application.paths,
                                                metadata.java_version,
                                            ).ok_or_else(|| golani::error::Error::ServerCreation("호환되는 Java를 찾을 수 없습니다. golani java <버전>을 먼저 실행하세요.".into()))?;
                                            let running = start(&server_dir, &java.executable).await?;
                                            println!("서버를 시작했습니다 (PID: {})", running.pid);
                                        }
                                        "restart" => {
                        let metadata = load_metadata(&server_dir)?;
                        let java = find_java_with_config(
                            application.config.java_path.as_deref(),
                            &application.paths,
                            metadata.java_version,
                        ).ok_or_else(|| {
                            golani::error::Error::ServerCreation(
                                "호환되는 Java를 찾을 수 없습니다".into(),
                            )
                        })?;
                        let running = restart(&server_dir, &java.executable).await?;
                        println!("서버를 재시작했습니다 (PID: {})", running.pid);
                    }
                    "update" => {
                        update_server(&server_dir).await?;
                        println!("서버를 업데이트했습니다. 재시작이 필요합니다.");
                    }
                    _ => unreachable!(),
                }
                return Ok(());
            }
            if arguments.first().map(String::as_str) == Some("new") {
                if arguments.len() < 4 {
                    eprintln!("사용법: golani new <vanilla|paper> <이름> <버전> [RAM_MB] [포트]");
                    return Ok(());
                }
                let kind = match arguments[1].as_str() {
                    "vanilla" => ServerKind::Vanilla,
                    "paper" => ServerKind::Paper,
                    _ => {
                        eprintln!("서버 종류는 vanilla 또는 paper여야 합니다.");
                        return Ok(());
                    }
                };
                let ram_mb = arguments
                    .get(4)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2048);
                let port = arguments
                    .get(5)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(25565);
                let metadata = create_server(
                    &MinecraftApi::new()?,
                    &application.config.servers_dir,
                    &arguments[2],
                    kind,
                    &arguments[3],
                    ram_mb,
                    port,
                    "golani Minecraft Server",
                    "survival",
                    "easy",
                    None,
                )
                .await?;
                println!(
                    "서버를 생성했습니다: {} (127.0.0.1:{})",
                    metadata.name, metadata.port
                );
                return Ok(());
            }
            if arguments.is_empty() && std::io::stdin().is_terminal() {
                return golani::tui::run(&application.config.servers_dir);
            }
            println!("golani 초기화가 완료되었습니다.");
            println!("데이터 디렉터리: {}", application.paths.root().display());
            println!(
                "서버 저장 위치: {}",
                application.config.servers_dir.display()
            );
        }
        Err(error) => return Err(error),
    }
    Ok(())
}
