use std::{env, process};

use tokio::signal::unix::{SignalKind, signal};
use vcontrol::{self, Optolink, VControl};

mod esphome_server;
mod webthing_server;

#[actix_rt::main]
async fn main() {
  env_logger::init();

  let optolink_device = env::var("OPTOLINK_DEVICE").unwrap_or_else(|_| "/dev/optolink".into());
  let device = Optolink::open(optolink_device).await.expect("Failed to open Optolink device");
  let vcontrol = VControl::connect(device).await.expect("Failed to connect to device");

  let port = env::var("PORT").map(|s| s.parse::<u16>().expect("PORT is invalid")).unwrap_or(8888);

  let sigint = async { signal(SignalKind::interrupt()).unwrap().recv().await };
  let sigterm = async { signal(SignalKind::terminate()).unwrap().recv().await };

  let (webthing_server, webthing_server_handle, webthing_server_stopped, thing, commands) =
    webthing_server::start(port, vcontrol).await;
  let (esphome_server, esphome_server_stop, esphome_server_stopped) =
    esphome_server::start(6053, thing, commands).await;

  tokio::select! {
    _ = sigint => {
      log::info!("Received SIGINT, stopping server.");
    },
    _ = sigterm => {
      log::info!("Received SIGTERM, stopping server.");
    },
    _ = webthing_server_stopped => (),
    _ = esphome_server_stopped => (),
  }

  webthing_server_handle.stop(true).await;
  esphome_server_stop.send(()).unwrap();

  match esphome_server.await {
    Ok(()) => {
      log::info!("ESPHome server stopped.");
    },
    Err(err) => {
      log::error!("ESPHome server crashed: {err}");
      process::exit(1);
    },
  }

  match webthing_server.await {
    Ok(()) => {
      log::info!("WebThing server stopped.");
    },
    Err(err) => {
      log::error!("WebThing server crashed: {err}");
      process::exit(1);
    },
  }
}
