use std::net::TcpListener;

pub const PREFERRED_DEBUG_PORT: u16 = 9229;
pub const DEBUG_PORT_SCAN_END: u16 = 9260;

pub struct DebugPortResolution {
    pub port: u16,
    pub port_hold: Option<TcpListener>,
}

fn reserve_port(port: u16) -> anyhow::Result<TcpListener> {
    Ok(TcpListener::bind(("127.0.0.1", port))?)
}

pub fn reserve_ephemeral_port() -> anyhow::Result<(u16, TcpListener)> {
    let listener = reserve_port(0)?;
    Ok((listener.local_addr()?.port(), listener))
}

pub async fn resolve_debug_port(preferred: u16) -> anyhow::Result<DebugPortResolution> {
    for port in debug_port_scan_candidates(preferred) {
        if let Ok(port_hold) = reserve_port(port) {
            return Ok(DebugPortResolution {
                port,
                port_hold: Some(port_hold),
            });
        }
    }
    let (port, port_hold) = reserve_ephemeral_port()?;
    Ok(DebugPortResolution {
        port,
        port_hold: Some(port_hold),
    })
}

pub fn debug_port_scan_candidates(preferred: u16) -> impl Iterator<Item = u16> {
    preferred..=DEBUG_PORT_SCAN_END
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_debug_port_reserves_managed_launch_port() {
        let Some((preferred, preferred_hold)) = debug_port_scan_candidates(PREFERRED_DEBUG_PORT)
            .find_map(|port| reserve_port(port).ok().map(|hold| (port, hold)))
        else {
            return;
        };
        drop(preferred_hold);

        let resolved = resolve_debug_port(preferred).await.unwrap();

        assert_eq!(resolved.port, preferred);
        assert!(resolved.port_hold.is_some());
    }

    #[test]
    fn debug_port_scan_candidates_cover_helper_range() {
        let ports = debug_port_scan_candidates(PREFERRED_DEBUG_PORT).collect::<Vec<_>>();

        assert_eq!(ports.first(), Some(&9229));
        assert_eq!(ports.last(), Some(&9260));
    }
}
