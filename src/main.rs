use std::{env, process, sync::Arc};

use tokio::{
  signal::unix::{SignalKind, signal},
  sync::oneshot,
};
use vcontrol::{self, Optolink, VControl};
use webthing::{BaseActionGenerator, ThingsType, WebThingServer};

#[actix_rt::main]
async fn main() {
  env_logger::init();

  let optolink_device = env::var("OPTOLINK_DEVICE").unwrap_or_else(|_| "/dev/optolink".into());
  let device = Optolink::open(optolink_device).await.expect("Failed to open Optolink device");
  let vcontrol = VControl::connect(device).await.expect("Failed to connect to device");

  let port = env::var("PORT").map(|s| s.parse::<u16>().expect("PORT is invalid")).unwrap_or(8888);

  let (vcontrol, thing, commands) = vcontrol::thing::make_thing(vcontrol);
  let weak_thing = Arc::downgrade(&thing);

  let mut server = WebThingServer::new(
    ThingsType::Single(thing),
    Some(port),
    None,
    None,
    Box::new(BaseActionGenerator),
    None,
    Some(true),
  );

  let (server_stopped_tx, server_stopped_rx) = oneshot::channel();

  let server_thread = server.start(None);
  drop(server);

  let server_handle = server_thread.handle();
  let server_thread = tokio::spawn(async {
    let res = server_thread.await;
    // Server may have been stopped via a signal, in which case the channel is already closed.
    let _ = server_stopped_tx.send(());
    res
  });
  let update_thread = tokio::spawn(async {
    vcontrol::thing::update_thread(vcontrol, weak_thing, commands).await;
    log::info!("Update thread stopped.");
  });

  let sigint = async {
    signal(SignalKind::interrupt()).unwrap().recv().await.unwrap();
    log::info!("Received SIGINT, stopping server.");
    // Main thread waits for the server to stop.
    server_handle.stop(true).await;
  };
  let sigterm = async {
    signal(SignalKind::terminate()).unwrap().recv().await.unwrap();
    log::info!("Received SIGTERM, stopping server.");
    // Main thread waits for the server to stop.
    server_handle.stop(true).await;
  };

  tokio::select! {
    _ = sigint => (),
    _ = sigterm => (),
    _ = server_stopped_rx => (),
  }

  update_thread.await.unwrap();

  match server_thread.await.unwrap() {
    Ok(()) => {
      log::info!("Server stopped.");
    },
    Err(err) => {
      log::error!("Server crashed: {err}");
      process::exit(1);
    },
  }
}
