use std::net::TcpListener;

use crate::cdp;
use crate::launcher::listen_pids_on_port;

pub const PREFERRED_DEBUG_PORT: u16 = 9229;
pub const DEBUG_PORT_SCAN_LIMIT: u16 = 32;

pub enum DebugPortMode {
    Attach,
    Launch,
}

pub struct DebugPortResolution {
    pub port: u16,
    pub mode: DebugPortMode,
    pub port_hold: Option<TcpListener>,
}

pub async fn find_attachable_debug_port(preferred: u16, scan_limit: u16) -> Option<u16> {
    for offset in 0..scan_limit {
        let port = preferred.saturating_add(offset);
        if cdp::has_codex_cdp_target(port).await {
            return Some(port);
        }
    }
    None
}

pub fn find_free_debug_port(preferred: u16, scan_limit: u16) -> anyhow::Result<Option<u16>> {
    for offset in 0..scan_limit {
        let port = preferred.saturating_add(offset);
        if listen_pids_on_port(port)?.is_empty() {
            return Ok(Some(port));
        }
    }
    Ok(None)
}

pub fn reserve_ephemeral_port() -> anyhow::Result<(u16, TcpListener)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok((listener.local_addr()?.port(), listener))
}

pub async fn resolve_debug_port(preferred: u16) -> anyhow::Result<DebugPortResolution> {
    if let Some(port) = find_attachable_debug_port(preferred, DEBUG_PORT_SCAN_LIMIT).await {
        return Ok(DebugPortResolution {
            port,
            mode: DebugPortMode::Attach,
            port_hold: None,
        });
    }
    if let Some(port) = find_free_debug_port(preferred, DEBUG_PORT_SCAN_LIMIT)? {
        return Ok(DebugPortResolution {
            port,
            mode: DebugPortMode::Launch,
            port_hold: None,
        });
    }
    let (port, port_hold) = reserve_ephemeral_port()?;
    Ok(DebugPortResolution {
        port,
        mode: DebugPortMode::Launch,
        port_hold: Some(port_hold),
    })
}
