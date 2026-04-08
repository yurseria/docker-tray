use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::image::{CreateImageOptions, ListImagesOptions, RemoveImageOptions};
use bollard::models::{HostConfig, PortBinding, PortMap};
use bollard::network::ListNetworksOptions;
use bollard::volume::ListVolumesOptions;
use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tauri::State;

pub struct DockerState {
    pub client: Docker,
}

#[derive(Debug, Serialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: Vec<PortInfo>,
    pub created: i64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PortInfo {
    pub private_port: u16,
    pub public_port: Option<u16>,
    pub port_type: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub containers: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct ContainerGroup {
    pub name: String,
    pub containers: Vec<ContainerInfo>,
}

fn validate_container_id(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Invalid container ID".to_string());
    }
    Ok(())
}

fn extract_compose_project(labels: &HashMap<String, String>) -> Option<String> {
    labels.get("com.docker.compose.project").cloned()
}

fn container_from_summary(c: bollard::models::ContainerSummary) -> ContainerInfo {
    let labels = c.labels.unwrap_or_default();
    ContainerInfo {
        id: c.id.unwrap_or_default().chars().take(12).collect(),
        names: c
            .names
            .unwrap_or_default()
            .into_iter()
            .map(|n| n.trim_start_matches('/').to_string())
            .collect(),
        image: c.image.unwrap_or_default(),
        state: c.state.unwrap_or_default(),
        status: c.status.unwrap_or_default(),
        ports: c
            .ports
            .unwrap_or_default()
            .into_iter()
            .map(|p| PortInfo {
                private_port: p.private_port,
                public_port: p.public_port.map(|v| v as u16),
                port_type: p.typ.map(|t| format!("{:?}", t)).unwrap_or_default(),
            })
            .collect(),
        created: c.created.unwrap_or(0),
        labels,
    }
}

#[tauri::command]
pub async fn list_containers(
    docker: State<'_, DockerState>,
) -> Result<Vec<ContainerGroup>, String> {
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };

    let containers = docker
        .client
        .list_containers(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    let mut groups: HashMap<String, Vec<ContainerInfo>> = HashMap::new();
    let mut ungrouped: Vec<ContainerInfo> = Vec::new();

    for c in containers {
        let info = container_from_summary(c);
        if let Some(project) = extract_compose_project(&info.labels) {
            groups.entry(project).or_default().push(info);
        } else {
            ungrouped.push(info);
        }
    }

    let mut result: Vec<ContainerGroup> = groups
        .into_iter()
        .map(|(name, containers)| ContainerGroup { name, containers })
        .collect();

    result.sort_by(|a, b| a.name.cmp(&b.name));

    if !ungrouped.is_empty() {
        result.push(ContainerGroup {
            name: "Standalone".to_string(),
            containers: ungrouped,
        });
    }

    Ok(result)
}

