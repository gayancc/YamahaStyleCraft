#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use genos_core::{
    document::View,
    edit::{Command, Session},
    midi::{self, Transport},
    project,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Manager, State};

#[derive(Default)]
struct Studio {
    session: Option<Session>,
    transport: Transport,
    epoch: u64,
}
type Shared = Arc<Mutex<Studio>>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn recovery_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(error)?
        .join("recovery.gss"))
}
fn checkpoint(session: &Session, path: &Path) -> View {
    let mut view = session.view();
    let result = if view.dirty {
        project::session_bytes(session).and_then(|b| project::write_recovery(path, &b))
    } else {
        if path.exists() {
            std::fs::remove_file(path).map_err(genos_core::Error::Io)
        } else {
            Ok(())
        }
    };
    if let Err(e) = result {
        view.warnings.push(format!("Autosave failed: {e}"));
    }
    view
}
#[tauri::command]
async fn open_document(path: String, state: State<'_, Shared>) -> Result<View, String> {
    let session =
        tauri::async_runtime::spawn_blocking(move || project::open_file(Path::new(&path)))
            .await
            .map_err(error)?
            .map_err(error)?;
    let mut s = state.lock().map_err(error)?;
    s.transport.stop();
    let view = session.view();
    s.session = Some(session);
    s.epoch += 1;
    Ok(view)
}
#[tauri::command]
async fn edit_document(
    revision: u64,
    command: Command,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let recovery = recovery_path(&app)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(error)?;
        if s.transport.status().playing {
            return Err("Stop playback before editing".into());
        }
        let session = s.session.as_mut().ok_or("Open a Style first")?;
        session.execute(revision, command).map_err(error)?;
        Ok(checkpoint(session, &recovery))
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn history(
    redo: bool,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    let recovery = recovery_path(&app)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(error)?;
        if s.transport.status().playing {
            return Err("Stop playback before editing".into());
        }
        let session = s.session.as_mut().ok_or("Open a Style first")?;
        if redo { session.redo() } else { session.undo() }.map_err(error)?;
        Ok(checkpoint(session, &recovery))
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn save_document(
    path: String,
    project_file: bool,
    state: State<'_, Shared>,
    app: tauri::AppHandle,
) -> Result<View, String> {
    // Snapshot under lock, perform I/O away from UI thread, mark saved only if revision still matches.
    let (bytes, revision, epoch) = {
        let s = state.lock().map_err(error)?;
        let session = s.session.as_ref().ok_or("Open a Style first")?;
        let bytes = if project_file {
            project::session_bytes(session).map_err(error)?
        } else {
            session.document.style.bytes().map_err(error)?
        };
        (bytes, session.revision, s.epoch)
    };
    tauri::async_runtime::spawn_blocking(move || project::write_new(Path::new(&path), &bytes))
        .await
        .map_err(error)?
        .map_err(error)?;
    let mut s = state.lock().map_err(error)?;
    if s.epoch != epoch {
        return Err("File saved, but the active document changed during save".into());
    }
    let session = s.session.as_mut().ok_or("Project changed during save")?;
    if session.revision == revision {
        session.mark_saved().map_err(error)?;
    }
    Ok(checkpoint(session, &recovery_path(&app)?))
}
#[tauri::command]
fn recovery_status(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(recovery_path(&app)?.is_file())
}
#[tauri::command]
async fn restore_recovery(app: tauri::AppHandle, state: State<'_, Shared>) -> Result<View, String> {
    let path = recovery_path(&app)?;
    let mut session = tauri::async_runtime::spawn_blocking(move || project::open_file(&path))
        .await
        .map_err(error)?
        .map_err(error)?;
    session.mark_unsaved();
    let view = session.view();
    let mut s = state.lock().map_err(error)?;
    s.transport.stop();
    s.session = Some(session);
    s.epoch += 1;
    Ok(view)
}
#[tauri::command]
fn discard_recovery(app: tauri::AppHandle) -> Result<(), String> {
    let p = recovery_path(&app)?;
    if p.exists() {
        std::fs::remove_file(p).map_err(error)?;
    }
    Ok(())
}
#[tauri::command]
async fn scan_devices() -> Result<midi::Devices, String> {
    tauri::async_runtime::spawn_blocking(midi::devices)
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
fn play(
    section: u64,
    channels: Vec<u8>,
    index: usize,
    name: String,
    looping: bool,
    state: State<'_, Shared>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(error)?;
    let doc = s
        .session
        .as_ref()
        .ok_or("Open a Style first")?
        .document
        .clone();
    s.transport
        .play(&doc, section, channels, index, name, looping)
        .map_err(error)
}
#[tauri::command]
fn stop(state: State<'_, Shared>) -> Result<(), String> {
    state.lock().map_err(error)?.transport.stop();
    Ok(())
}
#[tauri::command]
fn transport_status(state: State<'_, Shared>) -> Result<midi::Status, String> {
    Ok(state.lock().map_err(error)?.transport.status())
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![
            open_document,
            edit_document,
            history,
            save_document,
            recovery_status,
            restore_recovery,
            discard_recovery,
            scan_devices,
            play,
            stop,
            transport_status
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .state::<Shared>()
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .transport
                    .stop();
            }
        })
        .run(tauri::generate_context!())
        .expect("GenosStyleStudio could not start");
}
