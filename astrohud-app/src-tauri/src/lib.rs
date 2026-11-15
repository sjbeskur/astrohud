mod core;

#[cfg(feature = "app")]
mod app {
    use super::core::{ApiRequest, ApiResponse, ConnectionInfo, Session};
    use std::sync::Mutex;
    struct AppState(Mutex<Session>);
    #[tauri::command]
    fn connection_info(state: tauri::State<'_, AppState>) -> Result<ConnectionInfo, String> {
        Ok(state
            .0
            .lock()
            .map_err(|_| "Connection unavailable.")?
            .info())
    }
    #[tauri::command]
    fn set_server(
        server: String,
        state: tauri::State<'_, AppState>,
    ) -> Result<ConnectionInfo, String> {
        let session = Session::new(&server)?;
        let info = session.info();
        *state.0.lock().map_err(|_| "Connection unavailable.")? = session;
        Ok(info)
    }
    #[tauri::command]
    fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
        let mut session = state.0.lock().map_err(|_| "Connection unavailable.")?;
        *session = Session::new(session.server.as_str())?;
        Ok(())
    }
    #[tauri::command]
    async fn api_request(
        request: ApiRequest,
        state: tauri::State<'_, AppState>,
    ) -> Result<ApiResponse, String> {
        let session = state
            .0
            .lock()
            .map_err(|_| "Connection unavailable.")?
            .clone();
        session.request(request).await
    }
    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub fn run() {
        let default_server = if cfg!(debug_assertions) {
            "http://127.0.0.1:18116"
        } else {
            "https://app.astrohud.com"
        };
        let server =
            std::env::var("ASTROHUD_CLIENT_SERVER_URL").unwrap_or_else(|_| default_server.into());
        let session = Session::new(&server).expect("valid client server origin");
        tauri::Builder::default()
            .manage(AppState(Mutex::new(session)))
            .invoke_handler(tauri::generate_handler![
                connection_info,
                set_server,
                disconnect,
                api_request
            ])
            .setup(|app| {
                tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                    .on_navigation(|url| {
                        (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                            || (matches!(url.scheme(), "http" | "https")
                                && url.host_str() == Some("tauri.localhost"))
                    })
                    .build()?;
                Ok(())
            })
            .run(tauri::generate_context!())
            .expect("Memory Lane desktop client failed");
    }
}
#[cfg(feature = "app")]
pub use app::run;
