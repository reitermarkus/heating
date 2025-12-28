use std::collections::HashMap;
use std::iter;
use std::sync::{Arc, Weak};

use esphome_native_api::parser::ProtoMessage;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::{Mutex, broadcast, mpsc};
use vcontrol::{Command, VControl, Value};

use super::send_entity_state;
use crate::esphome_server::entities::MultiEntity;

pub async fn send_state(
  tx: mpsc::Sender<ProtoMessage>,
  mut vcontrol_rx: broadcast::Receiver<(&'static str, Value)>,
  vcontrol_weak: Weak<Mutex<VControl>>,
  entity_map: Arc<HashMap<&str, MultiEntity>>,
  commands: Arc<HashMap<&'static str, &'static Command>>,
) {
  'outer: loop {
    let (command_name, value) = match vcontrol_rx.recv().await {
      Ok(res) => res,
      Err(broadcast::error::RecvError::Closed) => break,
      Err(broadcast::error::RecvError::Lagged(n)) => {
        log::warn!("Receiver lagged, {n} messages skipped.");
        continue;
      },
    };

    let Some(entity) = entity_map.get(command_name) else {
      // log::debug!("No entity for command {command_name}: {value:?}");
      continue;
    };

    match entity {
      MultiEntity::Single(entity) => {
        match send_entity_state(tx.clone(), vcontrol_weak.clone(), command_name, &commands, entity, value).await {
          Ok(()) => continue,
          Err(SendError(message)) => {
            log::error!("Failed to send message for command '{command_name}': {message:?}");
            break 'outer;
          },
        }
      },
      MultiEntity::Multiple(entities) => {
        let vcontrol::Value::Array(values) = value else {
          log::warn!("Invalid value for command {command_name}: {value:?}");
          continue;
        };

        for (entity, value) in entities.iter().zip(values.into_iter().flat_map(|v| iter::repeat_n(v, 2))) {
          match send_entity_state(tx.clone(), vcontrol_weak.clone(), command_name, &commands, entity, value).await {
            Ok(()) => continue,
            Err(SendError(message)) => {
              log::error!("Failed to send message for command '{command_name}': {message:?}");
              break 'outer;
            },
          }
        }
      },
    }
  }
}
