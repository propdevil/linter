pub(crate) fn standard(path: &str) -> Option<&'static str> {
    match path {
        "std::sync::Arc" | "alloc::sync::Arc" => Some("std:Arc"),
        "std::sync::Weak" | "alloc::sync::Weak" => Some("std:SyncWeak"),
        "std::rc::Rc" | "alloc::rc::Rc" => Some("std:Rc"),
        "std::rc::Weak" | "alloc::rc::Weak" => Some("std:RcWeak"),
        "std::sync::Mutex" => Some("std:Mutex"),
        "std::sync::RwLock" => Some("std:RwLock"),
        "std::cell::RefCell" | "core::cell::RefCell" => Some("std:RefCell"),
        "From" | "std::convert::From" | "core::convert::From" => Some("std:From"),
        "TryFrom" | "std::convert::TryFrom" | "core::convert::TryFrom" => Some("std:TryFrom"),
        "String" | "std::string::String" | "alloc::string::String" => Some("std:String"),
        "Vec" | "std::vec::Vec" | "alloc::vec::Vec" => Some("std:Vec"),
        "Option" | "std::option::Option" | "core::option::Option" => Some("std:Option"),
        "Result" | "std::result::Result" | "core::result::Result" => Some("std:Result"),
        "Box" | "std::boxed::Box" | "alloc::boxed::Box" => Some("std:Box"),
        _ => None,
    }
}
