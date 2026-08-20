//! Apple Container CLI (`container`) adapter.
//!
//! Apple's native `container` tool does NOT expose a Docker Engine API socket,
//! so every operation shells out to the `container` binary and parses its
//! `--format json` / `inspect` JSON output. Because the exact JSON field names
//! are not documented and vary between releases, we deserialize into
//! `serde_json::Value` and probe a small set of candidate keys.

use crate::docker::{ContainerGroup, ContainerInfo, ImageInfo, NetworkInfo, PortInfo, VolumeInfo};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;

/// Resolve the `container` binary. Allows overriding via CONTAINER_BIN for dev.
fn container_bin() -> String {
    std::env::var("CONTAINER_BIN").unwrap_or_else(|_| "container".to_string())
}

/// Build a `container` command.
fn container_cmd() -> Command {
    Command::new(container_bin())
}

/// Run a `container` command and return (stdout, stderr, success).
fn run(cmd: &mut Command) -> Result<(String, String, bool), String> {
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run container CLI: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((stdout, stderr, output.status.success()))
}

fn last_error(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Unknown container CLI error")
        .to_string()
}

/// Coerce a JSON value into a String, trimming a leading `sha256:` if present
/// when `digest` is set (for image IDs).
fn val_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => v.to_string(),
    }
}

