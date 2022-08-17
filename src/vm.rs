use voicemeeter::VoicemeeterRemote;
use eyre::{Result, WrapErr};
use log::warn;
use voicemeeter::interface::general_information::VoicemeeterVersion;
use voicemeeter::types::ParameterNameRef;

pub struct VoiceMeeterController {
    inner: Option<VoiceMeeterControllerInner>
}

struct VoiceMeeterControllerInner {
    remote: VoicemeeterRemote
}

impl VoiceMeeterController {
    pub fn new() -> Self {
        let vm = match VoicemeeterRemote::new() {
            Ok(v) => Some(VoiceMeeterControllerInner { remote: v }),
            Err(err) => {
                warn!("Initial VoiceMeeter connection failed: {err:?}, leaving unconnected for now...");
                None
            }
        };
        Self {
            inner: vm,
        }
    }

    fn retry_once<T>(&mut self, mut block: impl FnMut(&mut VoiceMeeterControllerInner) -> Result<T>) -> Result<T> {
        // If connected, call block
        let result = self.inner
            .as_mut()
            .and_then(|x| block(x).ok());
        match result {
            Some(v) => Ok(v),
            None => {
                // Not connected or block call failed, reconnect
                warn!("VoiceMeeter call failed or VoiceMeeter is not connected, re-connecting now...");
                let mut vm = VoiceMeeterControllerInner {
                    remote: VoicemeeterRemote::new().wrap_err("failed to connect to VoiceMeeter")?
                };
                let call_result = block(&mut vm);
                self.inner = Some(vm);
                call_result
            }
        }
    }

    pub fn get_version(&mut self) -> Result<VoicemeeterVersion> {
        self.retry_once(|controller| {
            controller.remote.get_voicemeeter_version()
                .wrap_err("failed to get VoiceMeeter version")
        })
    }

    pub fn set_parameter_string(&mut self, param: &str, new_value: &str) -> Result<()> {
        self.retry_once(|controller| {
            controller.remote.set_parameter_string(ParameterNameRef::from_str(param), new_value)
                .wrap_err_with(|| format!("failed to set string parameter {param} to {new_value}"))
        })
    }

    pub fn set_parameter_float(&mut self, param: &str, new_value: f32) -> Result<()> {
        self.retry_once(|controller| {
            controller.remote.set_parameter_float(ParameterNameRef::from_str(param), new_value)
                .wrap_err_with(|| format!("failed to set float parameter {param} to {new_value}"))
        })
    }

    pub fn get_parameter_string(&mut self, param: &str) -> Result<String> {
        self.retry_once(|controller| {
            controller.remote.get_parameter_string(ParameterNameRef::from_str(param))
                .wrap_err_with(|| format!("failed to get string parameter {param}"))
        })
    }

    pub fn get_parameter_float(&mut self, param: &str) -> Result<f32> {
        self.retry_once(|controller| {
            controller.remote.get_parameter_float(ParameterNameRef::from_str(param))
                .wrap_err_with(|| format!("failed to get float parameter {param}"))
        })
    }

    pub fn update_parameters_dirty(&mut self) -> Result<bool> {
        self.retry_once(|controller| {
            controller.remote.is_parameters_dirty()
                .wrap_err("failed to check update parameters")
        })
    }
}