use std::{
  panic,
  sync::{RwLock, Weak},
};

use serde_json::json;
use tokio::sync::broadcast::{Receiver, error::RecvError};
use webthing::Thing;

use vcontrol::Value;

pub async fn update_thread(
  weak_thing: Weak<RwLock<Box<dyn Thing + 'static>>>,
  mut rx: Receiver<(&'static str, Value)>,
) {
  while let Some(thing) = weak_thing.upgrade() {
    let (command_name, new_value) = match rx.recv().await {
      Ok(res) => res,
      Err(RecvError::Closed) => break,
      Err(RecvError::Lagged(n)) => {
        log::warn!("Receiver lagged, {n} messages skipped.");
        continue;
      },
    };

    let join_result = actix_rt::task::spawn_blocking(move || {
      let mut thing = thing.write().unwrap();
      update_thing(thing.as_mut(), command_name, new_value);
    })
    .await;

    if let Err(err) = join_result {
      // This can only be a panic, not a cancellation, since
      // we don't cancel the update thread anywhere.
      panic::resume_unwind(err.into_panic())
    }
  }
}

fn update_thing(thing: &mut dyn Thing, command_name: &str, new_value: Value) {
  let prop = thing.find_property(command_name).unwrap();

  let old_value = prop.get_value();

  let new_value = if let Some(value) = Some(new_value) {
    // OutputValue
    let value = value; // value.value
    let mapping: Option<&'static phf::map::Map<i32, &'static str>> = None; // value.mapping

    let new_value = json!(value);

    if old_value != new_value {
      if let Some(mapping) = mapping {
        let mapping_found = match value {
          Value::Int(value) => {
            if let Ok(value) = i32::try_from(value) {
              mapping.contains_key(&value)
            } else {
              false
            }
          },
          _ => false,
        };

        if !mapping_found {
          log::warn!("Property '{}' does not have an enum mapping for {:?}.", command_name, value);
        }
      }
    }

    new_value
  } else {
    json!(null)
  };

  if let Err(err) = prop.set_cached_value(new_value.clone()) {
    log::error!("Failed setting cached value for property '{}': {}", command_name, err)
  }

  if old_value != new_value {
    log::debug!("Property '{}' changed from {} to {}.", command_name, old_value, new_value);
  }

  thing.property_notify(command_name.to_string(), new_value);
}
