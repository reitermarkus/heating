use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::{future, net::SocketAddr};
use std::{io, iter};

use esphome_native_api::esphomeapi::EspHomeApi;
use esphome_native_api::parser::ProtoMessage;
use esphome_native_api::proto::version_2025_6_3::{
  BinarySensorStateResponse, DateCommandRequest, DateStateResponse, DateTimeStateResponse, NumberCommandRequest,
  NumberStateResponse, SelectStateResponse, SwitchCommandRequest, SwitchStateResponse, TextSensorStateResponse,
};
use esphome_native_api::proto::version_2025_6_3::{ListEntitiesDoneResponse, SensorStateResponse};
use log::{debug, info, warn};
use mac_address::{MacAddress, get_mac_address};
use tokio::net::TcpSocket;
use tokio::sync::broadcast;
use tokio::sync::oneshot::{self, Receiver, Sender};
use vcontrol::types::Date;
use vcontrol::{Command, VControl, Value};

use crate::esphome_server::entities::MultiEntity;
use crate::esphome_server::entity::Entity;

mod entities;
mod entity;

fn bool_state(value: &vcontrol::Value) -> (Option<bool>, bool) {
  let mut missing_state = false;
  let state = match value {
    vcontrol::Value::Empty => {
      missing_state = true;
      Some(false)
    },
    vcontrol::Value::Int(0) => Some(false),
    vcontrol::Value::Int(1) => Some(false),
    _ => None,
  };

  (state, missing_state)
}

