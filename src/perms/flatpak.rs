//! The `flatpak` backend: shiro drives `flatpak override` rather than inventing
//! storage of its own, so that what is on disk stays readable by the tool that
//! owns it.
