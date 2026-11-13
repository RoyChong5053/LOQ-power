use loq_core::Capabilities;
use zbus::Connection;

const SERVICE: &str = "org.loq.Power1";
const PATH: &str = "/org/loq/Power1";
const INTERFACE: &str = "org.loq.Power1";

/// Return the full read-only capability manifest.
#[tauri::command]
fn get_capabilities() -> Capabilities {
    loq_core::probe()
}

/// Return the crate version so the UI can display it.
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

async fn daemon_connection() -> Result<Connection, String> {
    Connection::system()
        .await
        .map_err(|error| format!("无法连接系统总线: {error}"))
}

async fn name_has_owner(connection: &Connection, name: &str) -> Result<bool, String> {
    let reply = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(name,),
        )
        .await
        .map_err(|error| format!("无法查询总线名称: {error}"))?;

    reply
        .body()
        .deserialize::<bool>()
        .map_err(|error| format!("无法解析总线回复: {error}"))
}

/// JSON snapshot from `loq-powerd`, or `null` when the daemon is not running.
#[tauri::command]
async fn get_daemon_state() -> Result<Option<serde_json::Value>, String> {
    let connection = daemon_connection().await?;
    if !name_has_owner(&connection, SERVICE).await? {
        return Ok(None);
    }

    let reply = connection
        .call_method(Some(SERVICE), PATH, Some(INTERFACE), "GetState", &())
        .await
        .map_err(|error| format!("无法读取守护进程状态: {error}"))?;

    let raw: String = reply
        .body()
        .deserialize()
        .map_err(|error| format!("无法解析守护进程状态: {error}"))?;

    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| format!("守护进程返回了无效 JSON: {error}"))
}

/// Apply an `ApplyRequest` through the daemon. Polkit may prompt the user.
#[tauri::command]
async fn apply_request(request_json: String) -> Result<serde_json::Value, String> {
    let connection = daemon_connection().await?;
    if !name_has_owner(&connection, SERVICE).await? {
        return Err("loq-powerd 未运行，请先启动守护进程".to_string());
    }

    // Fail early on malformed input so the user gets a clear message.
    serde_json::from_str::<loq_core::ApplyRequest>(&request_json)
        .map_err(|error| format!("请求格式无效: {error}"))?;

    let reply = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "Apply",
            &(request_json.as_str(),),
        )
        .await
        .map_err(|error| format!("应用失败: {error}"))?;

    let raw: String = reply
        .body()
        .deserialize()
        .map_err(|error| format!("无法解析守护进程回复: {error}"))?;

    serde_json::from_str(&raw).map_err(|error| format!("守护进程返回了无效 JSON: {error}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_capabilities,
            get_version,
            get_daemon_state,
            apply_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to start LOQ Power");
}
