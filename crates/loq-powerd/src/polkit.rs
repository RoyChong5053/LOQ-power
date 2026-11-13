use std::collections::HashMap;

use zbus::Connection;
use zbus::zvariant::Value;

/// Ask polkit whether `sender` may perform `action`.
///
/// Interactive authentication is allowed (`AllowUserInteraction`), which lets
/// a desktop authentication agent show a password prompt.
pub async fn check(conn: &Connection, sender: Option<&str>, action: &str) -> zbus::Result<bool> {
    let Some(sender) = sender else {
        return Ok(false);
    };

    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.PolicyKit1",
        "/org/freedesktop/PolicyKit1/Authority",
        "org.freedesktop.PolicyKit1.Authority",
    )
    .await?;

    let subject_details: HashMap<String, Value<'static>> =
        HashMap::from([("name".to_string(), Value::from(sender.to_string()))]);
    let subject = ("system-bus-name".to_string(), subject_details);
    let details: HashMap<String, String> = HashMap::new();

    // CheckAuthorization(subject (sa{sv}), action_id s, details a{ss},
    //                    flags u, cancellation_id s)
    //   -> is_authorized b, is_challenge b, details a{ss}
    let (authorized, _challenge, _details): (bool, bool, HashMap<String, String>) = proxy
        .call(
            "CheckAuthorization",
            &(subject, action, details, 1u32, String::new()),
        )
        .await?;

    tracing::debug!(sender, action, authorized, challenge = _challenge, details = ?_details, "polkit check");

    Ok(authorized)
}
