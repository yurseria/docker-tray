use crate::provider::ProviderKind;
use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum RuntimeKind {
    External, // Docker socket already available (Docker Desktop, OrbStack, etc.)
    Builtin,  // Bundled Colima
    Apple,    // Apple's native `container` CLI
    None,     // No runtime detected
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub kind: RuntimeKind,
    pub running: bool,
    pub message: String,
    pub provider: ProviderKind,
}

/// Check if a non-Colima Docker socket is available (Docker Desktop, OrbStack, etc.)
pub fn external_docker_available() -> bool {
    // Check standard socket locations (not Colima's)
    let standard_sockets = [
        "/var/run/docker.sock",
        // OrbStack
        &format!(
            "{}/.orbstack/run/docker.sock",
            dirs::home_dir().unwrap_or_default().display()
        ),
        // Docker Desktop
        &format!(
            "{}/.docker/run/docker.sock",
            dirs::home_dir().unwrap_or_default().display()
        ),
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
    if let Ok(output) = Command::new("ps").args(["-eo", "command"]).output() {
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
    let installed =
        PathBuf::from("/Applications/Docker Tray.app/Contents/Resources/runtime/colima/bin/colima");
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

/// Locate Homebrew even when the app was launched from Finder and inherited a
/// minimal PATH.
fn homebrew() -> Option<PathBuf> {
    for path in &["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    Command::new("sh")
        .args(["-lc", "command -v brew"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (!path.is_empty()).then(|| PathBuf::from(path))
        })
}

fn command_error(action: &str, output: &Output) -> String {
    let details = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let details = details.trim();

    if details.is_empty() {
        format!("{action} failed with status {}", output.status)
    } else {
        format!("{action} failed: {details}")
    }
}

/// Install a usable local runtime when a development or slim app bundle does
/// not contain Colima. This is also used by the Start Runtime button.
fn install_colima() -> Result<PathBuf, String> {
    let brew = homebrew().ok_or(
        "Colima is not bundled and Homebrew was not found. Install Homebrew, then try again.",
    )?;

    let mut formulas = Vec::new();
    for formula in ["colima", "docker"] {
        let installed = Command::new(&brew)
            .args(["list", "--formula", formula])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !installed {
            formulas.push(formula);
        }
    }

    if !formulas.is_empty() {
        let output = Command::new(&brew)
            .arg("install")
            .args(&formulas)
            .output()
            .map_err(|error| format!("Could not run Homebrew: {error}"))?;
        if !output.status.success() {
            return Err(command_error("Installing Colima", &output));
        }
    }

    bundled_colima(&PathBuf::new()).ok_or_else(|| {
        "Colima was installed, but its executable could not be found. Restart the app and try again."
            .to_string()
    })
}

fn ensure_colima(resource_dir: &PathBuf) -> Result<PathBuf, String> {
    bundled_colima(resource_dir).map_or_else(install_colima, Ok)
}

/// Resolve the runtime base dir from a Colima binary path
/// e.g. .../runtime/colima/bin/colima → .../runtime
fn runtime_base_from_colima(colima_path: &Path) -> Option<PathBuf> {
    colima_path.parent()?.parent()?.parent().map(PathBuf::from)
}

/// Build environment variables for Colima, derived from the known binary path
fn colima_env_for(colima_path: &Path) -> Vec<(String, String)> {
    let mut env = vec![(
        "LIMA_HOME".to_string(),
        dirs::home_dir()
            .unwrap_or_default()
            .join(".lima")
            .to_string_lossy()
            .to_string(),
    )];

    // A bundled Colima lives in runtime/colima/bin and has matching sibling
    // lima/docker directories. Homebrew Colima must use Homebrew's own paths;
    // setting LIMA_DIR to a guessed bundle path prevents it from starting.
    if let Some(runtime_base) = runtime_base_from_colima(colima_path).filter(|base| {
        base.join("lima/bin/limactl").exists() && base.join("docker/bin/docker").exists()
    }) {
        let lima_dir = runtime_base.join("lima");
        env.push((
            "PATH".to_string(),
            format!(
                "{}:{}:{}:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin",
                lima_dir.join("bin").display(),
                runtime_base.join("colima/bin").display(),
                runtime_base.join("docker/bin").display(),
            ),
        ));
        env.push((
            "LIMA_DIR".to_string(),
            lima_dir.to_string_lossy().to_string(),
        ));
    } else {
        env.push((
            "PATH".to_string(),
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin".to_string(),
        ));
    }

    env
}

/// Detect current runtime status for the selected provider.
///
/// For `Docker`/`Colima` this inspects Docker sockets; for `Apple` it checks
/// the `container` binary and a quick readiness probe. The detection result
/// is independent of the user's *choice* — it reports whether that provider
/// is currently usable.
pub fn detect_runtime(resource_dir: &PathBuf, provider: ProviderKind) -> RuntimeStatus {
    match provider {
        ProviderKind::Apple => detect_apple(),
        // Docker and Colima share the Docker API code path; the only
        // difference is whether an external socket is preferred. We keep
        // detection identical to the pre-Apple behavior for both.
        ProviderKind::Docker | ProviderKind::Colima => detect_docker(resource_dir),
    }
}

fn detect_docker(resource_dir: &PathBuf) -> RuntimeStatus {
    // Check if external Docker is running (Docker Desktop, OrbStack — not Colima)
    if external_docker_available() {
        return RuntimeStatus {
            kind: RuntimeKind::External,
            running: true,
            message: "External Docker runtime detected".to_string(),
            provider: ProviderKind::Docker,
        };
    }

    // Check if Colima is already running via socket (works even without binary)
    if colima_socket_path().exists() {
        return RuntimeStatus {
            kind: RuntimeKind::Builtin,
            running: true,
            message: "Built-in runtime (Colima) is running".to_string(),
            provider: ProviderKind::Colima,
        };
    }

    // Check if bundled Colima binary exists and can be started
    // (socket doesn't exist at this point, so running: false)
    if bundled_colima(resource_dir).is_some() {
        return RuntimeStatus {
            kind: RuntimeKind::Builtin,
            running: false,
            message: "Built-in runtime (Colima) is stopped".to_string(),
            provider: ProviderKind::Colima,
        };
    }

    RuntimeStatus {
        kind: RuntimeKind::None,
        running: false,
        message: "No Docker runtime found".to_string(),
        provider: ProviderKind::Docker,
    }
}

fn detect_apple() -> RuntimeStatus {
    if !crate::apple::apple_container_available() {
        return RuntimeStatus {
            kind: RuntimeKind::None,
            running: false,
            message: "Apple Container CLI not found. Install with: brew install container"
                .to_string(),
            provider: ProviderKind::Apple,
        };
    }

    // A quick probe: `container list` succeeds once the backend is running.
    match std::process::Command::new("container")
        .args(["list", "--format", "json"])
        .output()
    {
        Ok(o) if o.status.success() => RuntimeStatus {
            kind: RuntimeKind::Apple,
            running: true,
            message: "Apple Container is running".to_string(),
            provider: ProviderKind::Apple,
        },
        Ok(_) => RuntimeStatus {
            kind: RuntimeKind::Apple,
            running: false,
            message: "Apple Container is stopped".to_string(),
            provider: ProviderKind::Apple,
        },
        Err(_) => RuntimeStatus {
            kind: RuntimeKind::Apple,
            running: false,
            message: "Apple Container is stopped".to_string(),
            provider: ProviderKind::Apple,
        },
    }
}

/// Get the Colima docker socket path
pub fn colima_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".colima/default/docker.sock")
}

/// Try to connect to Docker, checking Colima socket as fallback
pub fn connect_docker() -> Option<Docker> {
    // Only use the default socket when it was verified as a working external
    // runtime. `connect_with_local_defaults` itself only constructs a client;
    // it does not touch the socket, so a stale Docker Desktop symlink could
    // otherwise win over a healthy Colima socket.
    if external_docker_available() {
        if let Ok(client) = Docker::connect_with_local_defaults() {
            return Some(client);
        }
    }

    // Prefer a running Colima runtime when no external Docker runtime was
    // verified. This is also what makes `tauri dev` reconnect to Colima even
    // when its development resource directory has no bundled binaries.
    let socket = colima_socket_path();
    if socket.exists() {
        let url = format!("unix://{}", socket.display());
        if let Ok(client) = Docker::connect_with_unix(&url, 120, bollard::API_DEFAULT_VERSION) {
            return Some(client);
        }
    }

    // Preserve support for a valid default socket when the Docker CLI is not
    // installed (and therefore cannot be probed by `external_docker_available`).
    Docker::connect_with_local_defaults().ok()
}

/// Extract a clean error message from Colima's verbose log output
fn extract_error(full: &str) -> String {
    let error_lines: Vec<&str> = full
        .lines()
        .filter(|l| l.contains("level=fatal") || l.contains("level=error"))
        .collect();
    let raw = if error_lines.is_empty() {
        full.lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("Unknown error")
            .to_string()
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
        Self {
            cpu: 2,
            memory: 4,
            disk: 20,
        }
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
            if line.starts_with('#') {
                continue;
            }
            if let Some(val) = line.strip_prefix("cpus:") {
                if let Ok(v) = val.trim().parse::<u32>() {
                    config.cpu = v;
                }
            } else if let Some(val) = line.strip_prefix("memory:") {
                // Lima format: "memory: 4096MiB"
                let val = val.trim().trim_end_matches("MiB").trim_end_matches("GiB");
                if let Ok(v) = val.parse::<u32>() {
                    config.memory = if v >= 1024 { v / 1024 } else { v };
                }
            } else if let Some(val) = line.strip_prefix("disk:") {
                // Lima format: "disk: 20GiB"
                let val = val.trim().trim_end_matches("GiB").trim_end_matches("MiB");
                if let Ok(v) = val.parse::<u32>() {
                    config.disk = v;
                }
            }
        }
        return config;
    }

    // Fallback to Colima config
    let colima_config = home.join(".colima/default/colima.yaml");
    if let Ok(content) = std::fs::read_to_string(&colima_config) {
        for line in content.lines() {
            if line.starts_with('#') {
                continue;
            }
            if let Some(val) = line.strip_prefix("cpu:") {
                if let Ok(v) = val.trim().parse::<u32>() {
                    config.cpu = v;
                }
            } else if let Some(val) = line.strip_prefix("memory:") {
                if let Ok(v) = val.trim().parse::<u32>() {
                    config.memory = v;
                }
            } else if let Some(val) = line.strip_prefix("disk:") {
                if let Ok(v) = val.trim().parse::<u32>() {
                    config.disk = v;
                }
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
    // Preserve the user's current Colima allocation. In particular, a
    // previously enlarged disk cannot be shrunk by Colima, so restarting with
    // the hard-coded defaults would make the runtime fail to start.
    start_builtin_with_config(resource_dir, &read_vm_config())
}

pub fn start_builtin_with_config(
    resource_dir: &PathBuf,
    config: &VmConfig,
) -> Result<String, String> {
    let colima = ensure_colima(resource_dir)?;

    // Update config files so existing VMs pick up the new values
    write_vm_config(config);

    // Derive env from the actual colima path (not resource_dir, which may be wrong in dev)
    let env = colima_env_for(&colima);
    let cpu = config.cpu.to_string();
    let mem = config.memory.to_string();
    let disk = config.disk.to_string();

    let output = Command::new(&colima)
        .args([
            "start",
            "--cpu",
            &cpu,
            "--memory",
            &mem,
            "--disk",
            &disk,
            "--runtime",
            "docker",
        ])
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
            .args([
                "start",
                "--cpu",
                &cpu,
                "--memory",
                &mem,
                "--disk",
                &disk,
                "--runtime",
                "docker",
            ])
            .envs(env)
            .output()
            .map_err(|e| e.to_string())?;

        if !retry.status.success() {
            return Err(extract_error(&full));
        }
    }

    Ok("Runtime started".to_string())
}

/// Stop the bundled Colima runtime
pub fn stop_builtin(resource_dir: &PathBuf) -> Result<String, String> {
    let colima = bundled_colima(resource_dir).ok_or("Bundled Colima not found")?;

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
