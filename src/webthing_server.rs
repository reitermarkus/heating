use std::{
  collections::HashMap,
  io,
  sync::{Arc, Weak},
};

use actix_server::ServerHandle;
use tokio::sync::{
  RwLock,
  oneshot::{self, Receiver},
};
use vcontrol::{Command, VControl};
use webthing::{BaseActionGenerator, Thing, ThingsType, WebThingServer};

pub async fn start(
  port: u16,
  vcontrol: VControl,
) -> (
  impl Future<Output = Result<(), io::Error>>,
  ServerHandle,
  Receiver<()>,
  Weak<std::sync::RwLock<Box<dyn Thing + 'static>>>,
  HashMap<&'static str, &'static Command>,
) {
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
  drop(server); // Ensure the update thread is stopped if the server stops.

  let server_handle = server_thread.handle();
  let server_thread = tokio::spawn(async {
    let res = server_thread.await;
    // Server may have been stopped via a signal, in which case the channel is already closed.
    let _ = server_stopped_tx.send(());
    log::debug!("Server thread stopped.");
    res
  });
  let update_thread = vcontrol::thing::update_thread(vcontrol, Weak::clone(&weak_thing), commands.clone());
  let update_thread = tokio::spawn(async move {
    let res = update_thread.await;
    log::debug!("Update thread stopped.");
    Ok(res)
  });

  let server_thread = async {
    tokio::select! {
      res = server_thread => res.unwrap(),
      res = update_thread =>res.unwrap(),
    }
  };

  (server_thread, server_handle, server_stopped_rx, weak_thing, commands)
}
