use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::{env, io, iter};

use esphome_native_api::esphomeapi::EspHomeApi;
use esphome_native_api::parser::ProtoMessage;
use esphome_native_api::proto::version_2025_12_1::{
  BinarySensorStateResponse, DateCommandRequest, DateStateResponse, DateTimeCommandRequest, DateTimeStateResponse,
  ListEntitiesRequest, NumberCommandRequest, NumberStateResponse, SelectStateResponse, SubscribeStatesRequest,
  SwitchCommandRequest, SwitchStateResponse, TextSensorStateResponse,
};
use esphome_native_api::proto::version_2025_12_1::{ListEntitiesDoneResponse, SensorStateResponse};
use log::{debug, info, warn};
use mac_address::get_mac_address;
use tokio::net::TcpSocket;
use tokio::sync::broadcast;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::{self, Receiver, Sender};
use vcontrol::types::{Date, DateTime};
use vcontrol::{Command, VControl, Value};

use crate::esphome_server::entities::MultiEntity;

mod entities;
mod entity;

fn bool_state(value: &vcontrol::Value) -> Option<bool> {
  match value {
    vcontrol::Value::Int(0) => Some(false),
    vcontrol::Value::Int(1) => Some(true),
    _ => None,
  }
}

fn map_entity_to_key(entity: &ProtoMessage) -> u32 {
  match entity {
    ProtoMessage::ListEntitiesBinarySensorResponse(res) => res.key,
    ProtoMessage::ListEntitiesSensorResponse(res) => res.key,
    ProtoMessage::ListEntitiesNumberResponse(res) => res.key,
    ProtoMessage::ListEntitiesDateResponse(res) => res.key,
    ProtoMessage::ListEntitiesDateTimeResponse(res) => res.key,
    ProtoMessage::ListEntitiesTextSensorResponse(res) => res.key,
    ProtoMessage::ListEntitiesSwitchResponse(res) => res.key,
    ProtoMessage::ListEntitiesSelectResponse(res) => res.key,
    _ => u32::MAX,
  }
}

fn map_multi_entity_to_key(multi_entity: &MultiEntity) -> u32 {
  match multi_entity {
    MultiEntity::Single(entity) => map_entity_to_key(&entity),
    MultiEntity::Multiple(entities) => entities.first().map(map_entity_to_key).unwrap_or(u32::MAX),
  }
}

