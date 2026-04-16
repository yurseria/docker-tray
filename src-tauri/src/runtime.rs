use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum RuntimeKind {
    External,  // Docker socket already available (Docker Desktop, OrbStack, etc.)
    Builtin,   // Bundled Colima
    None,      // No runtime detected
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub kind: RuntimeKind,
    pub running: bool,
    pub message: String,
}

/// Check if a non-Colima Docker socket is available (Docker Desktop, OrbStack, etc.)
pub fn external_docker_available() -> bool {
    // Check standard socket locations (not Colima's)
    let standard_sockets = [
        "/var/run/docker.sock",
        // OrbStack
        &format!("{}/.orbstack/run/docker.sock", dirs::home_dir().unwrap_or_default().display()),
        // Docker Desktop
        &format!("{}/.docker/run/docker.sock", dirs::home_dir().unwrap_or_default().display()),
    ];

    for sock in &standard_sockets {
        if std::path::Path::new(sock).exists() {
            // Verify it's actually working
            let result = Command::new("docker")
                .args(["info", "--format", "{{.ID}}"])
                .env("DOCKER_HOST", format!("unix://{}", sock))
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if result {
                return true;
            }
        }
    }
    false
}


/// Get the path to Colima binary — bundled first, then detect from running process
fn bundled_colima(resource_dir: &PathBuf) -> Option<PathBuf> {
    // 1. Check bundled binary in resource dir
    let colima = resource_dir.join("runtime/colima/bin/colima");
    if colima.exists() {
        return Some(colima);
    }

    // 2. Detect from running colima process (for dev mode / installed app mismatch)
    //    macOS pgrep -a doesn't show command, so use ps instead
    if let Ok(output) = Command::new("ps")
        .args(["-eo", "command"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("colima daemon") {
                if let Some(path) = line.split_whitespace().next() {
                    let p = PathBuf::from(path);
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
    }

    // 3. Check installed app location (for dev mode where resource_dir differs)
    let installed = PathBuf::from(
        "/Applications/Docker Tray.app/Contents/Resources/runtime/colima/bin/colima",
    );
    if installed.exists() {
        return Some(installed);
    }

    // 4. Check common system paths
    for path in &["/opt/homebrew/bin/colima", "/usr/local/bin/colima"] {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

/// Resolve the runtime base dir from a Colima binary path
/// e.g. .../runtime/colima/bin/colima → .../runtime
fn runtime_base_from_colima(colima_path: &Path) -> Option<PathBuf> {
    colima_path.parent()?.parent()?.parent().map(PathBuf::from)
}

/// Build environment variables for Colima, derived from the known binary path
fn colima_env_for(colima_path: &Path) -> Vec<(String, String)> {
    let runtime_base = runtime_base_from_colima(colima_path)
        .unwrap_or_else(|| PathBuf::from("/usr/local"));

    let lima_dir = runtime_base.join("lima");
    let mut env = vec![];

    env.push(("PATH".to_string(), format!(
        "{}:{}:{}:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin",
        lima_dir.join("bin").display(),
        runtime_base.join("colima/bin").display(),
        runtime_base.join("docker/bin").display(),
    )));
    env.push(("LIMA_HOME".to_string(),
        dirs::home_dir().unwrap_or_default().join(".lima").to_string_lossy().to_string(),
    ));
    env.push(("LIMA_DIR".to_string(), lima_dir.to_string_lossy().to_string()));

    env
}

/// colima_env kept for backward compat with start_builtin (finds binary itself)
fn colima_env(resource_dir: &PathBuf) -> Vec<(String, String)> {
    if let Some(colima) = bundled_colima(resource_dir) {
        return colima_env_for(&colima);
    }
    // Fallback: use resource_dir layout
    let lima_dir = resource_dir.join("runtime/lima");
    let mut env = vec![];
    env.push(("PATH".to_string(), format!(
        "{}:{}:{}:/usr/local/bin:/usr/bin:/bin",
        lima_dir.join("bin").display(),
        resource_dir.join("runtime/colima/bin").display(),
        resource_dir.join("runtime/docker/bin").display(),
    )));
    env.push(("LIMA_HOME".to_string(),
        dirs::home_dir().unwrap_or_default().join(".lima").to_string_lossy().to_string(),
    ));
    env.push(("LIMA_DIR".to_string(), lima_dir.to_string_lossy().to_string()));
    env
}

/// Detect current runtime status
pub fn detect_runtime(resource_dir: &PathBuf) -> RuntimeStatus {
    // Check if external Docker is running (Docker Desktop, OrbStack — not Colima)
    if external_docker_available() {
        return RuntimeStatus {
            kind: RuntimeKind::External,
            running: true,
            message: "External Docker runtime detected".to_string(),
        };
    }

    // Check if bundled Colima exists
    if bundled_colima(resource_dir).is_some() {
        // Check if Colima VM is running
        let running = is_colima_running(resource_dir);
        return RuntimeStatus {
            kind: RuntimeKind::Builtin,
            running,
            message: if running {
                "Built-in runtime (Colima) is running".to_string()
            } else {
                "Built-in runtime (Colima) is stopped".to_string()
            },
        };
    }

    RuntimeStatus {
        kind: RuntimeKind::None,
        running: false,
        message: "No Docker runtime found".to_string(),
    }
}

fn is_colima_running(resource_dir: &PathBuf) -> bool {
    let Some(colima) = bundled_colima(resource_dir) else {
        return false;
    };

    let env = colima_env(resource_dir);
    Command::new(&colima)
        .args(["status"])
        .envs(env)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the Colima docker socket path
pub fn colima_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".colima/default/docker.sock")
}

/// Try to connect to Docker, checking Colima socket as fallback
pub fn connect_docker() -> Option<Docker> {
    // Try default socket first
    if let Ok(client) = Docker::connect_with_local_defaults() {
        return Some(client);
    }
    // Try Colima socket
    let socket = colima_socket_path();
    if socket.exists() {
        let url = format!("unix://{}", socket.display());
        if let Ok(client) = Docker::connect_with_unix(&url, 120, bollard::API_DEFAULT_VERSION) {
            return Some(client);
        }
    }
    None
}

/// Extract a clean error message from Colima's verbose log output
fn extract_error(full: &str) -> String {
    let error_lines: Vec<&str> = full
        .lines()
        .filter(|l| l.contains("level=fatal") || l.contains("level=error"))
        .collect();
    let raw = if error_lines.is_empty() {
        full.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("Unknown error").to_string()
    } else {
        error_lines.last().unwrap_or(&"Unknown error").to_string()
    };
    // Extract just the msg="..." part if present
    if let Some(idx) = raw.find("msg=") {
        raw[idx + 4..].trim_matches('"').to_string()
    } else {
        raw
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub cpu: u32,
    pub memory: u32,
    pub disk: u32,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self { cpu: 2, memory: 4, disk: 20 }
    }
}

/// Read current VM config from Lima's YAML (authoritative) or Colima's YAML (fallback)
pub fn read_vm_config() -> VmConfig {
    let home = dirs::home_dir().unwrap_or_default();
    let mut config = VmConfig::default();

    // Try Lima config first (actual running VM values)
    let lima_config = home.join(".lima/colima/lima.yaml");
    if let Ok(content) = std::fs::read_to_string(&lima_config) {
        for line in content.lines() {
            if line.starts_with('#') { continue; }
            if let Some(val) = line.strip_prefix("cpus:") {
                if let Ok(v) = val.trim().parse::<u32>() { config.cpu = v; }
            } else if let Some(val) = line.strip_prefix("memory:") {
                // Lima format: "memory: 4096MiB"
                let val = val.trim().trim_end_matches("MiB").trim_end_matches("GiB");
                if let Ok(v) = val.parse::<u32>() {
                    config.memory = if v >= 1024 { v / 1024 } else { v };
                }
            } else if let Some(val) = line.strip_prefix("disk:") {
                // Lima format: "disk: 20GiB"
                let val = val.trim().trim_end_matches("GiB").trim_end_matches("MiB");
                if let Ok(v) = val.parse::<u32>() { config.disk = v; }
            }
        }
        return config;
    }

    // Fallback to Colima config
    let colima_config = home.join(".colima/default/colima.yaml");
    if let Ok(content) = std::fs::read_to_string(&colima_config) {
        for line in content.lines() {
            if line.starts_with('#') { continue; }
            if let Some(val) = line.strip_prefix("cpu:") {
                if let Ok(v) = val.trim().parse::<u32>() { config.cpu = v; }
            } else if let Some(val) = line.strip_prefix("memory:") {
                if let Ok(v) = val.trim().parse::<u32>() { config.memory = v; }
            } else if let Some(val) = line.strip_prefix("disk:") {
                if let Ok(v) = val.trim().parse::<u32>() { config.disk = v; }
            }
        }
    }

    config
}

/// Write VM config to both Colima and Lima config files
fn write_vm_config(config: &VmConfig) {
    let home = dirs::home_dir().unwrap_or_default();

    // 1. Update Colima config (~/.colima/default/colima.yaml)
    let colima_config = home.join(".colima/default/colima.yaml");
    if let Ok(content) = std::fs::read_to_string(&colima_config) {
        let mut lines: Vec<String> = content.lines().map(String::from).collect();
        for line in &mut lines {
            if !line.starts_with('#') {
                if line.starts_with("cpu:") {
                    *line = format!("cpu: {}", config.cpu);
                } else if line.starts_with("memory:") {
                    *line = format!("memory: {}", config.memory);
                } else if line.starts_with("disk:") {
                    *line = format!("disk: {}", config.disk);
                }
            }
        }
        let _ = std::fs::write(&colima_config, lines.join("\n") + "\n");
    }

    // 2. Update Lima config (~/.lima/colima/lima.yaml)
    //    Lima uses different format: cpus, memory in MiB, disk in GiB
    let lima_config = home.join(".lima/colima/lima.yaml");
    if let Ok(content) = std::fs::read_to_string(&lima_config) {
        let mut lines: Vec<String> = content.lines().map(String::from).collect();
        for line in &mut lines {
            if !line.starts_with('#') {
                if line.starts_with("cpus:") {
                    *line = format!("cpus: {}", config.cpu);
                } else if line.starts_with("memory:") {
                    *line = format!("memory: {}MiB", config.memory * 1024);
                } else if line.starts_with("disk:") {
                    *line = format!("disk: {}GiB", config.disk);
                }
            }
        }
        let _ = std::fs::write(&lima_config, lines.join("\n") + "\n");
    }
}

/// Start the bundled Colima runtime
pub fn start_builtin(resource_dir: &PathBuf) -> Result<String, String> {
    start_builtin_with_config(resource_dir, &VmConfig::default())
}

pub fn start_builtin_with_config(resource_dir: &PathBuf, config: &VmConfig) -> Result<String, String> {
    let colima = bundled_colima(resource_dir)
        .ok_or("Bundled Colima not found")?;

    // Update config files so existing VMs pick up the new values
    write_vm_config(config);

    // Derive env from the actual colima path (not resource_dir, which may be wrong in dev)
    let env = colima_env_for(&colima);
    let cpu = config.cpu.to_string();
    let mem = config.memory.to_string();
    let disk = config.disk.to_string();

    let output = Command::new(&colima)
        .args(["start", "--cpu", &cpu, "--memory", &mem, "--disk", &disk, "--runtime", "docker"])
        .envs(env.clone())
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let full = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // If start failed, try cleaning up corrupted state and retry once
        let _ = Command::new(&colima)
            .args(["delete", "--force"])
            .envs(env.clone())
            .output();

        let retry = Command::new(&colima)
            .args(["start", "--cpu", &cpu, "--memory", &mem, "--disk", &disk, "--runtime", "docker"])
            .envs(env)
            .output()
            .map_err(|e| e.to_string())?;

        if !retry.status.success() {
            return Err(extract_error(&full));
        }
    }

    // Create /var/run/docker.sock symlink so all tools find the socket
    create_docker_sock_symlink();

    Ok("Runtime started".to_string())
}

/// Create symlink from /var/run/docker.sock to Colima socket (requires sudo)
fn create_docker_sock_symlink() {
    let colima_sock = colima_socket_path();
    if !colima_sock.exists() {
        return;
    }
    let target = std::path::Path::new("/var/run/docker.sock");
    if target.exists() {
        return; // Already exists, don't overwrite
    }
    // Use osascript for admin privileges (GUI password dialog)
    let script = format!(
        r#"do shell script "ln -sf {} /var/run/docker.sock" with administrator privileges"#,
        colima_sock.display()
    );
    let _ = Command::new("osascript")
        .args(["-e", &script])
        .output();
}

/// Stop the bundled Colima runtime
pub fn stop_builtin(resource_dir: &PathBuf) -> Result<String, String> {
    let colima = bundled_colima(resource_dir)
        .ok_or("Bundled Colima not found")?;

    let env = colima_env_for(&colima);

    let output = Command::new(&colima)
        .args(["stop"])
        .envs(env)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let full = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(extract_error(&full));
    }

    Ok("Runtime stopped".to_string())
}
