#![feature(proc_macro_hygiene, decl_macro)]

use std::env;
use std::sync::{RwLock, Weak};
use std::time::Duration;

use lazy_static::lazy_static;
use vcontrol::{self, Optolink, VControl};
use webthing::{
  Action, Thing, ThingsType, WebThingServer,
  server::ActionGenerator,
};

lazy_static! {
  static ref OPTOLINK_DEVICE: String = env::var("OPTOLINK_DEVICE").unwrap_or("/dev/optolink".into());
}

fn vcontrol_connect() -> VControl {
  let mut device = Optolink::open(&*OPTOLINK_DEVICE).expect("Failed to open Optolink device");
  device.set_timeout(Some(Duration::from_secs(10))).unwrap();
  VControl::connect(device).expect("Failed to connect to device")
}

struct Generator;

impl ActionGenerator for Generator {
  fn generate(
    &self,
    _thing: Weak<RwLock<Box<dyn Thing + 'static>>>,
    _action: String,
    _value: Option<&serde_json::Value>
  ) -> Option<Box<dyn Action + 'static>> {
    None
  }
}

#[actix_rt::main]
async fn main() {
  env_logger::init();

  let vcontrol = vcontrol_connect();

  let port = env::var("PORT").map(|s| s.parse::<u16>().expect("PORT is invalid")).unwrap_or(8888);

  let mut server = WebThingServer::new(
    ThingsType::Single(vcontrol.into_thing()),
    Some(port),
    None,
    None,
    Box::new(Generator),
    None,
    Some(true),
  );

  server.start(None).await.expect("Server failed");
}
