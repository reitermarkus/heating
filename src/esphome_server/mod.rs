use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::{env, io};

use esphome_native_api::esphomeapi::EspHomeApi;
use esphome_native_api::parser::ProtoMessage;
use esphome_native_api::proto::version_2025_12_1::{
  ListEntitiesDoneResponse, ListEntitiesRequest, SubscribeHomeAssistantStatesRequest,
  SubscribeHomeassistantServicesRequest, SubscribeStatesRequest,
};

use mac_address::get_mac_address;
use tokio::net::TcpSocket;
use tokio::sync::broadcast;
use tokio::sync::oneshot::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use vcontrol::{Command, VControl};

use crate::esphome_server::entities::MultiEntity;
use crate::esphome_server::server::{handle_number_command, handle_switch_command};

mod entities;
mod entity;
mod server;
use server::{handle_date_command, handle_date_time_command, send_state_loop};

pub async fn start(
  vcontrol_weak: Weak<tokio::sync::Mutex<VControl>>,
  commands: HashMap<&'static str, &'static Command>,
  vcontrol_rx: broadcast::Receiver<(&'static str, vcontrol::Value)>,
) -> (impl Future<Output = Result<(), io::Error>>, Sender<()>, Receiver<()>) {
  let (server_stopped_tx, server_stopped_rx) = oneshot::channel();
  let (server_stop_tx, server_stop_rx) = oneshot::channel();

  let entities = entities::entities(&commands);

  let addr = SocketAddr::from(([0, 0, 0, 0], 6053));
  let socket = TcpSocket::new_v4().unwrap();
  socket.set_reuseaddr(true).unwrap();
  socket.bind(addr).unwrap();

  let listener = socket.listen(128).unwrap();
  log::debug!("Listening on: {}", addr);

  let mac_address = get_mac_address().unwrap().unwrap_or_default();
  let encryption_key = env::var("ESPHOME_ENCRYPTION_KEY").unwrap_or_default();

  let main_server = async move {
    log::info!("ESPHome server started.");

    let commands = Arc::new(commands);
    let entity_map = Arc::new(entities);

    loop {
      log::info!("Waiting for connection.");
      let stream = match listener.accept().await {
        Ok((stream, _)) => stream,
        Err(err) => {
          log::error!("Failed to accept connection: {err}");
          break;
        },
      };

      let peer_addr = stream.peer_addr().unwrap();
      log::info!("Accepted request from {peer_addr}.");
      let vcontrol_weak = vcontrol_weak.clone();
      let commands = commands.clone();
      let vcontrol_rx = vcontrol_rx.resubscribe();
      let entity_map = entity_map.clone();
      let encryption_key = encryption_key.clone();

      tokio::task::spawn(async move {
        log::info!("Starting ESPHome API server to {peer_addr}.");

        let mut server = EspHomeApi::builder()
          .api_version_major(1)
          .api_version_minor(42)
          .server_info("ESPHome Rust".into())
          // .esphome_version("2025.12.1".into()) // FIXME: Should be set by `esphome-native-api` automatically.
          .name("vitoligno_300c".into())
          .friendly_name("Vitoligno 300-C".into())
          .mac(mac_address.to_string())
          // .bluetooth_mac_address(bluetooth_mac_address.to_string())
          // .bluetooth_proxy_feature_flags(0b1111111)
          .manufacturer("Viessmann".to_string())
          .model("Vitoligno 300-C".to_string())
          .suggested_area("Boiler Room".to_string())
          .encryption_key(encryption_key.clone())
          .build();

        let (tx, mut rx) = server.start(stream).await.expect("Failed to start server");
        let tx_clone = tx.clone();

        let entity_map = Arc::clone(&entity_map);

        let mut send_state_loop_task: Option<JoinHandle<()>> = None;

        loop {
          let message = match rx.recv().await {
            Ok(message) => message,
            Err(broadcast::error::RecvError::Closed) => {
              log::info!("Connection to {peer_addr} closed.");
              break;
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
              log::warn!("Receiver lagged, {n} messages lost.");
              continue;
            },
          };

          let res = match message {
            ProtoMessage::SubscribeHomeassistantServicesRequest(SubscribeHomeassistantServicesRequest {}) => {
              log::info!("SubscribeHomeassistantServicesRequest");
              Ok(())
            },
            ProtoMessage::SubscribeHomeAssistantStatesRequest(SubscribeHomeAssistantStatesRequest {}) => {
              log::info!("SubscribeHomeAssistantStatesRequest");
              Ok(())
            },
            ProtoMessage::ListEntitiesRequest(ListEntitiesRequest {}) => {
              log::info!("ListEntitiesRequest");

              let mut entities = entity_map.values().collect::<Vec<_>>();
              entities.sort_by_key(|e| e.key());

              let mut res = Ok(());

              'outer: for entity in entities {
                match entity {
                  MultiEntity::Single(entity) => {
                    if let Err(err) = tx_clone.send(*entity.clone()).await {
                      res = Err(err);
                      break 'outer;
                    }
                  },
                  MultiEntity::Multiple(entities) => {
                    for entity in entities {
                      if let Err(err) = tx_clone.send(entity.clone()).await {
                        res = Err(err);
                        break 'outer;
                      }
                    }
                  },
                }
              }

              if let Err(err) = res {
                Err(err)
              } else {
                tx_clone.send(ProtoMessage::ListEntitiesDoneResponse(ListEntitiesDoneResponse {})).await
              }
            },
            ProtoMessage::DateCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              handle_date_command(request, &entity_map, vcontrol, &tx).await
            },
            ProtoMessage::DateTimeCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              handle_date_time_command(request, &entity_map, vcontrol, &tx).await
            },
            ProtoMessage::NumberCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              handle_number_command(request, &entity_map, vcontrol, &tx).await
            },
            ProtoMessage::SwitchCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              handle_switch_command(request, &entity_map, vcontrol, &tx).await
            },
            ProtoMessage::SubscribeStatesRequest(SubscribeStatesRequest {}) => {
              let tx = tx.clone();
              let vcontrol_rx = vcontrol_rx.resubscribe();
              let entity_map = Arc::clone(&entity_map);
              let commands = Arc::clone(&commands);

              if let Some(send_state_loop_task) = send_state_loop_task.take() {
                log::info!("Stopping previous “send state” loop.");
                send_state_loop_task.abort();
              }

              let vcontrol_weak = vcontrol_weak.clone();
              send_state_loop_task = Some(tokio::spawn(async move {
                log::info!("Starting “send state” loop.");
                send_state_loop(tx, vcontrol_rx, vcontrol_weak, entity_map, commands).await;
              }));

              Ok(())
            },
            request => {
              log::warn!("Unhandled request: {request:?}");
              Ok(())
            },
          };

          if let Err(err) = res {
            log::error!("Error handling request: {err}");
            break;
          }
        }

        if let Some(send_state_loop_task) = send_state_loop_task.take() {
          log::info!("Stopping “send state” loop.");
          send_state_loop_task.abort();
        }
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
