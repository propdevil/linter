const STANDARD: &[(&[&str], &str)] = &[
    (&["std::sync::Arc", "alloc::sync::Arc"], "std:Arc"),
    (&["std::sync::Weak", "alloc::sync::Weak"], "std:SyncWeak"),
    (&["std::rc::Rc", "alloc::rc::Rc"], "std:Rc"),
    (&["std::rc::Weak", "alloc::rc::Weak"], "std:RcWeak"),
    (&["std::sync::Mutex"], "std:Mutex"),
    (&["std::sync::RwLock"], "std:RwLock"),
    (
        &["std::cell::RefCell", "core::cell::RefCell"],
        "std:RefCell",
    ),
    (
        &["From", "std::convert::From", "core::convert::From"],
        "std:From",
    ),
    (
        &["TryFrom", "std::convert::TryFrom", "core::convert::TryFrom"],
        "std:TryFrom",
    ),
    (
        &["String", "std::string::String", "alloc::string::String"],
        "std:String",
    ),
    (&["Vec", "std::vec::Vec", "alloc::vec::Vec"], "std:Vec"),
    (
        &["Option", "std::option::Option", "core::option::Option"],
        "std:Option",
    ),
    (
        &["Result", "std::result::Result", "core::result::Result"],
        "std:Result",
    ),
    (&["Box", "std::boxed::Box", "alloc::boxed::Box"], "std:Box"),
];

pub(crate) fn standard(path: &str) -> Option<&'static str> {
    STANDARD
        .iter()
        .find_map(|(aliases, identity)| aliases.contains(&path).then_some(*identity))
}

#[cfg(test)]
mod tests {
    use super::standard;

    #[test]
    fn aliases_share_identity_without_conflating_distinct_types() {
        assert_eq!(standard("std::sync::Arc"), Some("std:Arc"));
        assert_eq!(standard("alloc::sync::Arc"), Some("std:Arc"));
        assert_eq!(standard("std::rc::Weak"), Some("std:RcWeak"));
        assert_eq!(standard("std::sync::Weak"), Some("std:SyncWeak"));
        assert_eq!(standard("Vec"), standard("alloc::vec::Vec"));
        assert_eq!(standard("user::Vec"), None);
        assert_eq!(standard("Arc"), None);
    }
}
