use std::collections::BTreeSet;

pub(super) const CONTEXTS: &[&str] =
    &["tokio::task::spawn_blocking", "tokio::task::block_in_place"];
pub(super) const ADAPTERS: &[&str] = &["unwrap", "expect"];

pub(super) fn functions() -> BTreeSet<String> {
    [
        "std::thread::sleep",
        "std::fs::read",
        "std::fs::read_to_string",
        "std::fs::write",
        "std::fs::File::open",
        "std::fs::File::create",
    ]
    .map(String::from)
    .into()
}

pub(super) struct Methods {
    pub receiver: &'static str,
    pub methods: &'static [&'static str],
    pub constructors: &'static [&'static str],
    pub fluent_methods: &'static [&'static str],
    pub returns_guard: bool,
}

pub(super) const METHODS: &[Methods] = &[
    Methods {
        receiver: "std::process::Command",
        methods: &["spawn", "status", "output"],
        constructors: &["std::process::Command::new"],
        fluent_methods: &["arg", "args"],
        returns_guard: false,
    },
    Methods {
        receiver: "std::fs::OpenOptions",
        methods: &["open"],
        constructors: &["std::fs::OpenOptions::new"],
        fluent_methods: &["read", "write", "create"],
        returns_guard: false,
    },
    Methods {
        receiver: "std::sync::Mutex",
        methods: &["lock"],
        constructors: &["std::sync::Mutex::new"],
        fluent_methods: &[],
        returns_guard: true,
    },
    Methods {
        receiver: "std::sync::RwLock",
        methods: &["read", "write"],
        constructors: &["std::sync::RwLock::new"],
        fluent_methods: &[],
        returns_guard: true,
    },
    Methods {
        receiver: "parking_lot::Mutex",
        methods: &["lock"],
        constructors: &["parking_lot::Mutex::new"],
        fluent_methods: &[],
        returns_guard: true,
    },
    Methods {
        receiver: "tokio::sync::Mutex",
        methods: &["blocking_lock"],
        constructors: &["tokio::sync::Mutex::new"],
        fluent_methods: &[],
        returns_guard: false,
    },
    Methods {
        receiver: "tokio::sync::mpsc::Receiver",
        methods: &["blocking_recv"],
        constructors: &[],
        fluent_methods: &[],
        returns_guard: false,
    },
];
