//! X11: read `_NET_WM_STATE` of the window in `_NET_ACTIVE_WINDOW`.
//! Native Wayland has no equivalent, so detection is off there.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

pub struct Detector {
    x: Option<X>,
}

struct X {
    conn: RustConnection,
    root: Window,
    active: Atom,
    state: Atom,
    fullscreen: Atom,
}

impl Detector {
    pub fn new() -> Self {
        Self { x: X::connect() }
    }

    pub fn is_fullscreen_active(&mut self) -> bool {
        self.x.as_ref().and_then(|x| x.check()).unwrap_or(false)
    }
}

impl X {
    fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atom = |name: &str| {
            Some(
                conn.intern_atom(false, name.as_bytes())
                    .ok()?
                    .reply()
                    .ok()?
                    .atom,
            )
        };
        Some(Self {
            active: atom("_NET_ACTIVE_WINDOW")?,
            state: atom("_NET_WM_STATE")?,
            fullscreen: atom("_NET_WM_STATE_FULLSCREEN")?,
            conn,
            root,
        })
    }

    fn check(&self) -> Option<bool> {
        let active = self
            .conn
            .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()?;
        if active == 0 {
            return Some(false);
        }
        let states = self
            .conn
            .get_property(false, active, self.state, AtomEnum::ATOM, 0, 64)
            .ok()?
            .reply()
            .ok()?;
        let fullscreen = states.value32()?.any(|a| a == self.fullscreen);
        Some(fullscreen)
    }
}
