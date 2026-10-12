#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod output;
mod storage;

fn native_error(message: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "legacists".encode_utf16().chain(Some(0)).collect();
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn main() {
    if tauri::webview_version().is_err() {
        native_error("既存のWebView2 Runtimeを利用できません。このアプリはWindows 11 x64と利用可能なWebView2が必要です。管理者に端末環境をご確認ください。");
        return;
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from) else {
        native_error("LEG-6001: ユーザー用データ領域を確認できません。");
        return;
    };
    let storage = match storage::Storage::open(&local) {
        Ok(s) => s,
        Err(_) => {
            native_error("LEG-6001: 保存領域を利用できません。二重起動、権限、所有情報、設定形式を確認してください。");
            return;
        }
    };
    let runtime = storage.root.join("runtime");
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(application::Application::new(storage))
        .invoke_handler(tauri::generate_handler![application::ipc])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title(format!("legacists v{}", env!("CARGO_PKG_VERSION")))
            .inner_size(1440.0, 900.0)
            .min_inner_size(980.0, 640.0)
            .data_directory(runtime.clone())
            .on_navigation(|url| {
                url.host_str()
                    .is_some_and(|host| matches!(host, "tauri.localhost" | "127.0.0.1"))
                    && matches!(url.scheme(), "http" | "https" | "tauri")
            })
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!());
    if result.is_err() {
        native_error("LEG-9001: 起動を完了できませんでした。既存WebView2と保存領域を確認して再起動してください。");
    }
}
