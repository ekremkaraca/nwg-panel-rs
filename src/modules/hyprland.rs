use serde::Deserialize;
use hyprland::data::{Workspace, Client, Workspaces, Clients};
use hyprland::dispatch::{Dispatch, DispatchType};
use hyprland::event_listener::EventListener;
use hyprland::shared::{Address, HyprData, HyprDataActive, HyprDataActiveOptional};
use std::thread;
use crossbeam_channel as cb;

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct HyprWorkspace {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub windows: i32,
    pub hasfullscreen: bool,
    pub lastwindow: String,
    pub lastwindowtitle: String,
    pub ispersistent: bool,
}

impl From<Workspace> for HyprWorkspace {
    fn from(workspace: Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name,
            monitor: workspace.monitor,
            windows: workspace.windows as i32,
            hasfullscreen: workspace.fullscreen,
            lastwindow: workspace.last_window.to_string(),
            lastwindowtitle: workspace.last_window_title,
            ispersistent: false, // Not available in new API
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HyprClient {
    pub address: String,
    pub mapped: bool,
    pub hidden: bool,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub workspace: HyprWorkspaceInfo,
    pub floating: bool,
    pub fullscreen: bool,
    pub fullscreen_mode: i32,
    pub maximized: bool,
    pub focus_history_id: i32,
    pub pid: i32,
    pub xwayland: bool,
    pub title: String,
    pub class: String,
    pub initial_class: String,
}

impl From<Client> for HyprClient {
    fn from(client: Client) -> Self {
        Self {
            address: client.address.to_string(),
            mapped: client.mapped,
            hidden: false, // Not available in new API
            at: (client.at.0.into(), client.at.1.into()),
            size: (client.size.0.into(), client.size.1.into()),
            workspace: HyprWorkspaceInfo {
                id: client.workspace.id,
                name: client.workspace.name,
            },
            floating: client.floating,
            fullscreen: client.fullscreen == hyprland::data::FullscreenMode::Fullscreen,
            fullscreen_mode: 0, // Not available in new API
            maximized: false, // Not available in new API
            focus_history_id: client.focus_history_id as i32,
            pid: client.pid,
            xwayland: client.xwayland,
            title: client.title,
            class: client.class,
            initial_class: client.initial_class,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HyprWorkspaceInfo {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum AppMsg {
    HyprActiveWindow(String),
    HyprWorkspaces {
        workspaces: Vec<HyprWorkspace>,
        active_id: i32,
    },
    HyprActiveWindowAddress(String),
    HyprClients {
        clients: Vec<HyprClient>,
    },
    TrayItemsChanged(Vec<TrayItem>),
    TrayIconUpdated {
        item: TrayItem,
        icon: TrayIconPayload,
    },
}

pub fn spawn_hyprland_poller(sender: cb::Sender<AppMsg>) {
    thread::spawn(move || {
        let mut event_listener = EventListener::new();
        
        // Handle active window changes
        let sender1 = sender.clone();
        event_listener.add_active_window_changed_handler(move |data| {
            let title = data.as_ref().map(|w| w.title.clone()).unwrap_or_default();
            let _ = sender1.send(AppMsg::HyprActiveWindow(title));
            
            if let Some(window) = data {
                let _ = sender1.send(AppMsg::HyprActiveWindowAddress(window.address.to_string()));
            }
        });
        
        // Handle workspace changes
        let sender2 = sender.clone();
        event_listener.add_workspace_changed_handler(move |workspace_id| {
            if let Ok(workspaces) = Workspaces::get() {
                let workspaces_vec: Vec<HyprWorkspace> = workspaces
                    .into_iter()
                    .map(|w| w.into())
                    .collect();
                let _ = sender2.send(AppMsg::HyprWorkspaces {
                    workspaces: workspaces_vec,
                    active_id: workspace_id.id,
                });
            }
        });
        
        // Handle window open/close events
        let sender3 = sender.clone();
        event_listener.add_window_opened_handler(move |_| {
            if let Ok(clients) = Clients::get() {
                let clients_vec: Vec<HyprClient> = clients
                    .into_iter()
                    .map(|c| c.into())
                    .collect();
                let _ = sender3.send(AppMsg::HyprClients { clients: clients_vec });
            }
        });
        
        let sender4 = sender.clone();
        event_listener.add_window_closed_handler(move |_| {
            if let Ok(clients) = Clients::get() {
                let clients_vec: Vec<HyprClient> = clients
                    .into_iter()
                    .map(|c| c.into())
                    .collect();
                let _ = sender4.send(AppMsg::HyprClients { clients: clients_vec });
            }
        });
        
        // Initial data fetch
        if let Ok(Some(active_window)) = Client::get_active() {
            let _ = sender.send(AppMsg::HyprActiveWindow(active_window.title));
            let _ = sender.send(AppMsg::HyprActiveWindowAddress(active_window.address.to_string()));
        }
        
        if let Ok(workspaces) = Workspaces::get() {
            let workspaces_vec: Vec<HyprWorkspace> = workspaces
                .into_iter()
                .map(|w| w.into())
                .collect();
            if let Ok(active_workspace) = Workspace::get_active() {
                let _ = sender.send(AppMsg::HyprWorkspaces {
                    workspaces: workspaces_vec,
                    active_id: active_workspace.id,
                });
            }
        }
        
        if let Ok(clients) = Clients::get() {
            let clients_vec: Vec<HyprClient> = clients
                .into_iter()
                .map(|c| c.into())
                .collect();
            let _ = sender.send(AppMsg::HyprClients { clients: clients_vec });
        }
        
        // Start event listener (blocking)
        let _ = event_listener.start_listener();
    });
}


pub fn hyprctl_dispatch_workspace(id: i32) -> anyhow::Result<()> {
    let workspace_id = hyprland::dispatch::WorkspaceIdentifierWithSpecial::Id(id);
    Dispatch::call(DispatchType::Workspace(workspace_id))?;
    Ok(())
}

pub fn hyprctl_dispatch_focus_address(address: &str) -> anyhow::Result<()> {
    let addr = Address::new(address);
    Dispatch::call(DispatchType::FocusWindow(hyprland::dispatch::WindowIdentifier::Address(addr)))?;
    Ok(())
}

pub fn hyprctl_dispatch_close_address(address: &str) -> anyhow::Result<()> {
    let addr = Address::new(address);
    Dispatch::call(DispatchType::CloseWindow(hyprland::dispatch::WindowIdentifier::Address(addr)))?;
    Ok(())
}

// Tray-related types moved here temporarily
#[derive(Debug, Clone, PartialEq)]
pub struct TrayItem {
    pub service: String,
    pub path: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TrayIconPayload {
    None,
    IconName(String),
    Pixmap(Vec<(i32, Vec<u8>)>),
}

impl TrayItem {
    pub fn as_registration_string(&self) -> String {
        if self.path.starts_with('/') {
            format!("{}{}", self.service, self.path)
        } else {
            format!("{}/{}", self.service, self.path)
        }
    }
}
