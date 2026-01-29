use super::hyprland::{AppMsg, TrayIconPayload, TrayItem};
use crossbeam_channel as cb;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use zbus::blocking::connection;
use zbus::blocking::Proxy;
use zbus::interface;

pub struct StatusNotifierWatcher {
    sender: cb::Sender<AppMsg>,
    items: Arc<Mutex<Vec<TrayItem>>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    fn register_status_notifier_item(&self, service: &str) {
        if let Some(item) = parse_sni_registration(service, None) {
            let mut items = self.items.lock().unwrap();
            if !items.iter().any(|x| x == &item) {
                items.push(item);
                let _ = self.sender.send(AppMsg::TrayItemsChanged(items.clone()));
            }
        }
    }

    fn register_status_notifier_host(&self, _service: &str) {
        // We act as the host.
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .map(|x| x.as_registration_string())
            .collect()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

pub fn spawn_sni_watcher(sender: cb::Sender<AppMsg>) {
    thread::spawn(move || {
        let items = Arc::new(Mutex::new(Vec::<TrayItem>::new()));

        let watcher = StatusNotifierWatcher {
            sender: sender.clone(),
            items: items.clone(),
        };

        let connection = match connection::Builder::session()
            .and_then(|b| b.name("org.kde.StatusNotifierWatcher"))
            .and_then(|b| b.serve_at("/StatusNotifierWatcher", watcher))
            .and_then(|b| b.build())
        {
            Ok(c) => c,
            Err(err) => {
                eprintln!("Failed to start SNI watcher DBus service: {err}");
                return;
            }
        };

        let mut last_icon_names = HashMap::<String, String>::new();

        // Best-effort icon refresh loop.
        loop {
            let snapshot = { items.lock().unwrap().clone() };
            for item in snapshot {
                if let Some(icon_name) = fetch_sni_icon_name_best_effort(&connection, &item) {
                    let key = item.as_registration_string();
                    let changed = last_icon_names
                        .get(&key)
                        .map(|v| v != &icon_name)
                        .unwrap_or(true);
                    if changed {
                        last_icon_names.insert(key, icon_name.clone());
                        let _ = sender.send(AppMsg::TrayIconUpdated {
                            item: item.clone(),
                            icon: TrayIconPayload::IconName(icon_name),
                        });
                    }
                }
            }

            thread::sleep(Duration::from_secs(2));
        }
    });
}

fn fetch_sni_icon_name_best_effort(conn: &zbus::blocking::Connection, item: &TrayItem) -> Option<String> {
    let proxy = Proxy::new(
        conn,
        item.service.as_str(),
        item.path.as_str(),
        "org.kde.StatusNotifierItem",
    )
    .ok()?;
    proxy.get_property::<String>("IconName").ok().filter(|s| !s.trim().is_empty())
}

// Parse SNI registration string into service+path components
pub fn parse_sni_registration(service: &str, _sender: Option<&str>) -> Option<TrayItem> {
    if service.starts_with('/') {
        // Path-only form requires the DBus caller unique name; we don't support this yet.
        None
    } else {
        // Full form or default path
        let parts: Vec<&str> = service.splitn(2, '/').collect();
        let (svc, path) = match parts.as_slice() {
            [svc] => (svc.to_string(), "/StatusNotifierItem".to_string()),
            [svc, path] => (svc.to_string(), format!("/{}", path)),
            _ => return None,
        };
        Some(TrayItem { service: svc, path })
    }
}

pub fn activate_sni_item(item: &TrayItem) -> anyhow::Result<()> {
    let conn = zbus::blocking::Connection::session()?;
    let proxy = Proxy::new(
        &conn,
        item.service.as_str(),
        item.path.as_str(),
        "org.kde.StatusNotifierItem",
    )?;
    // Activate(x, y)
    let _: () = proxy.call("Activate", &(0i32, 0i32))?;
    Ok(())
}
