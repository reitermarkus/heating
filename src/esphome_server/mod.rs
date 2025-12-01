use std::collections::HashMap;
use std::io;
use std::{future, net::SocketAddr, sync::Weak, time::Duration};

use esphome_native_api::esphomeapi::EspHomeApi;
use esphome_native_api::parser::ProtoMessage;
use esphome_native_api::proto::version_2025_6_3::{BinarySensorStateResponse, NumberStateResponse};
use esphome_native_api::proto::version_2025_6_3::{ListEntitiesDoneResponse, SensorStateResponse};
use log::{debug, info, warn};
use mac_address::get_mac_address;
use tokio::sync::oneshot::{self, Receiver, Sender};
use tokio::{net::TcpSocket, time::sleep};
use vcontrol::Command;
use webthing::Thing;

mod entities;

pub async fn start(
  port: u16,
  thing: Weak<std::sync::RwLock<Box<dyn Thing + 'static>>>,
  commands: HashMap<&'static str, &'static Command>,
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

    let commands = commands.clone();

    loop {
      info!("Waiting for connection.");
      let (stream, _) = listener.accept().await.expect("Failed to accept connection");
      debug!("Accepted request from {}", stream.peer_addr().unwrap());

      let commands = commands.clone();
      let thing = Weak::clone(&thing);

      tokio::task::spawn(async move {
        let mut server = EspHomeApi::builder()
          .api_version_major(1)
          .api_version_minor(42)
          .server_info("ESPHome Rust".to_string())
          .name("vitoligno_300c".to_string())
          .friendly_name("Vitoligno 300-C".to_string())
          // .bluetooth_mac_address("00:00:00:00:00:00".to_string())
          .mac(format!("{}", mac_address))
          .manufacturer("Viessmann".to_string())
          .model("Vitoligno 300-C".to_string())
          .suggested_area("Boiler Room".to_string())
          .encryption_key("px7tsbK3C7bpXHr2OevEV2ZMg/FrNBw2+O2pNPbedtA=".to_string())
          .build();

        let (tx, mut rx) = server.start(stream).await.expect("Failed to start server");
        let tx_clone = tx.clone();
        debug!("Server started");

        let commands_clone = commands.clone();
        let thing = Weak::clone(&thing);

        tokio::spawn(async move {
          let commands = commands_clone;

          loop {
            let message = rx.recv().await;
            if message.as_ref().is_err() {
              info!("Connection closed or error: {:?}", &message);
              return;
            }
            // Process the received message
            debug!("Received message: {:?}", message);

            match message.unwrap() {
              ProtoMessage::ListEntitiesRequest(list_entities_request) => {
                debug!("ListEntitiesRequest: {:?}", list_entities_request);

                let entity_map = entities::entities(commands.clone());
                let mut entities = entity_map.values().collect::<Vec<_>>();
                entities.sort_by_key(|entity| match entity {
                  ProtoMessage::ListEntitiesBinarySensorResponse(res) => res.key,
                  ProtoMessage::ListEntitiesSensorResponse(res) => res.key,
                  ProtoMessage::ListEntitiesNumberResponse(res) => res.key,
                  _ => u32::MAX,
                });

                for entity in entities {
                  dbg!(&entity);
                  tx_clone.send(entity.clone()).unwrap();
                }

                debug!("ListEntitiesDoneResponse");
                tx_clone.send(ProtoMessage::ListEntitiesDoneResponse(ListEntitiesDoneResponse {})).unwrap();
              },
              ProtoMessage::SubscribeStatesRequest(req) => {
                debug!("SubscribeStatesRequest: {req:?}");

                let tx_clone = tx.clone();
                let commands_clone = commands.clone();
                let thing = Weak::clone(&thing);

                tokio::spawn(async move {
                  let tx = tx_clone;
                  let commands = commands_clone;

                  debug!("State retrieval loop");

                  let entity_map = entities::entities(commands.clone());

                  loop {
                    sleep(Duration::from_secs(3)).await;

                    let Some(thing) = thing.upgrade() else { break };

                    for (entity_name, entity) in &entity_map {
                      let thing = thing.read().unwrap();
                      let Some(value) = thing.get_property(&entity_name) else { continue };

                      match entity {
                        ProtoMessage::ListEntitiesBinarySensorResponse(res) => {
                          let mut missing_state = false;
                          let state = match value {
                            serde_json::Value::Null => {
                              missing_state = true;
                              Some(false)
                            },
                            serde_json::Value::Bool(b) => Some(b),
                            serde_json::Value::Number(ref n) => match n.as_i64() {
                              Some(0) => Some(false),
                              Some(1) => Some(true),
                              _ => None,
                            },
                            _ => None,
                          };

                          let Some(state) = state else {
                            warn!("Unsupported value for binary sensor: {value:?}");
                            continue;
                          };

                          let message = BinarySensorStateResponse { key: res.key, state, missing_state };
                          tx.send(ProtoMessage::BinarySensorStateResponse(message)).expect("Failed to send message");
                          continue;
                        },
                        ProtoMessage::ListEntitiesSensorResponse(res) => {
                          let mut missing_state = false;
                          let state = match value {
                            serde_json::Value::Null => {
                              missing_state = true;
                              Some(0.0)
                            },
                            serde_json::Value::Number(ref n) => n
                              .as_f64()
                              .map(|n| n as f32)
                              .or(n.as_i128().map(|n| n as f32))
                              .or(n.as_u128().map(|n| n as f32)),
                            _ => None,
                          };

                          let Some(state) = state else {
                            warn!("Unsupported value for sensor: {value:?}");
                            continue;
                          };

                          let message = SensorStateResponse { key: res.key, state, missing_state };
                          tx.send(ProtoMessage::SensorStateResponse(message)).expect("Failed to send message");
                          continue;
                        },
                        ProtoMessage::ListEntitiesNumberResponse(res) => {
                          let mut missing_state = false;
                          let state = match value {
                            serde_json::Value::Null => {
                              missing_state = true;
                              Some(0.0)
                            },
                            serde_json::Value::Number(ref n) => n
                              .as_f64()
                              .map(|n| n as f32)
                              .or(n.as_i128().map(|n| n as f32))
                              .or(n.as_u128().map(|n| n as f32)),
                            _ => None,
                          };

                          let Some(state) = state else {
                            warn!("Unsupported value for number: {value:?}");
                            continue;
                          };

                          let message = NumberStateResponse { key: res.key, state, missing_state };
                          tx.send(ProtoMessage::NumberStateResponse(message)).expect("Failed to send message");
                          continue;
                        },
                        _ => continue,
                      };
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
