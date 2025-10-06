use crate::uart::service::ArcUartService;
use api_models::common::Volume;

use crate::audio_device::VolumeControlDevice;

pub struct RSPlayerFirmwareVolumeControlDevice {
    uart_service: ArcUartService,
}

impl RSPlayerFirmwareVolumeControlDevice {
    pub fn new(uart_service: ArcUartService) -> Self {
        Self { uart_service }
    }
}

impl VolumeControlDevice for RSPlayerFirmwareVolumeControlDevice {
    fn vol_up(&mut self) -> Volume {
        self.uart_service.send_command("VolUp");
        Volume::default()
    }
    fn vol_down(&mut self) -> Volume {
        self.uart_service.send_command("VolDown");
        Volume::default()
    }
    fn get_vol(&mut self) -> Volume {
        self.uart_service.send_command("QueryCurVolume");
        Volume::default()
    }
    fn set_vol(&mut self, level: u8) -> Volume {
        self.uart_service.send_command(&format!("SetVol({})", level));
        Volume::default()
    }
}