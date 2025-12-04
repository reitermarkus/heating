use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::{future, net::SocketAddr, sync::Weak, time::Duration};

use esphome_native_api::esphomeapi::EspHomeApi;
use esphome_native_api::parser::ProtoMessage;
use esphome_native_api::proto::version_2025_6_3::{
  BinarySensorStateResponse, NumberCommandRequest, NumberStateResponse,
};
use esphome_native_api::proto::version_2025_6_3::{ListEntitiesDoneResponse, SensorStateResponse};
use log::{debug, info, warn};
use mac_address::get_mac_address;
use tokio::net::TcpSocket;
use tokio::sync::broadcast;
use tokio::sync::oneshot::{self, Receiver, Sender};
use tokio::time::sleep;
use vcontrol::{Command, VControl, Value};

mod entities;

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

    let commands = commands.clone();

    loop {
      info!("Waiting for connection.");
      let (stream, _) = listener.accept().await.expect("Failed to accept connection");
      debug!("Accepted request from {}", stream.peer_addr().unwrap());

      let commands = commands.clone();
      let vcontrol = vcontrol.clone();
      let vcontrol_rx = vcontrol_rx.resubscribe();
      let entity_map = Arc::new(entities::entities(commands.clone()));

      let map_entity_to_key = |entity: &ProtoMessage| match entity {
        ProtoMessage::ListEntitiesBinarySensorResponse(res) => res.key,
        ProtoMessage::ListEntitiesSensorResponse(res) => res.key,
        ProtoMessage::ListEntitiesNumberResponse(res) => res.key,
        _ => u32::MAX,
      };

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

        let entity_map = Arc::clone(&entity_map);
        let vcontrol = Arc::clone(&vcontrol);

        tokio::spawn(async move {
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

                let mut entities = entity_map.values().collect::<Vec<_>>();
                entities.sort_by_key(|e| map_entity_to_key(e));

                for entity in entities {
                  tx_clone.send(entity.clone()).unwrap();
                  sleep(Duration::from_millis(10)).await; // FIXME: Make channel buffer in `EspHomeApi` bigger.
                }

                debug!("ListEntitiesDoneResponse");
                tx_clone.send(ProtoMessage::ListEntitiesDoneResponse(ListEntitiesDoneResponse {})).unwrap();
              },
              ProtoMessage::NumberCommandRequest(NumberCommandRequest { key, state }) => {
                let Some((entity_name, entity)) = entity_map.iter().find(|(_, e)| map_entity_to_key(e) == key) else {
                  warn!("Unknown number command: {key}");
                  continue;
                };

                let vcontrol = vcontrol.write().await;
                let mut vcontrol = vcontrol.lock().await;

                log::info!("Setting value for {entity_name}: {state}");
                if let Err(err) = vcontrol.set(entity_name, Value::Double(state as f64)).await {
                  log::error!("Failed to set value ({state}) for {entity_name}: {err}")
                }
              },
              ProtoMessage::SubscribeStatesRequest(req) => {
                debug!("SubscribeStatesRequest: {req:?}");

                let tx = tx.clone();
                let mut vcontrol_rx = vcontrol_rx.resubscribe();
                let entity_map = Arc::clone(&entity_map);

                tokio::spawn(async move {
                  debug!("State retrieval loop");

                  loop {
                    let (command_name, value) = match vcontrol_rx.recv().await {
                      Ok(res) => res,
                      Err(broadcast::error::RecvError::Closed) => break,
                      Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("Receiver lagged, {n} messages skipped.");
                        continue;
                      },
                    };

                    let Some(entity) = entity_map.get(command_name) else { continue };

                    match entity {
                      ProtoMessage::ListEntitiesBinarySensorResponse(res) => {
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

                        let Some(state) = state else {
                          warn!("Unsupported value for binary sensor: {value:?}");
                          continue;
                        };

                        let message = BinarySensorStateResponse { key: res.key, state, missing_state };
                        tx.send(ProtoMessage::BinarySensorStateResponse(message)).expect("Failed to send message");
                        sleep(Duration::from_millis(100)).await;
                        continue;
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
                          continue;
                        };

                        let message = SensorStateResponse { key: res.key, state, missing_state };
                        tx.send(ProtoMessage::SensorStateResponse(message)).expect("Failed to send message");
                        sleep(Duration::from_millis(100)).await;
                        continue;
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
                          continue;
                        };

                        let message = NumberStateResponse { key: res.key, state, missing_state };
                        match tx.send(ProtoMessage::NumberStateResponse(message)) {
                          Ok(_receivers) => (),
                          Err(value) => log::error!("Failed to send message: {value:?}"),
                        }
                        sleep(Duration::from_millis(100)).await;
                        continue;
                      },
                      _ => continue,
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