pub async fn start(
  port: u16,
  vcontrol_weak: Weak<tokio::sync::Mutex<VControl>>,
  commands: HashMap<&'static str, &'static Command>,
  vcontrol_rx: broadcast::Receiver<(&'static str, vcontrol::Value)>,
) -> (impl Future<Output = Result<(), io::Error>>, Sender<()>, Receiver<()>) {
  let (server_stopped_tx, server_stopped_rx) = oneshot::channel();
  let (server_stop_tx, server_stop_rx) = oneshot::channel();

  let addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], port));
  let socket = TcpSocket::new_v4().unwrap();
  socket.set_reuseaddr(true).unwrap();
  socket.bind(addr).unwrap();

  let listener = socket.listen(128).unwrap();
  log::debug!("Listening on: {}", addr);

  let mac_address = get_mac_address().unwrap().unwrap_or_default();

  let main_server = async move {
    log::info!("ESPHome server started.");

    let commands = Arc::new(commands);

    loop {
      info!("Waiting for connection.");
      let stream = match listener.accept().await {
        Ok((stream, _)) => stream,
        Err(err) => {
          log::error!("Failed to accept connection: {err}");
          break;
        },
      };
      log::debug!("Accepted request from {}", stream.peer_addr().unwrap());

      let vcontrol_weak = vcontrol_weak.clone();
      let commands = Arc::clone(&commands);
      let vcontrol_rx = vcontrol_rx.resubscribe();
      let entity_map = Arc::new(entities::entities(&commands));

      let encryption_key = env::var("ESPHOME_ENCRYPTION_KEY").unwrap_or_default();

      tokio::task::spawn(async move {
        let mut server = EspHomeApi::builder()
          .api_version_major(1)
          .api_version_minor(42)
          .server_info("ESPHome Rust".to_string())
          .name("vitoligno_300c".to_string())
          .friendly_name("Vitoligno 300-C".to_string())
          .mac(mac_address.to_string())
          // .bluetooth_mac_address(bluetooth_mac_address.to_string())
          // .bluetooth_proxy_feature_flags(0b1111111)
          .manufacturer("Viessmann".to_string())
          .model("Vitoligno 300-C".to_string())
          .suggested_area("Boiler Room".to_string())
          .encryption_key(encryption_key)
          .build();

        let (tx, mut rx) = server.start(stream).await.expect("Failed to start server");
        let tx_clone = tx.clone();
        debug!("Server started");

        let entity_map = Arc::clone(&entity_map);

        tokio::spawn(async move {
          loop {
            let message = match rx.recv().await {
              Ok(message) => message,
              Err(err) => {
                info!("Connection closed or error: {err}");
                return;
              },
            };

            let vcontrol_weak = vcontrol_weak.clone();

            debug!("Received message: {:?}", message);

            match message {
              ProtoMessage::SubscribeHomeassistantServicesRequest(req) => {
                log::debug!("SubscribeHomeassistantServicesRequest: {req:#?}");
              },
              ProtoMessage::SubscribeHomeAssistantStatesRequest(req) => {
                log::debug!("SubscribeHomeAssistantStatesRequest: {req:#?}");
              },
              ProtoMessage::ListEntitiesRequest(ListEntitiesRequest {}) => {
                let mut entities = entity_map.values().collect::<Vec<_>>();
                entities.sort_by_key(|e| map_multi_entity_to_key(e));

                for entity in entities {
                  match entity {
                    MultiEntity::Single(entity) => {
                      tx_clone.send(entity.clone()).await.unwrap();
                    },
                    MultiEntity::Multiple(entities) => {
                      for entity in entities {
                        tx_clone.send(entity.clone()).await.unwrap();
                      }
                    },
                  }
                }

                debug!("ListEntitiesDoneResponse");
                tx_clone.send(ProtoMessage::ListEntitiesDoneResponse(ListEntitiesDoneResponse {})).await.unwrap();
              },
              ProtoMessage::NumberCommandRequest(NumberCommandRequest { key, state, .. }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown number command: {key}");
                  continue;
                };

                let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
                let mut vcontrol = vcontrol.lock().await;

                log::info!("Setting value for {command_name}: {state}");
                if let Err(err) = vcontrol.set(command_name, Value::Double(state as f64)).await {
                  log::error!("Failed to set value ({state}) for {command_name}: {err}")
                }
              },
              ProtoMessage::SwitchCommandRequest(SwitchCommandRequest { key, state, .. }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown switch command: {key}");
                  continue;
                };

                let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
                let mut vcontrol = vcontrol.lock().await;

                log::info!("Setting value for {command_name}: {state}");
                if let Err(err) = vcontrol.set(command_name, Value::Int(state as i64)).await {
                  log::error!("Failed to set value ({state}) for {command_name}: {err}")
                }
              },
              ProtoMessage::DateCommandRequest(DateCommandRequest { key, year, month, day, .. }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown date command: {key}");
                  continue;
                };

                let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
                let mut vcontrol = vcontrol.lock().await;

                let date = Date::new(year as u16, month as u8, day as u8).unwrap();
                if let Err(err) = vcontrol.set(command_name, Value::Date(date)).await {
                  log::error!("Failed to set value ({date}) for {command_name}: {err}")
                }
              },
              ProtoMessage::DateTimeCommandRequest(DateTimeCommandRequest { key, epoch_seconds, .. }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown date-time command: {key}");
                  continue;
                };

                let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
                let mut vcontrol = vcontrol.lock().await;

                let date_time = DateTime::from_unix_timestamp(epoch_seconds);
                if let Err(err) = vcontrol.set(command_name, Value::DateTime(date_time)).await {
                  log::error!("Failed to set value ({date_time}) for {command_name}: {err}")
                }
              },
              ProtoMessage::SubscribeStatesRequest(SubscribeStatesRequest {}) => {
                let tx = tx.clone();
                let mut vcontrol_rx = vcontrol_rx.resubscribe();
                let entity_map = Arc::clone(&entity_map);
                let commands = Arc::clone(&commands);

                tokio::spawn(async move {
                  debug!("State retrieval loop");

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
                        match send_entity_state(
                          tx.clone(),
                          vcontrol_weak.clone(),
                          command_name,
                          &commands,
                          entity,
                          value,
                        )
                        .await
                        {
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

                        for (entity, value) in
                          entities.iter().zip(values.into_iter().flat_map(|v| iter::repeat_n(v, 2)))
                        {
                          match send_entity_state(
                            tx.clone(),
                            vcontrol_weak.clone(),
                            command_name,
                            &commands,
                            entity,
                            value,
                          )
                          .await
                          {
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
                });
              },
              request => {
                debug!("Unexpected request: {request:?}")
              },
            }
          }
        });
      });
    }

    let _ = server_stopped_tx.send(());
    Ok(())
  };

  let main_server = async {
    tokio::select! {
      res = main_server => res,
      _ = server_stop_rx => {
        log::info!("ESPHome server stopped.");

        Ok(())
      },
    }
  };

  let main_server = tokio::spawn(main_server);
  let main_server = async { main_server.await.unwrap() };

  (main_server, server_stop_tx, server_stopped_rx)
}

