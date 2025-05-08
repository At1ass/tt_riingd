use std::sync::Arc;

use event_listener::Event;
use log::error;
use zbus::{interface, object_server::SignalEmitter};

use crate::controller::Controllers;

pub struct DBusInterface {
    pub controllers: Arc<Controllers>,

    // Events
    pub stop: Event,
}

#[interface(name = "io.github.tt_riingd1")]
impl DBusInterface {
    #[zbus(signal)]
    async fn stopped(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    async fn stop(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        emitter.stopped().await?;
        self.stop.notify(1);

        Ok(())
    }

    async fn set_speed_for_all(&self, speed: u8) {
        if let Err(e) = self.controllers.set_speed_for_all(speed).await {
            error!("{e}");
        }
    }

    #[zbus(property)]
    async fn version(&self) -> &'static str {
        "1.0"
    }

    #[zbus(property)]
    async fn speed_for_timer(&self) -> String {
        if let Ok(speed) = self.controllers.get_speed_for_timer().await {
            format!("{:?}", speed)
        } else {
            "Unknown".to_string()
        }
    }

    #[zbus(property)]
    async fn set_speed_for_timer(&mut self, speed: u8) {
        if let Err(e) = self.controllers.set_speed_for_timer(speed).await {
            error!("{e}");
        }
    }

}
