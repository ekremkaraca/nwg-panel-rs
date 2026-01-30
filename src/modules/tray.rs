use super::hyprland::{AppMsg, TrayIconPayload, TrayItem};
use crossbeam_channel as cb;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use zbus::connection;
use zbus::fdo::DBusProxy;
use zbus::interface;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::Proxy;

use zbus::blocking::Connection as BlockingConnection;
use zbus::blocking::Proxy as BlockingProxy;

pub struct StatusNotifierWatcher {
    sender: cb::Sender<AppMsg>,
    items: Arc<Mutex<Vec<TrayItem>>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let sender = header.sender().map(|s| s.to_string());
        if let Some(item) = parse_sni_registration(service, sender.as_deref()) {
            let mut items = self.items.lock().await;
            let is_new = !items.iter().any(|x| x == &item);
            if is_new {
                eprintln!(
                    "tray: registered item {} (service='{}', path='{}')",
                    item.as_registration_string(),
                    item.service,
                    item.path
                );
                items.push(item);
                let _ = self.sender.send(AppMsg::TrayItemsChanged(items.clone()));

                // Notify clients that an item was registered.
                let _ = Self::status_notifier_item_registered(&emitter, &items.last().unwrap().as_registration_string()).await;
            }
        }
    }

    async fn register_status_notifier_host(&self, _service: &str) {
        // We act as the host.
    }

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_registered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        // This is only used for DBus property reads; keep it best-effort.
        // If the mutex is contended, return an empty list.
        match self.items.try_lock() {
            Ok(items) => items.iter().map(|x| x.as_registration_string()).collect(),
            Err(_) => Vec::new(),
        }
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
        let rt = match Runtime::new() {
            Ok(rt) => rt,
            Err(err) => {
                eprintln!("Failed to start tokio runtime for tray: {err}");
                return;
            }
        };

        rt.block_on(async move {
            let items = Arc::new(Mutex::new(Vec::<TrayItem>::new()));

            let watcher = StatusNotifierWatcher {
                sender: sender.clone(),
                items: items.clone(),
            };

            // Try to become the watcher. If another watcher already exists (common when
            // running another bar/tray), fall back to a client mode where we read items
            // from the existing watcher.
            let connection = match connection::Builder::session()
                .and_then(|b| b.name("org.kde.StatusNotifierWatcher"))
                .and_then(|b| b.serve_at("/StatusNotifierWatcher", watcher))
            {
                Ok(b) => match b.build().await {
                    Ok(c) => Some(c),
                    Err(err) => {
                        eprintln!("Failed to start SNI watcher DBus service (will try client mode): {err}");
                        None
                    }
                },
                Err(err) => {
                    eprintln!("Failed to start SNI watcher DBus service (will try client mode): {err}");
                    None
                }
            };

            let we_are_watcher = connection.is_some();

            let connection = match connection {
                Some(c) => c,
                None => match zbus::Connection::session().await {
                    Ok(c) => c,
                    Err(err) => {
                        eprintln!("Failed to connect to session bus for tray client mode: {err}");
                        return;
                    }
                },
            };

            // Many SNI clients (notably Qt) expect the watcher to emit HostRegistered once it is
            // available. Do this best-effort immediately after we successfully own the bus name.
            if we_are_watcher {
                if let Ok(iface) = connection
                    .object_server()
                    .interface::<_, StatusNotifierWatcher>("/StatusNotifierWatcher")
                    .await
                {
                    let _ = StatusNotifierWatcher::status_notifier_host_registered(
                        iface.signal_emitter(),
                    )
                    .await;
                }
            }

            // If we couldn't own the watcher name, periodically read items from the existing watcher.
            if !we_are_watcher {
                let items_for_existing = items.clone();
                let sender_for_existing = sender.clone();
                let connection_for_existing = connection.clone();
                tokio::spawn(async move {
                    let proxy = match Proxy::new(
                        &connection_for_existing,
                        "org.kde.StatusNotifierWatcher",
                        "/StatusNotifierWatcher",
                        "org.kde.StatusNotifierWatcher",
                    )
                    .await
                    {
                        Ok(p) => p,
                        Err(_) => return,
                    };

                    loop {
                        let regs = match proxy.get_property::<Vec<String>>("RegisteredStatusNotifierItems").await {
                            Ok(v) => v,
                            Err(_) => {
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                continue;
                            }
                        };

                        eprintln!("tray: client-mode sees {} registered items", regs.len());

                        let mut next_items: Vec<TrayItem> = regs
                            .into_iter()
                            .filter_map(|s| parse_sni_registration(&s, None))
                            .collect();
                        next_items.sort_by(|a, b| a.as_registration_string().cmp(&b.as_registration_string()));
                        next_items.dedup_by(|a, b| a == b);

                        {
                            let mut items = items_for_existing.lock().await;
                            if *items != next_items {
                                *items = next_items;
                                let _ = sender_for_existing.send(AppMsg::TrayItemsChanged(items.clone()));
                            }
                        }

                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                });
            }

            // Task: remove items when their owning name disappears (only effective when we see the names).
            if let Ok(dbus_proxy) = DBusProxy::new(&connection).await {
                let items_for_removal = items.clone();
                let sender_for_removal = sender.clone();
                let connection_for_unreg = connection.clone();
                tokio::spawn(async move {
                    let mut changes = match dbus_proxy.receive_name_owner_changed().await {
                        Ok(s) => s,
                        Err(err) => {
                            eprintln!("Failed to subscribe to NameOwnerChanged: {err}");
                            return;
                        }
                    };

                    use futures_util::StreamExt;
                    while let Some(signal) = changes.next().await {
                        let args = match signal.args() {
                            Ok(a) => a,
                            Err(_) => continue,
                        };

                        // Only care when a name loses its owner.
                        let gone = args.new_owner.as_deref().unwrap_or("").trim().is_empty();
                        if !gone {
                            continue;
                        }

                        let name = args.name.as_str();
                        let mut items = items_for_removal.lock().await;
                        let before = items.len();
                        items.retain(|it| it.service != name);
                        if items.len() != before {
                            let _ = sender_for_removal.send(AppMsg::TrayItemsChanged(items.clone()));

                            // Best-effort emit the unregistered signal (ignore errors).
                            if let Ok(iface) = connection_for_unreg
                                .object_server()
                                .interface::<_, StatusNotifierWatcher>("/StatusNotifierWatcher")
                                .await
                            {
                                let _ = StatusNotifierWatcher::status_notifier_item_unregistered(
                                    iface.signal_emitter(),
                                    name,
                                )
                                .await;
                            }
                        }
                    }
                });
            }

            // Task: best-effort icon refresh loop (polling fallback).
            tokio::spawn(async move {
                let mut last_icon_names = HashMap::<String, String>::new();
                let mut last_icon_pixmap_fp = HashMap::<String, u64>::new();
                loop {
                    let snapshot = { items.lock().await.clone() };
                    for item in snapshot {
                        let key = item.as_registration_string();
                        if let Some(icon) = fetch_sni_icon_best_effort(&connection, &item).await {
                            match &icon {
                                TrayIconPayload::IconName(icon_name) => {
                                    let changed = last_icon_names
                                        .get(&key)
                                        .map(|v| v != icon_name)
                                        .unwrap_or(true);
                                    if changed {
                                        last_icon_names.insert(key.clone(), icon_name.clone());
                                        let _ = sender.send(AppMsg::TrayIconUpdated {
                                            item: item.clone(),
                                            icon,
                                        });
                                    }
                                }
                                TrayIconPayload::Pixmap(pixmaps) => {
                                    let fp = pixmap_fingerprint(pixmaps);
                                    let changed = last_icon_pixmap_fp
                                        .get(&key)
                                        .map(|v| *v != fp)
                                        .unwrap_or(true);
                                    if changed {
                                        last_icon_pixmap_fp.insert(key.clone(), fp);
                                        let _ = sender.send(AppMsg::TrayIconUpdated {
                                            item: item.clone(),
                                            icon,
                                        });
                                    }
                                }
                                TrayIconPayload::None => {}
                            }
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });

            // Keep the service alive.
            std::future::pending::<()>().await;
        });
    });
}

async fn fetch_sni_icon_best_effort(conn: &zbus::Connection, item: &TrayItem) -> Option<TrayIconPayload> {
    let proxy = Proxy::new(
        conn,
        item.service.as_str(),
        item.path.as_str(),
        "org.kde.StatusNotifierItem",
    )
    .await
    .ok()?;

    match proxy.get_property::<String>("IconName").await {
        Ok(name) => {
            if !name.trim().is_empty() {
                return Some(TrayIconPayload::IconName(name));
            }
        }
        Err(err) => {
            eprintln!(
                "tray: failed to read IconName for {}: {err}",
                item.as_registration_string()
            );
        }
    }

    match proxy
        .get_property::<Vec<(i32, i32, Vec<u8>)>>("IconPixmap")
        .await
    {
        Ok(pixmaps) => {
            if pixmaps.is_empty() {
                return None;
            }
            // Filter out obviously invalid entries (some apps may send multiple sizes).
            let filtered: Vec<(i32, i32, Vec<u8>)> = pixmaps
                .into_iter()
                .filter(|(w, h, bytes)| {
                    if *w <= 0 || *h <= 0 {
                        return false;
                    }
                    let expected = (*w as usize) * (*h as usize) * 4;
                    if bytes.len() != expected {
                        eprintln!(
                            "tray: IconPixmap size mismatch for {}: {}x{} bytes={} expected={}",
                            item.as_registration_string(),
                            w,
                            h,
                            bytes.len(),
                            expected
                        );
                        return false;
                    }
                    true
                })
                .collect();
            if filtered.is_empty() {
                return None;
            }
            return Some(TrayIconPayload::Pixmap(filtered));
        }
        Err(err) => {
            eprintln!(
                "tray: failed to read IconPixmap for {}: {err}",
                item.as_registration_string()
            );
        }
    }

    None
}

fn pixmap_fingerprint(pixmaps: &[(i32, i32, Vec<u8>)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (w, h, bytes) in pixmaps {
        w.hash(&mut hasher);
        h.hash(&mut hasher);
        bytes.len().hash(&mut hasher);
        if let Some(b0) = bytes.first() {
            b0.hash(&mut hasher);
        }
        if let Some(b1) = bytes.get(1) {
            b1.hash(&mut hasher);
        }
        if let Some(bn) = bytes.last() {
            bn.hash(&mut hasher);
        }
    }
    hasher.finish()
}

// Parse SNI registration string into service+path components
pub fn parse_sni_registration(service: &str, sender: Option<&str>) -> Option<TrayItem> {
    if service.starts_with('/') {
        // Path-only form: service is the caller unique name.
        let svc = sender?.to_string();
        Some(TrayItem {
            service: svc,
            path: service.to_string(),
        })
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
    let conn = BlockingConnection::session()?;
    let proxy = BlockingProxy::new(
        &conn,
        item.service.as_str(),
        item.path.as_str(),
        "org.kde.StatusNotifierItem",
    )?;
    // Activate(x, y)
    let _: () = proxy.call("Activate", &(0i32, 0i32))?;
    Ok(())
}