async fn send_entity_state(
  tx: tokio::sync::mpsc::Sender<ProtoMessage>,
  vcontrol: Weak<tokio::sync::Mutex<VControl>>,
  command_name: &'static str,
  commands: &HashMap<&'static str, &'static Command>,
  entity: &ProtoMessage,
  value: vcontrol::Value,
) -> Result<(), SendError<ProtoMessage>> {
  let device_id = 0;

  match entity {
    ProtoMessage::ListEntitiesBinarySensorResponse(res) => {
      let (missing_state, state) = match bool_state(&value) {
        Some(state) => (false, state),
        None => {
          warn!("Unsupported value for binary sensor {command_name}: {value:?}");
          (true, false)
        },
      };

      let message = BinarySensorStateResponse { device_id, key: res.key, state, missing_state };
      tx.send(ProtoMessage::BinarySensorStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesSwitchResponse(res) => {
      let state = match bool_state(&value) {
        Some(state) => state,
        None => {
          log::error!("Unsupported value for switch {command_name}: {value:?}");
          return Ok(());
        },
      };

      let message = SwitchStateResponse { device_id, key: res.key, state };
      tx.send(ProtoMessage::SwitchStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesSensorResponse(res) => {
      let mut missing_state = false;
      let state = match value {
        vcontrol::Value::Empty => {
          missing_state = true;
          Some(0.0)
        },
        vcontrol::Value::Int(n) => Some(n as f32),
        vcontrol::Value::Double(n) => Some(n as f32),
        _ => None,
      };

      let Some(state) = state else {
        log::error!("Unsupported value for sensor: {value:?}");
        return Ok(());
      };

      let message = SensorStateResponse { device_id, key: res.key, state, missing_state };
      if command_name == "Ecotronic_Kesselstarts" {
        log::debug!("Ecotronic_Kesselstarts response: {message:?}");
      }
      tx.send(ProtoMessage::SensorStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesNumberResponse(res) => {
      let mut missing_state = false;
      let state = match value {
        vcontrol::Value::Empty => {
          missing_state = true;
          Some(0.0)
        },
        vcontrol::Value::Int(n) => Some(n as f32),
        vcontrol::Value::Double(n) => Some(n as f32),
        _ => None,
      };

      let Some(state) = state else {
        warn!("Unsupported value for number: {value:?}");
        return Ok(());
      };

      let message = NumberStateResponse { device_id, key: res.key, state, missing_state };
      tx.send(ProtoMessage::NumberStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesDateResponse(res) => {
      let mut missing_state = false;
      let state = match value {
        vcontrol::Value::Empty => {
          missing_state = true;
          Some((1970, 1, 1))
        },
        vcontrol::Value::Date(ref date) => {
          let (year, month, day) = (u32::from(date.year()), u32::from(date.month()), u32::from(date.day()));

          if year == 1970 && month == 1 && day == 1 {
            missing_state = true;
          }

          Some((year, month, day))
        },
        _ => None,
      };

      let Some((year, month, day)) = state else {
        warn!("Unsupported value for date: {value:?}");
        return Ok(());
      };

      let message = DateStateResponse { device_id, key: res.key, missing_state, year, month, day };
      tx.send(ProtoMessage::DateStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesDateTimeResponse(res) => {
      let (missing_state, epoch_seconds) = match value {
        vcontrol::Value::Empty => (true, 0),
        vcontrol::Value::Error(error) => {
          if let Some(time) = error.time() {
            (false, time.unix_timestamp())
          } else {
            (true, 0)
          }
        },
        vcontrol::Value::DateTime(datetime) => (false, datetime.unix_timestamp()),
        value => {
          log::error!("Unsupported value for date-time: {value:?}");
          return Ok(());
        },
      };

      let message = DateTimeStateResponse { device_id, key: res.key, missing_state, epoch_seconds };
      tx.send(ProtoMessage::DateTimeStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesTextSensorResponse(res) => {
      let mut missing_state = false;
      let state = match value {
        vcontrol::Value::Empty => {
          missing_state = true;
          "".to_owned()
        },
        vcontrol::Value::String(s) => s,
        vcontrol::Value::Int(n) => {
          if let Some(mapping) = commands[command_name].mapping().as_ref() {
            mapping.get(&(n as i32)).map(|&s| s.to_owned()).unwrap_or(n.to_string())
          } else {
            n.to_string()
          }
        },
        vcontrol::Value::ByteArray(bytes) => {
          bytes.into_iter().map(|byte| format!("{byte:02X}")).collect::<Vec<String>>().join(", ")
        },
        vcontrol::Value::Error(error) => {
          let Some(vcontrol) = vcontrol.upgrade() else { return Ok(()) };
          let vcontrol = vcontrol.lock().await;

          error.to_str(vcontrol.device()).unwrap_or_default().to_owned()
        },
        state => format!("{state:?}"),
      };

      let message = TextSensorStateResponse { device_id, key: res.key, missing_state, state };
      tx.send(ProtoMessage::TextSensorStateResponse(message)).await?;
    },
    ProtoMessage::ListEntitiesSelectResponse(res) => {
      let mut missing_state = false;
      let state = match value {
        vcontrol::Value::Empty => {
          missing_state = true;
          Some("".into())
        },
        vcontrol::Value::Int(n) => {
          let mapping = commands[command_name].mapping().unwrap();
          Some(mapping[&(n as i32)].to_owned())
        },
        _ => None,
      };

      let Some(state) = state else {
        warn!("Unsupported value for sensor: {value:?}");
        return Ok(());
      };

      let message = SelectStateResponse { device_id, key: res.key, missing_state, state };
      tx.send(ProtoMessage::SelectStateResponse(message)).await?;
    },
    _ => (),
  }

  Ok(())
}