/// Pick the first present key from a JSON object.
fn pick<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    if let Value::Object(map) = obj {
        for k in keys {
            if let Some(v) = map.get(*k) {
                if !v.is_null() {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn pick_str(obj: &Value, keys: &[&str]) -> String {
    pick(obj, keys).map(val_string).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// True if the `container` binary is installed and runnable.
pub fn apple_container_available() -> bool {
    Command::new(container_bin())
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ensure the Containerization framework backend is running. Idempotent.
/// `container system start` returns success if already running.
pub fn system_start() -> Result<(), String> {
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["system", "start"]);
        run(&mut c)
    }?;
    if ok {
        return Ok(());
    }
    Err(format!(
        "container system start failed: {}",
        last_error(&err)
    ))
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// Parse one container row from `container list --format json`.
/// The schema is undocumented, so we probe common key spellings.
fn container_from_json(obj: &Value) -> ContainerInfo {
    let id = pick_str(obj, &["id", "name", "containerID", "container_id"]);
    let image = pick_str(obj, &["image", "imageRef", "image_ref"]);
    let state = pick_str(obj, &["state", "status"]);

    let names = {
        let mut n = vec![id.clone()];
        // Avoid duplicate entry if name === id (Apple treats them as equal).
        n.dedup();
        n
    };

    // Ports: Apple list output may not include port mappings; left empty here.
    // Inspect (env/mounts) is the source of truth for richer detail.
    let ports: Vec<PortInfo> = Vec::new();

    ContainerInfo {
        id: id.chars().take(12).collect(),
        names,
        image,
        state,
        status: pick_str(obj, &["status", "state"]),
        ports,
        created: pick(obj, &["created", "createdAt", "created_at", "creation"])
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        labels: HashMap::new(),
    }
}

pub fn list_containers() -> Result<Vec<ContainerGroup>, String> {
    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(["list", "--all", "--format", "json"]);
        run(&mut c)
    }?;
    if !ok {
        return Err(last_error(&stderr));
    }

    let trimmed = stdout.trim();
    let items: Vec<Value> = if trimmed.is_empty() {
        Vec::new()
    } else if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid list JSON: {e}"))?
    } else {
        // Some versions emit JSON-lines; tolerate that.
        trimmed
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("Invalid list JSON line: {e}"))?
    };

    let containers: Vec<ContainerInfo> = items.iter().map(container_from_json).collect();

    // Apple Container has no compose concept — everything is standalone.
    Ok(if containers.is_empty() {
        Vec::new()
    } else {
        vec![ContainerGroup {
            name: "Standalone".to_string(),
            containers,
        }]
    })
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

pub fn list_images() -> Result<Vec<ImageInfo>, String> {
    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(["image", "list", "--format", "json"]);
        run(&mut c)
    }?;
    if !ok {
        return Err(last_error(&stderr));
    }

    let trimmed = stdout.trim();
    let items: Vec<Value> = if trimmed.is_empty() {
        Vec::new()
    } else if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid image list JSON: {e}"))?
    } else {
        trimmed
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("Invalid image list JSON line: {e}"))?
    };

    Ok(items
        .iter()
        .map(|obj| {
            let id = pick_str(obj, &["id", "digest", "imageID", "image_id"]);
            ImageInfo {
                // Keep parity with Docker display: strip the "sha256:" prefix and truncate.
                id: id.trim_start_matches("sha256:").chars().take(12).collect(),
                repo_tags: {
                    let mut tags = Vec::new();
                    if let Some(t) = pick(obj, &["name", "reference", "repoTags", "repo_tags"]) {
                        match t {
                            Value::String(s) => tags.push(s.clone()),
                            Value::Array(arr) => {
                                for a in arr {
                                    if let Some(s) = a.as_str() {
                                        tags.push(s.to_string());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    if tags.is_empty() {
                        tags.push(pick_str(obj, &["name", "reference"]));
                    }
                    tags.into_iter().filter(|s| !s.is_empty()).collect()
                },
                size: pick(obj, &["size", "fullSize", "full_size"])
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                created: pick(obj, &["created", "createdAt", "created_at", "creation"])
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

pub fn start_container(id: &str) -> Result<(), String> {
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["start", id]);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}

pub fn stop_container(id: &str) -> Result<(), String> {
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["stop", id]);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}

/// Apple Container has no `restart`; emulate with stop then start.
pub fn restart_container(id: &str) -> Result<(), String> {
    // Best-effort stop: ignore "not running" style failures, then start.
    let _ = container_cmd().args(["stop", id]).output();
    start_container(id)
}

pub fn remove_container(id: &str, force: bool) -> Result<(), String> {
    let mut args = vec!["delete"];
    if force {
        args.push("--force");
    }
    args.push(id);
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(&args);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}

pub fn remove_image(image: &str) -> Result<(), String> {
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["image", "delete", "--force", image]);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}

pub fn pull_image(image: &str) -> Result<(), String> {
    // --progress none avoids TTY control sequences in captured stdout.
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["image", "pull", "--progress", "none", image]);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}

// ---------------------------------------------------------------------------
// Create / Run
// ---------------------------------------------------------------------------

pub fn create_container(input: &crate::docker::CreateContainerInput) -> Result<String, String> {
    let mut args: Vec<String> = vec!["run".to_string()];

    if let Some(name) = &input.name {
        args.push("--name".to_string());
        args.push(name.clone());
    }

    for p in &input.ports {
        // Apple format: [host-ip:]host-port:container-port[/protocol]
        args.push("-p".to_string());
        args.push(format!("{}:{}", p.host, p.container));
    }

    for v in &input.volumes {
        args.push("-v".to_string());
        args.push(format!("{}:{}", v.host, v.container));
    }

    for e in &input.env {
        args.push("-e".to_string());
        args.push(e.clone());
    }

    if input.auto_start {
        args.push("-d".to_string());
    } else {
        args.push("create".to_string());
        // Replace the leading "run" with "create".
        args[0] = "create".to_string();
    }

    args.push(input.image.clone());

    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(&args);
        run(&mut c)
    }?;
    if !ok {
        return Err(last_error(&stderr));
    }
    // The new container ID is printed on stdout (detached) — use the name if
    // the caller supplied one (Apple treats name == ID), else the printed ID.
    let printed = stdout.trim().to_string();
    Ok(if let Some(name) = &input.name {
        if name.is_empty() {
            printed
        } else {
            name.clone()
        }
    } else {
        printed
    })
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

pub fn get_container_logs(id: &str, tail: Option<&str>) -> Result<Vec<String>, String> {
    let mut args = vec!["logs".to_string()];
    if let Some(n) = tail {
        if n != "all" {
            args.push("-n".to_string());
            args.push(n.to_string());
        }
    }
    args.push(id.to_string());

    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(&args);
        run(&mut c)
    }?;
    if !ok {
        return Err(last_error(&stderr));
    }

    // Apple `container logs` does not split stdout/stderr per line markers like
    // Docker; combine both streams, preserving order.
    let mut lines: Vec<String> = stdout
        .lines()
        .map(String::from)
        .chain(stderr.lines().map(String::from))
        .collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    Ok(lines)
}

/// Apple Container has no `--since` flag. As a fallback we fetch the last
/// `tail` lines; the frontend already de-duplicates against previously shown
/// content by content prefix matching.
pub fn get_container_logs_since(
    id: &str,
    _since: i64,
    _timestamps: bool,
) -> Result<Vec<String>, String> {
    get_container_logs(id, Some("200"))
}

// ---------------------------------------------------------------------------
// Inspect — env & mounts
// ---------------------------------------------------------------------------

/// Run `container inspect <id>` and return the parsed JSON object.
fn inspect(id: &str) -> Result<Value, String> {
    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(["inspect", id]);
        run(&mut c)
    }?;
    if !ok {
        return Err(last_error(&stderr));
    }
    let trimmed = stdout.trim();
    // inspect may emit a single object or an array of objects.
    if trimmed.starts_with('[') {
        let arr: Vec<Value> =
            serde_json::from_str(trimmed).map_err(|e| format!("Invalid inspect JSON: {e}"))?;
        arr.into_iter()
            .next()
            .ok_or_else(|| "Empty inspect result".to_string())
    } else {
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid inspect JSON: {e}"))
    }
}

pub fn get_container_env(id: &str) -> Result<Vec<String>, String> {
    let obj = inspect(id)?;

    // Probe nested config locations where env typically lives.
    let candidates: Vec<&str> = vec!["config", "process", "container", "specification", "spec"];
    let mut env: Vec<String> = Vec::new();

    let mut scan = |o: &Value| {
        for key in &candidates {
            if let Some(sub) = o.get(*key) {
                if let Some(e) = find_env_in(sub) {
                    env = e;
                    return;
                }
            }
        }
        if let Some(e) = find_env_in(o) {
            env = e;
        }
    };
    scan(&obj);

    Ok(env)
}

/// Search a JSON subtree for an `env` array of strings.
fn find_env_in(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if k == "env" || k == "environment" {
                    if let Value::Array(arr) = val {
                        let out: Vec<String> = arr
                            .iter()
                            .map(|e| match e {
                                Value::String(s) => s.clone(),
                                _ => val_string(e),
                            })
                            .collect();
                        if !out.is_empty() {
                            return Some(out);
                        }
                    }
                }
            }
            for (_, val) in map {
                if let Some(found) = find_env_in(val) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

pub fn get_container_mounts(id: &str) -> Result<Vec<crate::docker::MountInfo>, String> {
    let obj = inspect(id)?;

    let mut mounts: Vec<crate::docker::MountInfo> = Vec::new();
    collect_mounts(&obj, &mut mounts);

    // Mirror the Docker adapter's filtering: only bind mounts with a real,
    // accessible host path; skip sockets and /proc /sys /dev.
    Ok(mounts
        .into_iter()
        .filter(|m| {
            m.mount_type == "bind"
                && !m.source.starts_with("/var/run")
                && !m.source.starts_with("/proc")
                && !m.source.starts_with("/sys")
                && !m.source.starts_with("/dev")
        })
        .collect())
}

fn collect_mounts(v: &Value, out: &mut Vec<crate::docker::MountInfo>) {
    match v {
        Value::Object(map) => {
            // A mounts array typically lives under "mounts" / "volumes".
            for key in &["mounts", "volumes", "mount"] {
                if let Some(Value::Array(arr)) = map.get(*key) {
                    for item in arr {
                        if let Some(m) = parse_mount(item) {
                            out.push(m);
                        }
                    }
                }
            }
            for (_, val) in map {
                collect_mounts(val, out);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_mounts(item, out);
            }
        }
        _ => {}
    }
}

fn parse_mount(item: &Value) -> Option<crate::docker::MountInfo> {
    let source = pick_str(item, &["source", "hostPath", "host_path", "src"]);
    let destination = pick_str(
        item,
        &["destination", "target", "containerPath", "container_path"],
    );
    if source.is_empty() && destination.is_empty() {
        return None;
    }
    let mount_type = pick_str(item, &["type", "mountType", "mount_type"]);
    let mount_type = if mount_type.is_empty() {
        "bind".to_string()
    } else {
        mount_type.to_lowercase()
    };
    let rw = pick(item, &["rw", "readWrite", "read_write", "writable"])
        .map(|v| match v {
            Value::Bool(b) => *b,
            Value::String(s) => s == "true" || s == "rw",
            _ => true,
        })
        .unwrap_or(true);
    let mode = pick_str(item, &["mode", "options", "propagation"]);

    Some(crate::docker::MountInfo {
        mount_type,
        source,
        destination,
        mode,
        rw,
    })
}

// ---------------------------------------------------------------------------
// Volumes & Networks (best-effort; networks require macOS 26+)
// ---------------------------------------------------------------------------

pub fn list_volumes() -> Result<Vec<VolumeInfo>, String> {
    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(["volume", "list", "--format", "json"]);
        run(&mut c)
    }?;
    if !ok {
        // Volume support may be absent on some builds; surface an empty list
        // rather than crashing the UI.
        let msg = last_error(&stderr);
        if msg.to_lowercase().contains("unknown command") || msg.to_lowercase().contains("no such")
        {
            return Ok(Vec::new());
        }
        return Err(msg);
    }

    let trimmed = stdout.trim();
    let items: Vec<Value> = if trimmed.is_empty() {
        Vec::new()
    } else if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid volume list JSON: {e}"))?
    } else {
        trimmed
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("Invalid volume list JSON line: {e}"))?
    };

    Ok(items
        .iter()
        .map(|obj| VolumeInfo {
            name: pick_str(obj, &["name", "label"]),
            driver: pick_str(obj, &["driver", "type"]).to_lowercase(),
            mountpoint: pick_str(obj, &["mountpoint", "mountPoint", "mount_point", "path"]),
            labels: HashMap::new(),
        })
        .collect())
}

pub fn list_networks() -> Result<Vec<NetworkInfo>, String> {
    let (stdout, stderr, ok) = {
        let mut c = container_cmd();
        c.args(["network", "list", "--format", "json"]);
        run(&mut c)
    }?;
    if !ok {
        // Networks are macOS 26+ only; tolerate older versions gracefully.
        let msg = last_error(&stderr);
        if msg.to_lowercase().contains("unknown command") || msg.to_lowercase().contains("no such")
        {
            return Ok(Vec::new());
        }
        return Err(msg);
    }

    let trimmed = stdout.trim();
    let items: Vec<Value> = if trimmed.is_empty() {
        Vec::new()
    } else if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid network list JSON: {e}"))?
    } else {
        trimmed
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("Invalid network list JSON line: {e}"))?
    };

    Ok(items
        .iter()
        .map(|obj| NetworkInfo {
            id: pick_str(obj, &["id", "name"]).chars().take(12).collect(),
            name: pick_str(obj, &["name", "id"]),
            driver: pick_str(obj, &["driver", "plugin", "type"]).to_lowercase(),
            scope: pick_str(obj, &["scope"]).to_lowercase(),
            containers: pick(obj, &["containers"])
                .and_then(|c| match c {
                    Value::Array(a) => Some(a.len()),
                    Value::Object(m) => Some(m.len()),
                    _ => None,
                })
                .unwrap_or(0),
        })
        .collect())
}

pub fn remove_volume(name: &str) -> Result<(), String> {
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["volume", "delete", name]);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}

pub fn remove_network(name: &str) -> Result<(), String> {
    let (_out, err, ok) = {
        let mut c = container_cmd();
        c.args(["network", "delete", name]);
        run(&mut c)
    }?;
    if ok {
        Ok(())
    } else {
        Err(last_error(&err))
    }
}
