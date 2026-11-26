//! Native Windows 11 adapter for Drink.
//!
//! The portable modules hold the desktop policy, menu text and icon artwork,
//! so they are tested on every platform. The Win32 modules own windows, the
//! notification area and Core Audio sessions.

pub mod controller;
pub mod flavor;
pub mod icon;
pub mod menu;

#[cfg(windows)]
pub mod app;
#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod foreground;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod overlay;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod win;
