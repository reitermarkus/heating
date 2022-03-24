#![feature(proc_macro_hygiene, decl_macro)]

use std::env;
use std::sync::Arc;

use lazy_static::lazy_static;
use tokio::signal;
use vcontrol::{self, Optolink, VControl};
use webthing::{
  ThingsType, WebThingServer,
  BaseActionGenerator,
};

lazy_static! {
  static ref OPTOLINK_DEVICE: String = env::var("OPTOLINK_DEVICE").unwrap_or_else(|_| "/dev/optolink".into());
}

async fn vcontrol_connect() -> VControl {
  let device = Optolink::open(&*OPTOLINK_DEVICE).await.expect("Failed to open Optolink device");
  VControl::connect(device).await.expect("Failed to connect to device")
}

#[actix_rt::main]
async fn main() {
  env_logger::init();

  let vcontrol = vcontrol_connect().await;

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

  let signal = async {
    signal::ctrl_c().await.unwrap()
  };
  let server = async move {
    let (server, _) = tokio::join!(server_thread, update_thread);
    server.expect("server failed");
  };

  tokio::select! {
    _ = signal => (),
    _ = server => (),
  }
}
