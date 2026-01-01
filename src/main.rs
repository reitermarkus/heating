use std::{env, process, sync::Arc};

use tokio::{
  signal::unix::{SignalKind, signal},
  sync::oneshot,
};
use vcontrol::{self, Optolink, VControl};

use crate::command_poller::poll_thread;

mod command_poller;
mod esphome_server;

#[tokio::main]
async fn main() {
  env_logger::init();

  let optolink_device = env::var("OPTOLINK_DEVICE").unwrap_or_else(|_| "/dev/optolink".into());

  let device = if optolink_device.contains(':') {
    Optolink::connect(optolink_device).await.expect("Failed to connect to Optolink device")
  } else {
    Optolink::open(optolink_device).await.expect("Failed to open Optolink device")
  };
  let vcontrol = VControl::connect(device).await.expect("Failed to connect to device");

  let sigint = async { signal(SignalKind::interrupt()).unwrap().recv().await };
  let sigterm = async { signal(SignalKind::terminate()).unwrap().recv().await };

  let (vcontrol, rx, poll_thread, commands) = poll_thread(vcontrol).await;
  let (esphome_server, esphome_server_stop, esphome_server_stopped) =
    esphome_server::start(6053, Arc::downgrade(&vcontrol), commands.clone(), rx).await;

  let (poll_thread_stopped_tx, poll_thread_stopped) = oneshot::channel();
  let poll_thread = tokio::spawn(async {
    let res = poll_thread.await;
    // Poll thread may have been stopped via a signal, in which case the channel is already closed.
    let _ = poll_thread_stopped_tx.send(());
    log::info!("Poll thread stopped.");
    res
  });

  tokio::select! {
    _ = sigint => {
      log::info!("Received SIGINT, stopping server.");
    },
    _ = sigterm => {
      log::info!("Received SIGTERM, stopping server.");
    },
    _ = esphome_server_stopped => (),
    _ = poll_thread_stopped => (),
  }
  drop(vcontrol);

  log::info!("Stopping ESPHome server.");
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

  match poll_thread.await {
    Ok(Ok(())) => (),
    Ok(Err(err)) => {
      log::error!("Poll thread crashed: {err}");
      process::exit(1);
    },
    Err(_) => {
      log::error!("Failed to join poll thread.");
      process::exit(1);
    },
  }
}