fn map_entity_to_key(entity: &ProtoMessage) -> u32 {
  match entity {
    ProtoMessage::ListEntitiesBinarySensorResponse(res) => res.key,
    ProtoMessage::ListEntitiesSensorResponse(res) => res.key,
    ProtoMessage::ListEntitiesNumberResponse(res) => res.key,
    ProtoMessage::ListEntitiesDateResponse(res) => res.key,
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
  vcontrol: Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
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
  debug!("Listening on: {}", addr);

  let mac_address = tokio::task::spawn_blocking(|| get_mac_address()).await.unwrap().unwrap().unwrap_or_default();

  let main_server = async move {
    info!("Starting ESPHome server loop.");

    let commands = Arc::new(commands);

    loop {
      info!("Waiting for connection.");
      let (stream, _) = listener.accept().await.expect("Failed to accept connection");
      debug!("Accepted request from {}", stream.peer_addr().unwrap());

      let commands = Arc::clone(&commands);
      let vcontrol = vcontrol.clone();
      let vcontrol_rx = vcontrol_rx.resubscribe();
      let entity_map = Arc::new(entities::entities(&commands));

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
          .encryption_key("px7tsbK3C7bpXHr2OevEV2ZMg/FrNBw2+O2pNPbedtA=".to_string())
          .build();

        let (tx, mut rx) = server.start(stream).await.expect("Failed to start server");
        let tx_clone = tx.clone();
        debug!("Server started");

        let entity_map = Arc::clone(&entity_map);
        let vcontrol = Arc::clone(&vcontrol);

        tokio::spawn(async move {
          loop {
            let vcontrol = Arc::clone(&vcontrol);

            let message = match rx.recv().await {
              Ok(message) => message,
              Err(err) => {
                info!("Connection closed or error: {err}");
                return;
              },
            };

            debug!("Received message: {:?}", message);

            match message {
              ProtoMessage::SubscribeHomeassistantServicesRequest(req) => {
                log::info!("SubscribeHomeassistantServicesRequest: {req:#?}");
              },
              ProtoMessage::SubscribeHomeAssistantStatesRequest(req) => {
                log::info!("SubscribeHomeAssistantStatesRequest: {req:#?}");
              },
              ProtoMessage::ListEntitiesRequest(list_entities_request) => {
                debug!("ListEntitiesRequest: {:?}", list_entities_request);

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
              ProtoMessage::NumberCommandRequest(NumberCommandRequest { key, state }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown number command: {key}");
                  continue;
                };

                let vcontrol = vcontrol.write().await;
                let mut vcontrol = vcontrol.lock().await;

                log::info!("Setting value for {command_name}: {state}");
                if let Err(err) = vcontrol.set(command_name, Value::Double(state as f64)).await {
                  log::error!("Failed to set value ({state}) for {command_name}: {err}")
                }
              },
              ProtoMessage::SwitchCommandRequest(SwitchCommandRequest { key, state }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown switch command: {key}");
                  continue;
                };

                let vcontrol = vcontrol.write().await;
                let mut vcontrol = vcontrol.lock().await;

                log::info!("Setting value for {command_name}: {state}");
                if let Err(err) = vcontrol.set(command_name, Value::Int(state as i64)).await {
                  log::error!("Failed to set value ({state}) for {command_name}: {err}")
                }
              },
              ProtoMessage::DateCommandRequest(DateCommandRequest { key, year, month, day }) => {
                let Some((command_name, _)) = entity_map.iter().find(|(_, e)| map_multi_entity_to_key(e) == key) else {
                  warn!("Unknown date command: {key}");
                  continue;
                };

                let vcontrol = vcontrol.write().await;
                let mut vcontrol = vcontrol.lock().await;

                let date = Date::new(year as u16, month as u8, day as u8).unwrap();
                if let Err(err) = vcontrol.set(command_name, Value::Date(date)).await {
                  log::error!("Failed to set value ({date}) for {command_name}: {err}")
                }
              },
              ProtoMessage::SubscribeStatesRequest(req) => {
                debug!("SubscribeStatesRequest: {req:?}");

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
                        match send_entity_state(tx.clone(), vcontrol.clone(), command_name, &commands, entity, value)
                          .await
                        {
                          ControlFlow::Continue(()) => continue,
                          ControlFlow::Break(()) => break,
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
                          match send_entity_state(tx.clone(), vcontrol.clone(), command_name, &commands, entity, value)
                            .await
                          {
                            ControlFlow::Continue(()) => continue,
                            ControlFlow::Break(()) => break 'outer,
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

        // Wait indefinitely for the interrupts.
        future::pending::<()>().await;
      });
    }

    let _ = server_stopped_tx.send(());
    Ok(())
  };

  let main_server = tokio::spawn(main_server);

  let main_server = async {
    tokio::select! {
      res = main_server => res.unwrap(),
      _ = server_stop_rx => Ok(()),
    }
  };

  (main_server, server_stop_tx, server_stopped_rx)
}

async fn send_entity_state(
  mut tx: tokio::sync::mpsc::Sender<ProtoMessage>,
  vcontrol: Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
  command_name: &'static str,
  commands: &HashMap<&'static str, &'static Command>,
  entity: &ProtoMessage,
  value: vcontrol::Value,
) -> ControlFlow<()> {
  match entity {
    ProtoMessage::ListEntitiesBinarySensorResponse(res) => {
      let (state, missing_state) = bool_state(&value);

      let Some(state) = state else {
        warn!("Unsupported value for binary sensor {command_name}: {value:?}");
        return ControlFlow::Continue(());
      };

      let message = BinarySensorStateResponse { key: res.key, state, missing_state };
      match tx.send(ProtoMessage::BinarySensorStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
    },
    ProtoMessage::ListEntitiesSwitchResponse(res) => {
      let (state, _) = bool_state(&value);

      let Some(state) = state else {
        warn!("Unsupported value for switch {command_name}: {value:?}");
        return ControlFlow::Continue(());
      };

      let message = SwitchStateResponse { key: res.key, state };
      match tx.send(ProtoMessage::SwitchStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
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
        warn!("Unsupported value for sensor: {value:?}");
        return ControlFlow::Continue(());
      };

      let message = SensorStateResponse { key: res.key, state, missing_state };
      if command_name == "Ecotronic_Kesselstarts" {
        log::debug!("Ecotronic_Kesselstarts response: {message:?}");
      }
      match tx.send(ProtoMessage::SensorStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
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
        return ControlFlow::Continue(());
      };

      let message = NumberStateResponse { key: res.key, state, missing_state };
      match tx.send(ProtoMessage::NumberStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
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
        return ControlFlow::Continue(());
      };

      let message = DateStateResponse { key: res.key, missing_state, year, month, day };
      match tx.send(ProtoMessage::DateStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
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
        value => {
          warn!("Unsupported value for date-time: {value:?}");
          return ControlFlow::Continue(());
        },
      };

      let message = DateTimeStateResponse { key: res.key, missing_state, epoch_seconds };
      match tx.send(ProtoMessage::DateTimeStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
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
          let vcontrol = vcontrol.read().await;
          let vcontrol = vcontrol.lock().await;

          error.to_str(vcontrol.device()).unwrap_or_default().to_owned()
        },
        state => format!("{state:?}"),
      };

      let message = TextSensorStateResponse { key: res.key, missing_state, state };
      match tx.send(ProtoMessage::TextSensorStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
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
        return ControlFlow::Continue(());
      };

      let message = SelectStateResponse { key: res.key, missing_state, state };
      match tx.send(ProtoMessage::SelectStateResponse(message)).await {
        Ok(_receivers) => (),
        Err(value) => {
          log::error!("Failed to send message: {value:?}");
          return ControlFlow::Break(());
        },
      }
    },
    _ => (),
  }

  ControlFlow::Continue(())
}
