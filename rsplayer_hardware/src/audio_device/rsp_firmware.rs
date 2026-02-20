use crate::usb::ArcUsbService;
use api_models::common::Volume;

use crate::audio_device::VolumeControlDevice;

pub struct RSPlayerFirmwareVolumeControlDevice {
    usb_service: ArcUsbService,
}

impl RSPlayerFirmwareVolumeControlDevice {
    pub const fn new(usb_service: ArcUsbService) -> Self {
        Self { usb_service }
    }
}

impl VolumeControlDevice for RSPlayerFirmwareVolumeControlDevice {
    fn vol_up(&mut self) -> Volume {
        let _ = self.usb_service.send_command("VolUp");
        Volume::default()
    }
    fn vol_down(&mut self) -> Volume {
        let _ = self.usb_service.send_command("VolDown");
        Volume::default()
    }
    fn get_vol(&mut self) -> Volume {
        let _ = self.usb_service.send_command("QueryCurVolume");
        Volume::default()
    }
    fn set_vol(&mut self, level: u8) -> Volume {
        let _ = self.usb_service.send_command(&format!("SetVol({level})"));
        Volume {
            current: level,
            ..Default::default()
        }
    }
}
