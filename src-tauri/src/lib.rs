#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use arboard::Clipboard;
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

type Db = Arc<Mutex<Connection>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub kind: String,
    pub content: String,
    pub preview: String,
    pub created_at: String,
    pub favorite: bool,
    pub source_app: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub history_limit: u32,
    pub retention_days: u32,
    pub max_image_mb: u32,
    pub monitoring_paused: bool,
    pub excluded_apps: String,
    pub language: String,
    pub auto_paste: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            history_limit: 1000,
            retention_days: 90,
            max_image_mb: 10,
            monitoring_paused: false,
            excluded_apps: String::new(),
            language: "ru".into(),
            auto_paste: false,
        }
    }
}

fn init_db(path: &std::path::Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS entries(id INTEGER PRIMARY KEY, kind TEXT NOT NULL, content TEXT NOT NULL, preview TEXT NOT NULL, created_at TEXT NOT NULL, favorite INTEGER NOT NULL DEFAULT 0, source_app TEXT, hash TEXT NOT NULL UNIQUE);
      CREATE INDEX IF NOT EXISTS entries_created ON entries(created_at DESC);
      CREATE TABLE IF NOT EXISTS snippets(id INTEGER PRIMARY KEY, title TEXT NOT NULL, content TEXT NOT NULL, updated_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
      INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(1,CURRENT_TIMESTAMP);
      PRAGMA user_version=1;").map_err(|e| e.to_string())?;
    Ok(db)
}

#[tauri::command]
fn list_entries(
    db: State<Db>,
    filter: Option<String>,
    query: Option<String>,
    favorites_only: Option<bool>,
) -> Result<Vec<Entry>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = db.prepare("SELECT id,kind,content,preview,created_at,favorite,source_app FROM entries WHERE (?1 IS NULL OR kind=?1) AND (?2 IS NULL OR preview LIKE '%'||?2||'%' OR content LIKE '%'||?2||'%') AND (?3=0 OR favorite=1) ORDER BY created_at DESC LIMIT 10000").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![
                filter,
                query.filter(|q| !q.is_empty()),
                favorites_only.unwrap_or(false) as i32
            ],
            |r| {
                Ok(Entry {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    content: r.get(2)?,
                    preview: r.get(3)?,
                    created_at: r.get(4)?,
                    favorite: r.get::<_, i32>(5)? != 0,
                    source_app: r.get(6)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn insert_entry(
    db: &Connection,
    kind: &str,
    content: &str,
    preview: &str,
    hash: &str,
    source_app: Option<&str>,
) -> Result<i64, String> {
    db.execute("INSERT OR IGNORE INTO entries(kind,content,preview,created_at,hash,source_app) VALUES(?1,?2,?3,?4,?5,?6)", params![kind,content,preview,Utc::now().to_rfc3339(),hash,source_app]).map_err(|e| e.to_string())?;
    db.query_row("SELECT id FROM entries WHERE hash=?1", [hash], |r| r.get(0))
        .map_err(|e| e.to_string())
}
fn content_hash(kind: &str, bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!(
        "{kind}:{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn is_excluded_process(process: &str, exclusions: &str) -> bool {
    let name = process.rsplit(['\\', '/']).next().unwrap_or(process);
    exclusions
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|item| item.eq_ignore_ascii_case(name))
}
fn settings_from_db(db: &Connection) -> Settings {
    db.query_row("SELECT value FROM settings WHERE key='app'", [], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default()
}

#[tauri::command]
fn toggle_favorite(db: State<Db>, id: i64) -> Result<(), String> {
    db.lock()
        .map_err(|e| e.to_string())?
        .execute("UPDATE entries SET favorite=1-favorite WHERE id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_entry(db: State<Db>, id: i64) -> Result<(), String> {
    db.lock()
        .map_err(|e| e.to_string())?
        .execute("DELETE FROM entries WHERE id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn clear_history(db: State<Db>) -> Result<(), String> {
    db.lock()
        .map_err(|e| e.to_string())?
        .execute("DELETE FROM entries WHERE favorite=0", [])
        .map(|_| ())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn restore_entry(id: i64, db: State<Db>) -> Result<(), String> {
    let (kind, content): (String, String) = db
        .lock()
        .map_err(|e| e.to_string())?
        .query_row("SELECT kind,content FROM entries WHERE id=?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| e.to_string())?;
    if kind == "text" || kind == "link" || kind == "code" {
        Clipboard::new()
            .and_then(|mut c| c.set_text(content))
            .map_err(|e| e.to_string())
    } else if kind == "file" {
        let paths: Vec<String> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        clipboard_win::raw::set_file_list(&paths).map_err(|e| e.to_string())
    } else if kind == "image" {
        let encoded = content
            .strip_prefix("data:image/png;base64,")
            .ok_or("Неподдерживаемый формат изображения")?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .map_err(|e| e.to_string())?;
        let decoded = image::load_from_memory(&bytes)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        let (width, height) = decoded.dimensions();
        Clipboard::new()
            .and_then(|mut c| {
                c.set_image(arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: std::borrow::Cow::Owned(decoded.into_raw()),
                })
            })
            .map_err(|e| e.to_string())
    } else {
        Err("Неизвестный тип элемента".into())
    }
}

#[tauri::command]
fn copy_entry_path(db: State<Db>, id: i64) -> Result<(), String> {
    let (kind, content, source_app): (String, String, Option<String>) = db
        .lock()
        .map_err(|e| e.to_string())?
        .query_row(
            "SELECT kind,content,source_app FROM entries WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let path = if kind == "file" {
        serde_json::from_str::<Vec<String>>(&content)
            .map_err(|e| e.to_string())?
            .join("\r\n")
    } else {
        source_app.ok_or_else(|| "Для этой записи путь недоступен".to_string())?
    };
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(path))
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn list_snippets(db: State<Db>) -> Result<Vec<Snippet>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let mut s = db
        .prepare("SELECT id,title,content,updated_at FROM snippets ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = s
        .query_map([], |r| {
            Ok(Snippet {
                id: r.get(0)?,
                title: r.get(1)?,
                content: r.get(2)?,
                updated_at: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn save_snippet(
    db: State<Db>,
    id: Option<i64>,
    title: String,
    content: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    if let Some(id) = id {
        db.execute(
            "UPDATE snippets SET title=?1,content=?2,updated_at=?3 WHERE id=?4",
            params![title, content, Utc::now().to_rfc3339(), id],
        )
    } else {
        db.execute(
            "INSERT INTO snippets(title,content,updated_at) VALUES(?1,?2,?3)",
            params![title, content, Utc::now().to_rfc3339()],
        )
    }
    .map(|_| ())
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_snippet(db: State<Db>, id: i64) -> Result<(), String> {
    db.lock()
        .map_err(|e| e.to_string())?
        .execute("DELETE FROM snippets WHERE id=?1", [id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn get_settings(db: State<Db>) -> Result<Settings, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let raw: Option<String> = db
        .query_row("SELECT value FROM settings WHERE key='app'", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}
#[tauri::command]
fn save_settings(db: State<Db>, settings: Settings) -> Result<(), String> {
    let value = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| e.to_string())?;
    db.execute("INSERT INTO settings(key,value) VALUES('app',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[value]).map_err(|e|e.to_string())?;
    db.execute("DELETE FROM entries WHERE favorite=0 AND id NOT IN (SELECT id FROM entries ORDER BY created_at DESC LIMIT ?1)",[settings.history_limit.max(1)]).map_err(|e|e.to_string())?;
    db.execute(
        "DELETE FROM entries WHERE favorite=0 AND created_at < ?1",
        [(Utc::now() - Duration::days(settings.retention_days as i64)).to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
unsafe fn clipboard_owner_process() -> Option<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            DataExchange::GetClipboardOwner,
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::WindowsAndMessaging::GetWindowThreadProcessId,
    };
    let hwnd = GetClipboardOwner();
    if hwnd.is_null() {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 {
        return None;
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
    CloseHandle(process);
    if ok == 0 {
        return None;
    }
    String::from_utf16(&buffer[..length as usize]).ok()
}

#[cfg(windows)]
unsafe fn clipboard_has_sensitive_format() -> bool {
    use windows_sys::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };
    let name: Vec<u16> = "ExcludeClipboardContentFromMonitorProcessing\0"
        .encode_utf16()
        .collect();
    let format = RegisterClipboardFormatW(name.as_ptr());
    format != 0 && IsClipboardFormatAvailable(format) != 0
}

#[cfg(windows)]
fn start_clipboard_listener(app: tauri::AppHandle, db: Db, monitoring_enabled: bool) {
    if !monitoring_enabled {
        return;
    }
    std::thread::spawn(move || unsafe {
        use std::ptr::null_mut;
        use windows_sys::Win32::{
            Foundation::*,
            System::{DataExchange::*, LibraryLoader::*},
            UI::WindowsAndMessaging::*,
        };
        unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        let class: Vec<u16> = "ClipVaultClipboardListener\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            lpszClassName: class.as_ptr(),
            hInstance: GetModuleHandleW(null_mut()),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            wc.hInstance,
            null_mut(),
        );
        if hwnd.is_null() || AddClipboardFormatListener(hwnd) == 0 {
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_CLIPBOARDUPDATE {
                let settings = db.lock().map(|c| settings_from_db(&c)).unwrap_or_default();
                if settings.monitoring_paused {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    continue;
                }
                let source = clipboard_owner_process();
                if source
                    .as_deref()
                    .is_some_and(|path| is_excluded_process(path, &settings.excluded_apps))
                {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    continue;
                }
                if clipboard_has_sensitive_format() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    continue;
                }
                if let Ok(paths) =
                    clipboard_win::get_clipboard::<Vec<String>, _>(clipboard_win::formats::FileList)
                {
                    if !paths.is_empty() {
                        let content = serde_json::to_string(&paths).unwrap_or_default();
                        let preview = paths
                            .iter()
                            .take(3)
                            .map(|p| {
                                std::path::Path::new(p)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(p)
                            })
                            .collect::<Vec<_>>()
                            .join(" · ");
                        if let Ok(conn) = db.lock() {
                            let _ = insert_entry(
                                &conn,
                                "file",
                                &content,
                                &format!("{} файл(ов): {}", paths.len(), preview),
                                &content_hash("file", content.as_bytes()),
                                source.as_deref(),
                            );
                        }
                        let _ = app.emit("clipboard-entry", ());
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                        continue;
                    }
                }
                let mut clipboard = match Clipboard::new() {
                    Ok(c) => c,
                    Err(_) => {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                        continue;
                    }
                };
                if let Ok(text) = clipboard.get_text() {
                    if !text.trim().is_empty() {
                        let kind = if text.starts_with("http://") || text.starts_with("https://") {
                            "link"
                        } else if text.lines().count() > 2
                            && (text.contains("fn ")
                                || text.contains("function ")
                                || text.contains("const ")
                                || text.contains("class ")
                                || text.contains("=>"))
                        {
                            "code"
                        } else {
                            "text"
                        };
                        let hash = content_hash(kind, text.as_bytes());
                        if let Ok(conn) = db.lock() {
                            let _ = insert_entry(
                                &conn,
                                kind,
                                &text,
                                &text.chars().take(240).collect::<String>(),
                                &hash,
                                source.as_deref(),
                            );
                        }
                        let _ = app.emit("clipboard-entry", ());
                    }
                } else if let Ok(image) = clipboard.get_image() {
                    let max_mb = settings.max_image_mb;
                    let limit = max_mb as usize * 1024 * 1024;
                    if image.bytes.len() <= limit {
                        let hash = content_hash("image", &image.bytes);
                        if let Some(rgba) = image::RgbaImage::from_raw(
                            image.width as u32,
                            image.height as u32,
                            image.bytes.into_owned(),
                        ) {
                            let mut out = std::io::Cursor::new(Vec::new());
                            if image::DynamicImage::ImageRgba8(rgba)
                                .write_to(&mut out, image::ImageFormat::Png)
                                .is_ok()
                            {
                                let encoded = base64::Engine::encode(
                                    &base64::engine::general_purpose::STANDARD,
                                    out.get_ref(),
                                );
                                let content = format!("data:image/png;base64,{}", encoded);
                                if let Ok(conn) = db.lock() {
                                    let _ = insert_entry(
                                        &conn,
                                        "image",
                                        &content,
                                        &format!("{} × {} px", image.width, image.height),
                                        &hash,
                                        source.as_deref(),
                                    );
                                }
                                let _ = app.emit("clipboard-entry", ());
                            }
                        }
                    }
                }
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        RemoveClipboardFormatListener(hwnd);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_clipboard_events_create_one_row_and_preserve_unicode() {
        let path =
            std::env::temp_dir().join(format!("clipvault-test-{}.sqlite", std::process::id()));
        let db = init_db(&path).expect("temporary test database");
        let sample = "  Привет, 世界 🌍\n";
        let first = insert_entry(&db, "text", sample, sample, "text-hash", None).unwrap();
        let second = insert_entry(&db, "text", sample, sample, "text-hash", None).unwrap();
        assert_eq!(first, second);
        let count: i64 = db
            .query_row("SELECT count(*) FROM entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let value: String = db
            .query_row("SELECT content FROM entries WHERE id=?1", [first], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(value, sample);
        let migration: i64 = db
            .query_row(
                "SELECT count(*) FROM schema_migrations WHERE version=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(migration, 1);
        drop(db);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn settings_default_to_local_russian_configuration() {
        let settings = Settings::default();
        assert_eq!(settings.language, "ru");
        assert_eq!(settings.history_limit, 1000);
        assert!(!settings.monitoring_paused);
    }
    #[test]
    fn process_exclusions_match_case_insensitive_executable_names() {
        assert!(is_excluded_process(
            "C:\\Tools\\KeePass.EXE",
            "keepass.exe, banking.exe"
        ));
        assert!(!is_excluded_process(
            "editor.exe",
            "keepass.exe, banking.exe"
        ));
    }
}

#[cfg(not(windows))]
fn start_clipboard_listener(_app: tauri::AppHandle, _db: Db, _monitoring_enabled: bool) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let dir = if let Some(path) = std::env::var_os("CLIPVAULT_DATA_DIR") {
                std::path::PathBuf::from(path)
            } else {
                app.path().app_data_dir()?
            };
            std::fs::create_dir_all(&dir)?;
            let db = Arc::new(Mutex::new(
                init_db(&dir.join("clipvault.sqlite3")).map_err(std::io::Error::other)?,
            ));
            app.manage(db.clone());
            let monitoring_enabled = !std::env::args().any(|arg| arg == "--safe-mode");
            start_clipboard_listener(app.handle().clone(), db.clone(), monitoring_enabled);
            let handle = app.handle().clone();
            use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            let primary_handle = handle.clone();
            let register = app.global_shortcut().on_shortcut(
                shortcut,
                move |_app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        if let Some(w) = primary_handle.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                },
            );
            if let Err(error) = register {
                log::warn!("Global shortcut Ctrl+Shift+V is unavailable: {error}; trying Ctrl+Alt+V");
                let fallback_handle = handle.clone();
                let fallback = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV);
                if let Err(fallback_error) = app.global_shortcut().on_shortcut(
                    fallback,
                    move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            if let Some(w) = fallback_handle.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    },
                ) {
                    log::warn!("Fallback global shortcut Ctrl+Alt+V is unavailable: {fallback_error}; use the tray menu");
                }
            }
            let show = app.handle().clone();
            let quit = app.handle().clone();
            let menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &tauri::menu::MenuItem::with_id(
                        app,
                        "show",
                        "Открыть ClipVault",
                        true,
                        None::<&str>,
                    )?,
                    &tauri::menu::MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?,
                ],
            )?;
            tauri::tray::TrayIconBuilder::new()
                .tooltip("ClipVault")
                .menu(&menu)
                .on_menu_event(move |_app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = show.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => quit.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_entries,
            toggle_favorite,
            delete_entry,
            clear_history,
            restore_entry,
            copy_entry_path,
            list_snippets,
            save_snippet,
            delete_snippet,
            get_settings,
            save_settings
        ]);
    builder
        .run(tauri::generate_context!())
        .expect("ошибка запуска ClipVault");
}
