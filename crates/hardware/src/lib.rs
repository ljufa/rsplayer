//! Hardware integrations: volume-control devices, the USB front-panel
//! link, the optional LIRC infrared remote service, and platform/packaging
//! detection with the first-launch playback defaults derived from it.

pub mod audio_device;
#[cfg(feature = "lirc")]
pub mod ir_service;
pub mod platform;
pub mod usb;
