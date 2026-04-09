mod docker;
mod runtime;

use tauri_plugin_autostart::ManagerExt;

use bollard::Docker;
use docker::DockerState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{
    image::Image,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::{MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WebviewUrl, WebviewWindowBuilder,
};

fn send_notification(app: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let notif = app.notification();

    // Check if notification permission is granted
    let granted = notif.permission_state()
        .map(|s| s == tauri_plugin_notification::PermissionState::Granted)
        .unwrap_or(false);

    if granted {
        let _ = notif.builder().title(title).body(body).show();
    } else {
        // Fallback: osascript (shows as "Script Editor")
        let _ = std::process::Command::new("osascript")
            .args(["-e", &format!(
                r#"display notification "{}" with title "{}""#,
                body, title
            )])
            .output();
    }
}

pub struct BrowsingState(pub Arc<AtomicBool>);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(target_os = "macos")]
fn setup_macos_window(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSColor, NSWindow};

    let ns_window = window.ns_window().expect("Failed to get NSWindow");
    let ns_window: &NSWindow = unsafe { &*(ns_window as *const _ as *const NSWindow) };

    ns_window.setOpaque(false);
    ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
    ns_window.setHasShadow(true);

    if let Some(content_view) = ns_window.contentView() {
        content_view.setWantsLayer(true);
        if let Some(layer) = content_view.layer() {
            layer.setCornerRadius(12.0);
            layer.setMasksToBounds(true);
        }
    }
}

