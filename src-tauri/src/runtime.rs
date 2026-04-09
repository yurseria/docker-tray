use bollard::Docker;
use serde::Serialize;
use std::path::PathBuf;
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


/// Get the path to bundled Colima binary
fn bundled_colima(resource_dir: &PathBuf) -> Option<PathBuf> {
    let colima = resource_dir.join("runtime/colima/bin/colima");
    if colima.exists() {
        Some(colima)
    } else {
        None
    }
}

/// Build environment variables for bundled Colima to find Lima
fn colima_env(resource_dir: &PathBuf) -> Vec<(String, String)> {
    let lima_dir = resource_dir.join("runtime/lima");
    let mut env = vec![];

    // Colima needs to find limactl and docker
    let path = format!(
        "{}:{}:{}:/usr/local/bin:/usr/bin:/bin",
        lima_dir.join("bin").display(),
        resource_dir.join("runtime/colima/bin").display(),
        resource_dir.join("runtime/docker/bin").display(),
    );
    env.push(("PATH".to_string(), path));

    // Lima needs to find its share directory (guest agents, templates)
    env.push((
        "LIMA_HOME".to_string(),
        dirs::home_dir()
            .unwrap_or_default()
            .join(".lima")
            .to_string_lossy()
            .to_string(),
    ));

    // Tell Lima where its data files are
    env.push((
        "LIMA_DIR".to_string(),
        lima_dir.to_string_lossy().to_string(),
    ));

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

/// Start the bundled Colima runtime
pub fn start_builtin(resource_dir: &PathBuf) -> Result<String, String> {
    let colima = bundled_colima(resource_dir)
        .ok_or("Bundled Colima not found")?;

    let env = colima_env(resource_dir);

    let output = Command::new(&colima)
        .args(["start", "--cpu", "2", "--memory", "4", "--disk", "20", "--runtime", "docker"])
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
            .args(["start", "--cpu", "2", "--memory", "4", "--disk", "20", "--runtime", "docker"])
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

    let env = colima_env(resource_dir);

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
