mod vm;

use std::{env, thread};
use std::env::args;
use std::process::{Command, Stdio};
use std::time::Duration;
use eyre::{Result, WrapErr};
use fern::colors::ColoredLevelConfig;
use log::{info, warn};
use win32_coreaudio::{AudioEndpointVolumeCallback, AudioEndpointVolumeCallbackHandle, DataFlow, DeviceEnumerator, DeviceRole, NotificationClient, NotificationData};
use crate::vm::VoiceMeeterController;
use crossbeam::channel::{Sender, unbounded};
use win32_coreaudio::string::WinStr;

const APP_NAME: &str = env!("CARGO_PKG_NAME");
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

enum ChannelEvent {
    VolumeChange(CurrentVolume),
    DeviceChange
}

struct CurrentVolume {
    new_volume: f32,
    mute: bool,
}

fn main() -> Result<()> {
    // Setup logging
    let log_colors = ColoredLevelConfig::new();
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                log_colors.color(record.level()),
                message
            ))
        })
        // Add blanket level filter -
        .level(log::LevelFilter::Trace)
        // Output to stdout, files, and other Dispatch configurations
        .chain(std::io::stdout())
        // Apply globally
        .apply()
        .expect("Failed to initialize logging!");

    info!("Starting {APP_NAME} v{APP_VERSION}");

    // This is necessary because the VoiceMeeter SDK will crash the program if VoiceMeeter is not
    // running...
    // Thanks VoiceMeeter...
    if let Some("managed") = args().nth(1).as_deref() {
        info!("Launched in managed mode.");
        start()
    } else {
        info!("Launched in non-managed mode, booting managed program...");
        loop {
            let _ = Command::new(env::current_exe()?)
                .arg("managed")
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .stdin(Stdio::inherit())
                .spawn()?
                .wait();
            info!("Managed program crashed, booting it again in 5s...");
            thread::sleep(Duration::from_secs(5));
        }
    }
}

fn start() -> Result<()> {
    let mut enumerator = DeviceEnumerator::new()
        .wrap_err("failed to setup device enumerator")?;

    let mut controller = VoiceMeeterController::new();

    let (send, recv) = unbounded::<ChannelEvent>();
    // Do not drop the device change handle, otherwise the event listener will be unregistered
    let _device_change_handle = enumerator.register_endpoint_notification(DeviceChangeCallback {
        send: send.clone()
    });
    // Do not drop the volume change handle, otherwise the event listener will be unregistered
    let mut _vol_change_handle = setup_volume_cb(&mut controller, &mut enumerator, send.clone());
    loop {
        let evt = recv.recv().wrap_err("communication channel disconnected")?;
        match evt {
            ChannelEvent::VolumeChange(current_volume) => {
                if let Err(err) = update_volume(&mut controller, &current_volume) {
                    warn!("Failed to update current volume: {err:?}");
                }
            }
            ChannelEvent::DeviceChange => {
                // Re-attach volume change handle whenever the default device changes
                _vol_change_handle = setup_volume_cb(&mut controller, &mut enumerator, send.clone());
            }
        }
    }
}

fn setup_volume_cb(
    controller: &mut VoiceMeeterController,
    enumerator: &mut DeviceEnumerator,
    send: Sender<ChannelEvent>,
) -> Result<AudioEndpointVolumeCallbackHandle> {
    let default_audio_endpoint = enumerator.get_default_audio_endpoint(
        DataFlow::Render,
        DeviceRole::Multimedia,
    ).wrap_err("failed to get default audio endpoint")?;
    // Update volume once immediately
    let endpoint_volume = default_audio_endpoint
        .activate_audio_endpoint_volume()
        .wrap_err("failed to activate audio endpoint volume")?;
    if let Err(err) = endpoint_volume.get_master_volume_level_scalar()
        .wrap_err("failed to get master volume")
        .and_then(|master| Ok(CurrentVolume {
            new_volume: master,
            mute: endpoint_volume.get_mute().wrap_err("failed to get mute status")?
        }))
        .and_then(|v| update_volume(controller, &v)) {
        warn!("Failed to update current volume: {err:?}");
    }
    endpoint_volume
        .register_control_change_notify(VolumeCallback { send })
        .wrap_err("failed to register volume change notifier")
}

fn update_volume(controller: &mut VoiceMeeterController, volume: &CurrentVolume) -> Result<()> {
    let muted = if volume.new_volume == 0.0 || volume.mute {
        1f32
    } else {
        0f32
    };
    let new_gain = MIN_GAIN + (MAX_GAIN - MIN_GAIN) * volume.new_volume;
    controller.set_parameter_float("Strip[3].Mute", muted)?;
    controller.set_parameter_float("Strip[3].Gain", new_gain)?;
    controller.update_parameters_dirty().map(|_| ())
}

const MIN_GAIN: f32 = -30.0;
const MAX_GAIN: f32 = 12.0;

struct VolumeCallback {
    send: Sender<ChannelEvent>
}
impl AudioEndpointVolumeCallback for VolumeCallback {
    fn on_notify(&mut self, data: &NotificationData) -> windows::Result<()> {
        if let Err(e) =  self.send.send(ChannelEvent::VolumeChange(CurrentVolume {
            new_volume: data.master_volume,
            mute: data.muted,
        })) {
            warn!("Failed to send update volume event: {e:?}");
        };
        Ok(())
    }
}

struct DeviceChangeCallback {
    send: Sender<ChannelEvent>
}
impl NotificationClient for DeviceChangeCallback {
    fn on_default_device_changed(
        &mut self,
        data_flow: DataFlow,
        role: DeviceRole,
        _: &WinStr,
    ) -> windows::Result<()> {
        if data_flow == DataFlow::Render && role == DeviceRole::Multimedia {
            if let Err(e) = self.send.send(ChannelEvent::DeviceChange) {
                warn!("Failed to send device change event: {e:?}");
            }
        }
        Ok(())
    }
}