#[tauri::command]
pub async fn list_images(docker: State<'_, DockerState>) -> Result<Vec<ImageInfo>, String> {
    let opts = ListImagesOptions::<String> {
        all: false,
        ..Default::default()
    };

    let images = docker
        .client
        .list_images(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    Ok(images
        .into_iter()
        .map(|i| ImageInfo {
            id: i.id.chars().skip(7).take(12).collect(),
            repo_tags: i.repo_tags,
            size: i.size,
            created: i.created,
        })
        .collect())
}

#[tauri::command]
pub async fn list_volumes(docker: State<'_, DockerState>) -> Result<Vec<VolumeInfo>, String> {
    let opts = ListVolumesOptions::<String> {
        ..Default::default()
    };

    let response = docker
        .client
        .list_volumes(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    Ok(response
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|v| VolumeInfo {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            labels: v.labels,
        })
        .collect())
}

#[tauri::command]
pub async fn list_networks(docker: State<'_, DockerState>) -> Result<Vec<NetworkInfo>, String> {
    let opts = ListNetworksOptions::<String> {
        ..Default::default()
    };

    let networks = docker
        .client
        .list_networks(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    Ok(networks
        .into_iter()
        .map(|n| NetworkInfo {
            id: n.id.unwrap_or_default().chars().take(12).collect(),
            name: n.name.unwrap_or_default(),
            driver: n.driver.unwrap_or_default(),
            scope: n.scope.unwrap_or_default(),
            containers: n.containers.map(|c| c.len()).unwrap_or(0),
        })
        .collect())
}

#[tauri::command]
pub async fn start_container(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<(), String> {
    docker
        .client
        .start_container(&id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_container(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<(), String> {
    docker
        .client
        .stop_container(&id, Some(StopContainerOptions { t: 10 }))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restart_container(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<(), String> {
    docker
        .client
        .restart_container(&id, Some(bollard::container::RestartContainerOptions { t: 10 }))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_container_logs(
    docker: State<'_, DockerState>,
    id: String,
    tail: Option<String>,
) -> Result<Vec<String>, String> {
    use futures_util::StreamExt;

    let opts = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        tail: tail.unwrap_or_else(|| "100".to_string()),
        ..Default::default()
    };

    let mut stream = docker.client.logs(&id, Some(opts));
    let mut lines = Vec::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(output) => lines.push(output.to_string()),
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(lines)
}

#[tauri::command]
pub async fn docker_ping(docker: State<'_, DockerState>) -> Result<bool, String> {
    docker
        .client
        .ping()
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
}

// --- Container Env Vars ---

#[tauri::command]
pub async fn get_container_env(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<Vec<String>, String> {
    let info = docker
        .client
        .inspect_container(&id, None::<InspectContainerOptions>)
        .await
        .map_err(|e| e.to_string())?;

    Ok(info
        .config
        .and_then(|c| c.env)
        .unwrap_or_default())
}

// --- Remove ---

#[tauri::command]
pub async fn remove_container(
    docker: State<'_, DockerState>,
    id: String,
    force: Option<bool>,
) -> Result<(), String> {
    docker
        .client
        .remove_container(
            &id,
            Some(RemoveContainerOptions {
                force: force.unwrap_or(false),
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_image(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<(), String> {
    docker
        .client
        .remove_image(
            &id,
            Some(RemoveImageOptions {
                force: true,
                ..Default::default()
            }),
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_volume(
    docker: State<'_, DockerState>,
    name: String,
) -> Result<(), String> {
    docker
        .client
        .remove_volume(&name, None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_network(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<(), String> {
    docker
        .client
        .remove_network(&id)
        .await
        .map_err(|e| e.to_string())
}

// --- Pull Image ---

#[tauri::command]
pub async fn pull_image(
    docker: State<'_, DockerState>,
    image: String,
) -> Result<(), String> {
    use futures_util::StreamExt;

    let (repo, tag) = match image.split_once(':') {
        Some((r, t)) => (r.to_string(), t.to_string()),
        None => (image.clone(), "latest".to_string()),
    };

    let opts = CreateImageOptions {
        from_image: repo,
        tag,
        ..Default::default()
    };

    let mut stream = docker.client.create_image(Some(opts), None, None);
    while let Some(result) = stream.next().await {
        result.map_err(|e| e.to_string())?;
    }

    Ok(())
}

// --- Create Container ---

#[derive(Debug, Deserialize)]
pub struct PortMapping {
    pub host: String,
    pub container: String,
}

#[derive(Debug, Deserialize)]
pub struct VolumeMapping {
    pub host: String,
    pub container: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateContainerInput {
    pub name: Option<String>,
    pub image: String,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMapping>,
    pub env: Vec<String>,
    pub auto_start: bool,
}

#[tauri::command]
pub async fn create_container(
    docker: State<'_, DockerState>,
    input: CreateContainerInput,
) -> Result<String, String> {
    let mut exposed_ports = HashMap::new();
    let mut port_bindings: PortMap = HashMap::new();

    for p in &input.ports {
        let host_port: u16 = p.host.parse().map_err(|_| format!("Invalid host port: {}", p.host))?;
        let container_port_num: u16 = p.container.parse().map_err(|_| format!("Invalid container port: {}", p.container))?;
        if host_port == 0 || container_port_num == 0 {
            return Err("Port must be between 1 and 65535".to_string());
        }
        let container_port = format!("{}/tcp", p.container);
        exposed_ports.insert(container_port.clone(), HashMap::new());
        port_bindings.insert(
            container_port,
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(p.host.clone()),
            }]),
        );
    }

    let binds: Vec<String> = input
        .volumes
        .iter()
        .map(|v| format!("{}:{}", v.host, v.container))
        .collect();

    let config = Config {
        image: Some(input.image.clone()),
        exposed_ports: Some(exposed_ports),
        env: Some(input.env.clone()),
        host_config: Some(HostConfig {
            port_bindings: Some(port_bindings),
            binds: Some(binds),
            ..Default::default()
        }),
        ..Default::default()
    };

    let opts = input.name.as_ref().map(|n| CreateContainerOptions {
        name: n.as_str(),
        platform: None,
    });

    let response = docker
        .client
        .create_container(opts, config)
        .await
        .map_err(|e| e.to_string())?;

    if input.auto_start {
        docker
            .client
            .start_container(&response.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(response.id)
}

// --- Docker Compose ---

#[tauri::command]
pub async fn compose_up(file_path: String) -> Result<String, String> {
    let path = Path::new(&file_path);
    if !path.is_file() {
        return Err(format!("File not found: {}", file_path));
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("yml" | "yaml") => {}
        _ => return Err("File must have .yml or .yaml extension".to_string()),
    }

    // Try `docker compose` first, fall back to `docker-compose`
    let output = Command::new("docker")
        .args(["compose", "-f", &file_path, "up", "-d"])
        .output()
        .or_else(|_| {
            Command::new("docker-compose")
                .args(["-f", &file_path, "up", "-d"])
                .output()
        })
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(stderr);
    }

    Ok(format!("{}{}", stdout, stderr))
}

// --- Mount info ---

#[derive(Debug, Serialize, Clone)]
pub struct MountInfo {
    pub mount_type: String,
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub rw: bool,
}

#[tauri::command]
pub async fn get_container_mounts(
    docker: State<'_, DockerState>,
    id: String,
) -> Result<Vec<MountInfo>, String> {
    let info = docker
        .client
        .inspect_container(&id, None::<InspectContainerOptions>)
        .await
        .map_err(|e| e.to_string())?;

    let mounts = info
        .mounts
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            let source = m.source?;
            let mount_type = m
                .typ
                .map(|t| format!("{:?}", t).to_lowercase())
                .unwrap_or_default();

            // Only bind mounts have real host paths accessible from macOS.
            // Volume mounts (/var/lib/docker/volumes/...) are inside the Docker VM.
            // Also skip sockets and /proc, /sys, /dev paths.
            if mount_type != "bind" {
                return None;
            }
            if source.starts_with("/var/run")
                || source.starts_with("/proc")
                || source.starts_with("/sys")
                || source.starts_with("/dev")
            {
                return None;
            }

            Some(MountInfo {
                mount_type,
                source,
                destination: m.destination.unwrap_or_default(),
                mode: m.mode.unwrap_or_default(),
                rw: m.rw.unwrap_or(true),
            })
        })
        .collect();

    Ok(mounts)
}

// --- Open in Finder ---

#[tauri::command]
pub async fn open_in_finder(path: String) -> Result<(), String> {
    let output = Command::new("open")
        .args(["-R", &path])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Cannot open path: {}", err));
    }
    Ok(())
}

// --- Terminal ---

#[tauri::command]
pub async fn detect_terminal() -> Result<String, String> {
    let terminals = [
        ("/Applications/Ghostty.app", "ghostty"),
        ("/Applications/iTerm.app", "iterm"),
    ];

    for (path, name) in &terminals {
        if Path::new(path).exists() {
            return Ok(name.to_string());
        }
    }

    Ok("terminal".to_string())
}

#[tauri::command]
pub async fn open_terminal(
    container_id: String,
    _container_name: String,
    shell: Option<String>,
    terminal_override: Option<String>,
) -> Result<(), String> {
    validate_container_id(&container_id)?;

    // Validate shell against allowlist
    let allowed_shells = ["/bin/sh", "/bin/bash", "/bin/zsh", "/bin/ash"];
    let sh = shell.unwrap_or_else(|| "/bin/sh".to_string());
    if !allowed_shells.contains(&sh.as_str()) {
        return Err(format!("Shell not allowed: {}", sh));
    }

    let terminal = match terminal_override {
        Some(ref t) if t != "auto" => t.clone(),
        _ => detect_terminal().await?,
    };
    let docker_cmd = format!("docker exec -it {} {}", container_id, sh);

    // Write a temp script so the terminal runs a single clean command
    let tmp = std::env::temp_dir().join(format!("docker-tray-{}.sh", container_id));
    std::fs::write(
        &tmp,
        format!("#!/bin/sh\n{}\n", docker_cmd),
    )
    .map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }

    let tmp_str = tmp.to_string_lossy().to_string();
    let tmp_escaped = tmp_str.replace('\\', "\\\\").replace('"', "\\\"");

    match terminal.as_str() {
        "ghostty" => {
            Command::new("/Applications/Ghostty.app/Contents/MacOS/ghostty")
                .args(["-e", &tmp_str])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "iterm" => {
            let script = format!(
                r#"tell application "iTerm"
                    activate
                    create window with default profile command "{}"
                end tell"#,
                tmp_escaped
            );
            Command::new("osascript")
                .args(["-e", &script])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => {
            let script = format!(
                r#"tell application "Terminal"
                    activate
                    do script "{}"
                end tell"#,
                tmp_escaped
            );
            Command::new("osascript")
                .args(["-e", &script])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    };

    Ok(())
}

// --- File Explorer ---

#[derive(Debug, Serialize, Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: String,
    pub modified: String,
    pub permissions: String,
}

#[tauri::command]
pub async fn list_container_files(
    container_id: String,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    validate_container_id(&container_id)?;
    // Try GNU ls first, fallback to plain ls for BusyBox/Alpine
    let output = Command::new("docker")
        .args(["exec", &container_id, "ls", "-la", "--time-style=long-iso", &path])
        .output()
        .map_err(|e| e.to_string())?;

    let (stdout, is_gnu) = if output.status.success() {
        (String::from_utf8_lossy(&output.stdout).to_string(), true)
    } else {
        // Fallback: plain ls -la (BusyBox)
        let fallback = Command::new("docker")
            .args(["exec", &container_id, "ls", "-la", &path])
            .output()
            .map_err(|e| e.to_string())?;
        if !fallback.status.success() {
            let err = String::from_utf8_lossy(&fallback.stderr);
            return Err(format!("Failed to list files: {}", err));
        }
        (String::from_utf8_lossy(&fallback.stdout).to_string(), false)
    };

    let entries: Vec<FileEntry> = stdout
        .lines()
        .skip(1) // skip "total N" line
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // GNU: perms links owner group size date time name...
            // BusyBox: perms links owner group size mon day time/year name...
            let min_cols = if is_gnu { 8 } else { 8 };
            if parts.len() < min_cols {
                return None;
            }

            let (size_idx, name_start, modified) = if is_gnu {
                // GNU: parts[4]=size, parts[5]=date, parts[6]=time, parts[7..]=name
                (4, 7, format!("{} {}", parts[5], parts[6]))
            } else {
                // BusyBox: parts[4]=size, parts[5]=mon, parts[6]=day, parts[7]=time/year, parts[8..]=name
                if parts.len() < 9 {
                    // Some BusyBox outputs have fewer columns
                    (4, 8.min(parts.len()), format!("{} {}", parts.get(5).unwrap_or(&""), parts.get(6).unwrap_or(&"")))
                } else {
                    (4, 8, format!("{} {} {}", parts[5], parts[6], parts[7]))
                }
            };

            if name_start >= parts.len() {
                return None;
            }

            let name = parts[name_start..].join(" ");
            if name == "." || name == ".." {
                return None;
            }
            let display_name = if let Some(idx) = name.find(" -> ") {
                name[..idx].to_string()
            } else {
                name
            };
            Some(FileEntry {
                is_dir: parts[0].starts_with('d'),
                permissions: parts[0].to_string(),
                size: parts.get(size_idx).unwrap_or(&"").to_string(),
                modified,
                name: display_name,
            })
        })
        .collect();

    Ok(entries)
}

#[tauri::command]
pub async fn read_container_file(
    container_id: String,
    path: String,
) -> Result<String, String> {
    validate_container_id(&container_id)?;
    let output = Command::new("docker")
        .args(["exec", &container_id, "cat", &path])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to read file: {}", err));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub async fn save_from_container(
    container_id: String,
    container_path: String,
    host_path: String,
) -> Result<(), String> {
    validate_container_id(&container_id)?;
    let src = format!("{}:{}", container_id, container_path);
    let output = Command::new("docker")
        .args(["cp", &src, &host_path])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to copy: {}", err));
    }
    Ok(())
}

#[tauri::command]
pub async fn import_to_container(
    container_id: String,
    host_path: String,
    container_path: String,
) -> Result<(), String> {
    validate_container_id(&container_id)?;
    let dest = format!("{}:{}", container_id, container_path);
    let output = Command::new("docker")
        .args(["cp", &host_path, &dest])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to import: {}", err));
    }
    Ok(())
}
