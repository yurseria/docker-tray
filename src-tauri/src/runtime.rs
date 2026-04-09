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

/// Check if a Docker socket is available
pub fn docker_socket_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ID}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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
    // Check if external Docker is running
    if docker_socket_available() {
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

/// Start the bundled Colima runtime
pub fn start_builtin(resource_dir: &PathBuf) -> Result<String, String> {
    let colima = bundled_colima(resource_dir)
        .ok_or("Bundled Colima not found")?;

    let env = colima_env(resource_dir);

    let output = Command::new(&colima)
        .args(["start", "--cpu", "2", "--memory", "2", "--disk", "20", "--runtime", "docker"])
        .envs(env)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!("{}{}", stdout, stderr));
    }

    Ok(format!("{}{}", stdout, stderr))
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

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!("{}{}", stdout, stderr));
    }

    Ok(format!("{}{}", stdout, stderr))
}
