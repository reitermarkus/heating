use std::{collections::HashMap, sync::Arc};

use esphome_native_api::{
  parser::ProtoMessage,
  proto::version_2025_12_1::{
    DateCommandRequest, DateStateResponse, DateTimeCommandRequest, DateTimeStateResponse, NumberCommandRequest,
    NumberStateResponse, SwitchCommandRequest, SwitchStateResponse,
  },
};
use tokio::sync::{
  Mutex,
  mpsc::{Sender, error::SendError},
};
use vcontrol::{
  VControl, Value,
  types::{Date, DateTime},
};

use crate::esphome_server::entities::MultiEntity;

pub async fn handle_date_command(
  request: DateCommandRequest,
  entity_map: &HashMap<&str, MultiEntity>,
  vcontrol: Arc<Mutex<VControl>>,
  tx: &Sender<ProtoMessage>,
) -> Result<(), SendError<ProtoMessage>> {
  let key = request.key;
  let Some((command_name, _)) = entity_map.iter().find(|(_, e)| e.key() == key) else {
    log::warn!("Unknown date command: {key}");
    return Ok(());
  };

  let mut vcontrol = vcontrol.lock().await;

  let date = Date::new(request.year as u16, request.month as u8, request.day as u8).unwrap();
  if let Err(err) = vcontrol.set(command_name, Value::Date(date)).await {
    log::error!("Failed to set value ({date}) for {command_name}: {err}");
    return Ok(());
  }

  tx.send(ProtoMessage::DateStateResponse(DateStateResponse {
    key,
    missing_state: false,
    year: request.year,
    month: request.month,
    day: request.day,
    device_id: request.device_id,
  }))
  .await
}

pub async fn handle_date_time_command(
  request: DateTimeCommandRequest,
  entity_map: &HashMap<&str, MultiEntity>,
  vcontrol: Arc<Mutex<VControl>>,
  tx: &Sender<ProtoMessage>,
) -> Result<(), SendError<ProtoMessage>> {
  let key = request.key;
  let Some((command_name, _)) = entity_map.iter().find(|(_, e)| e.key() == key) else {
    log::warn!("Unknown date-time command: {key}");
    return Ok(());
  };

  let mut vcontrol = vcontrol.lock().await;

  let date_time = DateTime::from_unix_timestamp(request.epoch_seconds);
  if let Err(err) = vcontrol.set(command_name, Value::DateTime(date_time)).await {
    log::error!("Failed to set value ({date_time}) for {command_name}: {err}");
    return Ok(());
  }

  tx.send(ProtoMessage::DateTimeStateResponse(DateTimeStateResponse {
    key,
    missing_state: false,
    epoch_seconds: request.epoch_seconds,
    device_id: request.device_id,
  }))
  .await
}

pub async fn handle_number_command(
  request: NumberCommandRequest,
  entity_map: &HashMap<&str, MultiEntity>,
  vcontrol: Arc<Mutex<VControl>>,
  tx: &Sender<ProtoMessage>,
) -> Result<(), SendError<ProtoMessage>> {
  let key = request.key;
  let Some((command_name, _)) = entity_map.iter().find(|(_, e)| e.key() == key) else {
    log::warn!("Unknown number command: {key}");
    return Ok(());
  };

  let mut vcontrol = vcontrol.lock().await;

  let state = request.state;
  log::info!("Setting value for {command_name}: {state}");
  if let Err(err) = vcontrol.set(command_name, Value::Double(state as f64)).await {
    log::error!("Failed to set value ({state}) for {command_name}: {err}");
    return Ok(());
  }

  tx.send(ProtoMessage::NumberStateResponse(NumberStateResponse {
    key,
    state,
    missing_state: false,
    device_id: request.device_id,
  }))
  .await
}

pub async fn handle_switch_command(
  request: SwitchCommandRequest,
  entity_map: &HashMap<&str, MultiEntity>,
  vcontrol: Arc<Mutex<VControl>>,
  tx: &Sender<ProtoMessage>,
) -> Result<(), SendError<ProtoMessage>> {
  let key = request.key;
  let Some((command_name, _)) = entity_map.iter().find(|(_, e)| e.key() == key) else {
    log::warn!("Unknown switch command: {key}");
    return Ok(());
  };

  let mut vcontrol = vcontrol.lock().await;

  let state = request.state;
  log::info!("Setting value for {command_name}: {state}");
  if let Err(err) = vcontrol.set(command_name, Value::Int(state as i64)).await {
    log::error!("Failed to set value ({state}) for {command_name}: {err}")
  }

  tx.send(ProtoMessage::SwitchStateResponse(SwitchStateResponse { key, state, device_id: request.device_id })).await
}
