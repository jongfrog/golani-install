use std::process::Command;
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirewallBackend {
    WindowsDefender,
    Ufw,
    Firewalld,
    Unknown,
}

pub fn detect_firewall_backend() -> FirewallBackend {
    if cfg!(windows) {
        return FirewallBackend::WindowsDefender;
    }
    // Check for ufw
    if Command::new("which").arg("ufw").output().map(|o| o.status.success()).unwrap_or(false) {
        return FirewallBackend::Ufw;
    }
    // Check for firewalld
    if Command::new("which").arg("firewall-cmd").output().map(|o| o.status.success()).unwrap_or(false) {
        return FirewallBackend::Firewalld;
    }
    FirewallBackend::Unknown
}

pub fn firewall_rule_name(server_name: &str, port: u16) -> String {
    format!("golani-{}-{}", server_name, port)
}

pub fn add_firewall_rule(server_name: &str, port: u16) -> Result<()> {
    let backend = detect_firewall_backend();
    let rule_name = firewall_rule_name(server_name, port);
    
    match backend {
        FirewallBackend::WindowsDefender => add_windows_rule(&rule_name, port),
        FirewallBackend::Ufw => add_ufw_rule(port),
        FirewallBackend::Firewalld => add_firewalld_rule(&rule_name, port),
        FirewallBackend::Unknown => {
            eprintln!("지원되지 않는 방화벽 환경입니다. 수동으로 포트 {}를 허용해주세요.", port);
            Ok(())
        }
    }
}

pub fn remove_firewall_rule(server_name: &str, port: u16) -> Result<()> {
    let backend = detect_firewall_backend();
    let rule_name = firewall_rule_name(server_name, port);
    
    match backend {
        FirewallBackend::WindowsDefender => remove_windows_rule(&rule_name),
        FirewallBackend::Ufw => remove_ufw_rule(port),
        FirewallBackend::Firewalld => remove_firewalld_rule(&rule_name, port),
        FirewallBackend::Unknown => Ok(()),
    }
}

fn add_windows_rule(rule_name: &str, port: u16) -> Result<()> {
    let output = Command::new("netsh")
        .args([
            "advfirewall", "firewall", "add", "rule",
            &format!("name={}", rule_name),
            "dir=in",
            "action=allow",
            "protocol=TCP",
            &format!("localport={}", port),
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 관리자 권한 필요 시 안내
        if stderr.contains("액세스가 거부") || stderr.contains("Access is denied") || stderr.contains("740") {
            return Err(Error::ServerCreation(format!(
                "방화벽 규칙 추가를 위해 관리자 권한이 필요합니다.\n다음 명령을 관리자 PowerShell에서 실행하세요:\nnetsh advfirewall firewall add rule name=\"{}\" dir=in action=allow protocol=TCP localport={}",
                rule_name, port
            )));
        }
        return Err(Error::ServerCreation(format!("방화벽 규칙 추가 실패: {}", stderr)));
    }
    Ok(())
}

fn remove_windows_rule(rule_name: &str) -> Result<()> {
    let output = Command::new("netsh")
        .args([
            "advfirewall", "firewall", "delete", "rule",
            &format!("name={}", rule_name),
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("액세스가 거부") || stderr.contains("Access is denied") {
            return Err(Error::ServerCreation(format!(
                "방화벽 규칙 삭제를 위해 관리자 권한이 필요합니다."
            )));
        }
    }
    Ok(())
}

fn add_ufw_rule(port: u16) -> Result<()> {
    let output = Command::new("ufw")
        .args(["allow", &format!("{}/tcp", port)])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::ServerCreation(format!("UFW 규칙 추가 실패: {}", stderr)));
    }
    Ok(())
}

fn remove_ufw_rule(port: u16) -> Result<()> {
    let output = Command::new("ufw")
        .args(["delete", "allow", &format!("{}/tcp", port)])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("Rule not found") && !stderr.contains("존재하지 않") {
            // 조용히 무시 (규칙이 이미 없을 수 있음)
        }
    }
    Ok(())
}

fn add_firewalld_rule(_rule_name: &str, port: u16) -> Result<()> {
    // 영구 규칙 추가
    let output = Command::new("firewall-cmd")
        .args([
            "--permanent",
            "--add-port", &format!("{}/tcp", port),
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::ServerCreation(format!("firewalld 규칙 추가 실패: {}", stderr)));
    }
    
    // 즉시 적용
    let output = Command::new("firewall-cmd")
        .args(["--reload"])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::ServerCreation(format!("firewalld 리로드 실패: {}", stderr)));
    }
    Ok(())
}

fn remove_firewalld_rule(_rule_name: &str, port: u16) -> Result<()> {
    let output = Command::new("firewall-cmd")
        .args([
            "--permanent",
            "--remove-port", &format!("{}/tcp", port),
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("not found") && !stderr.contains("존재하지 않") {
            // 조용히 무시
        }
    }
    
    let _ = Command::new("firewall-cmd")
        .args(["--reload"])
        .output();
    
    Ok(())
}

pub fn check_port_listening(port: u16) -> bool {
    std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
}

pub fn get_local_ips() -> Vec<String> {
    let mut ips = vec!["127.0.0.1".to_string()];
    
    if let Ok(output) = Command::new(if cfg!(windows) { "ipconfig" } else { "ip" })
        .args(if cfg!(windows) { vec![] } else { vec!["addr", "show"] })
        .output() 
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("IPv4") || line.contains("inet ") {
                if let Some(ip) = extract_ip(line) {
                    if ip != "127.0.0.1" && !ip.starts_with("169.254") {
                        ips.push(ip);
                    }
                }
            }
        }
    }
    ips
}

fn extract_ip(line: &str) -> Option<String> {
    // IPv4 주소 추출
    for part in line.split_whitespace() {
        if part.contains('.') && part.split('.').count() == 4 {
            if part.parse::<std::net::Ipv4Addr>().is_ok() {
                return Some(part.to_string());
            }
        }
    }
    None
}

pub fn diagnose_connection(port: u16) -> Vec<String> {
    let mut issues = Vec::new();
    
    // 1. 서버 프로세스 확인
    // (이미 상위에서 처리됨)
    
    // 2. 포트 리스닝 확인
    if !check_port_listening(port) {
        issues.push(format!("포트 {}에서 리스닝 중이지 않음", port));
    }
    
    // 3. 방화벽 규칙 확인 (Windows)
    if cfg!(windows) {
        if let Ok(output) = Command::new("netsh")
            .args(["advfirewall", "firewall", "show", "rule", "name=golani-*"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if !text.contains(&format!("{}", port)) {
                issues.push(format!("Windows 방화벽에 포트 {} 허용 규칙 없음", port));
            }
        }
    }
    
    // 4. 로컬 IP 확인
    let ips = get_local_ips();
    if ips.len() <= 1 {
        issues.push("외부 접속 가능한 로컬 IP를 찾을 수 없음".to_string());
    }
    
    if issues.is_empty() {
        issues.push("모든 진단 항목 통과".to_string());
    }
    
    issues
}