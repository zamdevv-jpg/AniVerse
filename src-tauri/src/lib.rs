use std::sync::{Arc, Mutex};
use tauri::Manager;

struct AppServerState {
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    server_thread_stop: Arc<Mutex<Option<std::sync::mpsc::Sender<()>>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (port_tx, port_rx) = std::sync::mpsc::sync_channel::<u16>(1);
    let (shutdown_sender_tx, shutdown_sender_rx) =
        std::sync::mpsc::sync_channel::<tokio::sync::oneshot::Sender<()>>(1);
    let (server_thread_stop, server_thread_stop_rx) = std::sync::mpsc::channel::<()>();
    // Keep the local origin stable so service-worker offline pages survive restarts.
    // If the port is occupied, start_server falls back to an available port.
    let desired_port = 3000;

    // Spawn the embedded Axum backend on a dedicated background thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("gagal membuat tokio runtime");

        rt.block_on(async move {
            match otakunime::start_server(desired_port, None, None).await {
                Ok((bound_port, shutdown_tx)) => {
                    let _ = port_tx.send(bound_port);
                    let _ = shutdown_sender_tx.send(shutdown_tx);
                    // The desktop/mobile window owns the lifetime of this runtime.
                    let _ = server_thread_stop_rx.recv();
                }
                Err(e) => {
                    eprintln!("Error starting internal AniVerse server: {e}");
                    let _ = port_tx.send(0);
                }
            }
        });
    });

    let port = port_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap_or(0);

    let shutdown_tx = shutdown_sender_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .ok();

    let server_state = AppServerState {
        shutdown_tx: Arc::new(Mutex::new(shutdown_tx)),
        server_thread_stop: Arc::new(Mutex::new(Some(server_thread_stop))),
    };

    tauri::Builder::default()
        .manage(server_state)
        .setup(move |app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            if let Some(window) = app.get_webview_window("main") {
                if port == 0 {
                    log::error!("backend startup timed out; leaving the startup screen visible");
                    return Ok(());
                }
                let target_url = format!("http://127.0.0.1:{port}");
                let url = target_url.parse()?;
                window.navigate(url)?;
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    let state = window.state::<AppServerState>();
                    if let Ok(mut lock) = state.shutdown_tx.lock() {
                        if let Some(tx) = lock.take() {
                            let _ = tx.send(());
                        }
                    }
                    if let Ok(mut lock) = state.server_thread_stop.lock() {
                        if let Some(stop) = lock.take() {
                            let _ = stop.send(());
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
