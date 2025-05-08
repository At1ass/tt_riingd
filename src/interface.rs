use event_listener::Event;
use log::error;
use zbus::{interface, object_server::SignalEmitter};

use crate::controller::Controllers;

pub struct DBusInterface {
    pub controllers: Controllers,

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

    async fn set_speed(&self, speed: u8) {
        if let Err(e) = self.controllers.set_pwm(speed).await {
            error!("{e}");
        }
    }
}
