use std::{collections::HashMap, io, sync::Arc};

use actix_server::ServerHandle;
use tokio::sync::broadcast;
use tokio::sync::oneshot::{self, Receiver};
use vcontrol::{Command, VControl, Value};
use webthing::{BaseActionGenerator, ThingsType, WebThingServer};

mod thing;

pub async fn start(
  port: u16,
  vcontrol: Arc<tokio::sync::Mutex<VControl>>,
  commands: HashMap<&'static str, &'static Command>,
  rx: broadcast::Receiver<(&'static str, Value)>,
) -> (impl Future<Output = Result<(), io::Error>>, ServerHandle, Receiver<()>) {
  let thing = thing::make_thing(vcontrol, commands).await;
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
    log::info!("Server thread started.");
    let res = server_thread.await;
    // Server may have been stopped via a signal, in which case the channel is already closed.
    let _ = server_stopped_tx.send(());
    log::info!("Server thread stopped.");
    res
  });
  let update_thread = tokio::spawn(async move {
    log::info!("Update thread started.");
    let res = thing::update_thread(weak_thing, rx).await;
    log::info!("Update thread stopped.");
    Ok(res)
  });

  let server_thread = async {
    tokio::select! {
      res = server_thread => res.unwrap(),
      res = update_thread => res.unwrap(),
    }
  };

  (server_thread, server_handle, server_stopped_rx)
}
