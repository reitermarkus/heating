use std::{env, sync::Arc};

use tokio::signal;
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

  let server_thread = server.start(None);
  let update_thread = vcontrol::thing::update_thread(vcontrol, weak_thing, commands);

  let server = async move {
    let (server, _) = tokio::join!(server_thread, update_thread);
    server.expect("server failed");
  };

  let signal = async { signal::ctrl_c().await.unwrap() };

  tokio::select! {
    _ = signal => (),
    _ = server => (),
  }
}
