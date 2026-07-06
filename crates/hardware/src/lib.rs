//! Hardware integrations: volume-control devices, the USB front-panel
//! link, and the optional LIRC infrared remote service.

pub mod audio_device;
#[cfg(feature = "lirc")]
pub mod ir_service;
pub mod usb;