#[cfg(target_os = "macos")]
fn set_macos_accessory_app() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let docker_client = Docker::connect_with_local_defaults()
        .or_else(|_| Docker::connect_with_socket_defaults())
        .ok();

    let last_focus_lost = Arc::new(AtomicU64::new(0));
    let last_focus_lost_for_tray = last_focus_lost.clone();
    let browsing = Arc::new(AtomicBool::new(false));
    let browsing_for_event = browsing.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(DockerState {
            client: Arc::new(std::sync::Mutex::new(docker_client)),
        })
        .manage(BrowsingState(browsing))
        .manage(RuntimeState {
            starting: Arc::new(AtomicBool::new(false)),
            error: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            docker::list_containers,
            docker::list_images,
            docker::list_volumes,
            docker::list_networks,
            docker::start_container,
            docker::stop_container,
            docker::restart_container,
            docker::get_container_logs,
            docker::docker_ping,
            docker::remove_container,
            docker::remove_image,
            docker::remove_volume,
            docker::remove_network,
            docker::pull_image,
            docker::create_container,
            docker::compose_up,
            docker::get_container_logs_since,
            docker::get_container_env,
            docker::get_container_mounts,
            docker::open_in_finder,
            docker::detect_terminal,
            docker::open_terminal,
            docker::list_container_files,
            docker::read_container_file,
            docker::save_from_container,
            docker::import_to_container,
            runtime_status,
            runtime_start,
            runtime_stop,
            get_autostart,
            set_autostart,
            open_log_window,
            open_file_explorer_window,
            get_home_dir,
            pick_file_for_import,
            pick_yaml_file,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            set_macos_accessory_app();

            let icon = app
                .path()
                .resource_dir()
                .ok()
                .and_then(|dir| Image::from_path(dir.join("icons/tray-icon.png")).ok())
                .or_else(|| Image::from_path("icons/tray-icon.png").ok())
                .expect("Failed to load tray icon");

            let autostart_manager = app.autolaunch();
            let is_autostart = autostart_manager.is_enabled().unwrap_or(false);

            let autostart_item = CheckMenuItemBuilder::with_id("autostart", "Start at Login")
                .checked(is_autostart)
                .build(app)
                .expect("Failed to build autostart menu item");
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Docker Tray")
                .build(app)
                .expect("Failed to build quit menu item");
            let tray_menu = MenuBuilder::new(app)
                .item(&autostart_item)
                .separator()
                .item(&quit_item)
                .build()
                .expect("Failed to build tray menu");

            let _tray = TrayIconBuilder::with_id("docker-tray")
                .icon(icon)
                .icon_as_template(true)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("Docker Tray")
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "quit" => {
                            app.exit(0);
                        }
                        "autostart" => {
                            let manager = app.autolaunch();
                            let enabled = manager.is_enabled().unwrap_or(false);
                            if enabled {
                                let _ = manager.disable();
                            } else {
                                let _ = manager.enable();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        position,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let window = match app.get_webview_window("main") {
                            Some(w) => w,
                            None => WebviewWindowBuilder::new(
                                app,
                                "main",
                                WebviewUrl::default(),
                            )
                            .title("Docker Tray")
                            .inner_size(420.0, 560.0)
                            .decorations(false)
                            .skip_taskbar(true)
                            .always_on_top(true)
                            .transparent(true)
                            .visible(false)
                            .build()
                            .expect("Failed to create window"),
                        };

                        // Record that a tray click happened
                        last_focus_lost_for_tray.store(now_ms(), Ordering::SeqCst);

                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                            return;
                        }

                        {
                            let scale = window.scale_factor().unwrap_or(1.0);
                            let window_size = window
                                .outer_size()
                                .unwrap_or(tauri::PhysicalSize::new(420, 560));
                            let w = window_size.width as f64 / scale;
                            let h = window_size.height as f64 / scale;

                            let x = position.x - (w / 2.0);
                            let y = position.y - h;

                            let _ = window
                                .set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                setup_macos_window(&window);
                let _ = window.hide();
            }

            // Auto-start runtime if Docker is not available
            if !runtime::external_docker_available() {
                let resource_dir = app.path().resource_dir().unwrap_or_default();
                let docker_client = app.state::<DockerState>().client.clone();
                let starting = app.state::<RuntimeState>().starting.clone();
                let error = app.state::<RuntimeState>().error.clone();
                let app_handle = app.handle().clone();

                if !starting.load(Ordering::SeqCst) {
                    starting.store(true, Ordering::SeqCst);
                    if let Some(tray) = app_handle.tray_by_id("docker-tray") {
                        let _ = tray.set_tooltip(Some("Docker Tray — Starting runtime..."));
                    }
                    std::thread::spawn(move || {
                        let success = match runtime::start_builtin(&resource_dir) {
                            Ok(_) => {
                                std::thread::sleep(std::time::Duration::from_secs(2));
                                match runtime::connect_docker() {
                                    Some(client) => {
                                        if let Ok(mut guard) = docker_client.lock() {
                                            *guard = Some(client);
                                        }
                                        true
                                    }
                                    None => false
                                }
                            }
                            Err(e) => {
                                if let Ok(mut guard) = error.lock() {
                                    *guard = Some(e);
                                }
                                false
                            }
                        };
                        starting.store(false, Ordering::SeqCst);
                        if let Some(tray) = app_handle.tray_by_id("docker-tray") {
                            let _ = tray.set_tooltip(Some(if success { "Docker Tray" } else { "Docker Tray — Runtime failed" }));
                        }
                        if success {
                            send_notification(&app_handle, "Docker Tray", "Runtime is ready");
                        } else {
                            send_notification(&app_handle, "Docker Tray", "Runtime failed to start");
                        }
                    });
                }
            }

            Ok(())
        })
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                if window.label() == "main" {
                    let w = window.clone();
                    let ts = last_focus_lost.clone();
                    let br = browsing_for_event.clone();
                    // Delay hide to let tray click events arrive first
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        // Skip hide if file picker is open
                        if br.load(Ordering::SeqCst) {
                            return;
                        }
                        let clicked_at = ts.load(Ordering::SeqCst);
                        let elapsed = now_ms() - clicked_at;
                        // If a tray click happened within 150ms, skip hide
                        if elapsed < 200 {
                            return;
                        }
                        let _ = w.hide();
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn open_log_window(
    app: tauri::AppHandle,
    container_id: String,
    container_name: String,
) -> Result<(), String> {
    let label = format!("log-{}", container_id);

    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }

    let url = format!("index.html#/logs/{}/{}", container_id, container_name);

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(format!("Logs: {}", container_name))
        .inner_size(960.0, 600.0)
        .min_inner_size(600.0, 400.0)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn open_file_explorer_window(
    app: tauri::AppHandle,
    container_id: String,
    container_name: String,
) -> Result<(), String> {
    let label = format!("files-{}", container_id);

    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }

    let url = format!(
        "index.html#/files/{}/{}",
        container_id, container_name
    );

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(format!("Files: {}", container_name))
        .inner_size(800.0, 600.0)
        .min_inner_size(500.0, 400.0)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_home_dir() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory".to_string())
}

#[tauri::command]
fn pick_file_for_import(state: tauri::State<'_, BrowsingState>) -> Result<Option<String>, String> {
    use std::process::Command;
    state.0.store(true, Ordering::SeqCst);
    let result = (|| {
        let output = Command::new("osascript")
            .args([
                "-e",
                r#"set f to choose file with prompt "Select file to import"
                return POSIX path of f"#,
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Ok(None);
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
    })();
    let flag = state.0.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        flag.store(false, Ordering::SeqCst);
    });
    result
}

#[tauri::command]
fn pick_yaml_file(state: tauri::State<'_, BrowsingState>) -> Result<Option<String>, String> {
    use std::process::Command;
    state.0.store(true, Ordering::SeqCst);
    let result = (|| {
        let output = Command::new("osascript")
            .args([
                "-e",
                r#"set f to choose file with prompt "Select docker-compose file" of type {"yaml", "yml", "public.yaml", "public.plain-text"}
                return POSIX path of f"#,
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Ok(None);
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
    })();
    let flag = state.0.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        flag.store(false, Ordering::SeqCst);
    });
    result
}

// --- Runtime management ---

use std::sync::Mutex;

struct RuntimeState {
    starting: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

#[tauri::command]
fn runtime_status(
    app: tauri::AppHandle,
    runtime_state: tauri::State<'_, RuntimeState>,
) -> Result<runtime::RuntimeStatus, String> {
    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let mut status = runtime::detect_runtime(&resource_dir);

    // Override with starting state
    if runtime_state.starting.load(Ordering::SeqCst) {
        status.running = false;
        status.message = "Starting runtime...".to_string();
        status.kind = runtime::RuntimeKind::Builtin;
    }

    // Check for errors — override everything
    if let Ok(mut guard) = runtime_state.error.lock() {
        if let Some(err) = guard.take() {
            status.kind = runtime::RuntimeKind::None;
            status.running = false;
            status.message = err;
        }
    }

    Ok(status)
}

#[tauri::command]
fn runtime_start(
    app: tauri::AppHandle,
    docker: tauri::State<'_, DockerState>,
    runtime_state: tauri::State<'_, RuntimeState>,
) -> Result<(), String> {
    if runtime_state.starting.load(Ordering::SeqCst) {
        return Ok(()); // Already starting
    }

    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let docker_client = docker.client.clone();
    let starting = runtime_state.starting.clone();
    let error = runtime_state.error.clone();

    // Clear previous error
    if let Ok(mut guard) = error.lock() {
        *guard = None;
    }
    starting.store(true, Ordering::SeqCst);

    // Update tray tooltip
    if let Some(tray) = app.tray_by_id("docker-tray") {
        let _ = tray.set_tooltip(Some("Docker Tray — Starting runtime..."));
    }

    let app_handle = app.clone();

    // Run in background thread — returns immediately
    std::thread::spawn(move || {
        let success = match runtime::start_builtin(&resource_dir) {
            Ok(_) => {
                // Wait a moment for socket to appear
                std::thread::sleep(std::time::Duration::from_secs(2));
                // Reconnect Docker client
                match runtime::connect_docker() {
                    Some(client) => {
                        if let Ok(mut guard) = docker_client.lock() {
                            *guard = Some(client);
                        }
                        true
                    }
                    None => {
                        if let Ok(mut guard) = error.lock() {
                            *guard = Some("Runtime started but Docker connection failed. Try again.".to_string());
                        }
                        false
                    }
                }
            }
            Err(e) => {
                if let Ok(mut guard) = error.lock() {
                    *guard = Some(e);
                }
                false
            }
        };
        starting.store(false, Ordering::SeqCst);

        // Update tray tooltip
        if let Some(tray) = app_handle.tray_by_id("docker-tray") {
            let _ = tray.set_tooltip(Some(if success {
                "Docker Tray"
            } else {
                "Docker Tray — Runtime failed"
            }));
        }

        // Send macOS notification
        if success {
            send_notification(&app_handle, "Docker Tray", "Runtime is ready");
        } else {
            send_notification(&app_handle, "Docker Tray", "Runtime failed to start");
        }
    });

    Ok(())
}

#[tauri::command]
fn runtime_stop(app: tauri::AppHandle) -> Result<String, String> {
    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    runtime::stop_builtin(&resource_dir)
}

#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}
