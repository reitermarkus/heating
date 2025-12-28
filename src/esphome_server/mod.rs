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
use log::{debug, info, warn};
use mac_address::get_mac_address;
use tokio::net::TcpSocket;
use tokio::sync::broadcast;
use tokio::sync::oneshot::{self, Receiver, Sender};
use vcontrol::{Command, VControl};

use crate::esphome_server::entities::MultiEntity;
use crate::esphome_server::server::{handle_number_command, handle_switch_command};

mod entities;
mod entity;
mod server;
use server::{handle_date_command, handle_date_time_command, send_state};

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
      log::info!("Accepted request from {}", stream.peer_addr().unwrap());

      let vcontrol_weak = vcontrol_weak.clone();
      let commands = Arc::clone(&commands);
      let vcontrol_rx = vcontrol_rx.resubscribe();
      let entity_map = Arc::new(entities::entities(&commands));

      let encryption_key = env::var("ESPHOME_ENCRYPTION_KEY").unwrap_or_default();

      tokio::task::spawn(async move {
        let mut server = EspHomeApi::builder()
          .api_version_major(1)
          .api_version_minor(42)
          .server_info("ESPHome Rust".into())
          .esphome_version("2025.12.1".into()) // FIXME: Should be set by `esphome-native-api` automatically.
          .name("vitoligno_300c".into())
          .friendly_name("Vitoligno 300-C".into())
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
            ProtoMessage::SubscribeHomeassistantServicesRequest(SubscribeHomeassistantServicesRequest {}) => {
              log::info!("SubscribeHomeassistantServicesRequest");
            },
            ProtoMessage::SubscribeHomeAssistantStatesRequest(SubscribeHomeAssistantStatesRequest {}) => {
              log::info!("SubscribeHomeAssistantStatesRequest");
            },
            ProtoMessage::ListEntitiesRequest(ListEntitiesRequest {}) => {
              let mut entities = entity_map.values().collect::<Vec<_>>();
              entities.sort_by_key(|e| e.key());

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

              tx_clone.send(ProtoMessage::ListEntitiesDoneResponse(ListEntitiesDoneResponse {})).await.unwrap();
            },
            ProtoMessage::DateCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              let _ = handle_date_command(request, &entity_map, vcontrol, &tx).await; // TODO: Handle result.
            },
            ProtoMessage::DateTimeCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              let _ = handle_date_time_command(request, &entity_map, vcontrol, &tx).await; // TODO: Handle result.
            },
            ProtoMessage::NumberCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              let _ = handle_number_command(request, &entity_map, vcontrol, &tx).await; // TODO: Handle result.
            },
            ProtoMessage::SwitchCommandRequest(request) => {
              let Some(vcontrol) = vcontrol_weak.upgrade() else { break };
              let _ = handle_switch_command(request, &entity_map, vcontrol, &tx).await; // TODO: Handle result.
            },
            ProtoMessage::SubscribeStatesRequest(SubscribeStatesRequest {}) => {
              let tx = tx.clone();
              let vcontrol_rx = vcontrol_rx.resubscribe();
              let entity_map = Arc::clone(&entity_map);
              let commands = Arc::clone(&commands);

              tokio::spawn(async move {
                debug!("State retrieval loop");
                send_state(tx, vcontrol_rx, vcontrol_weak, entity_map, commands).await;
              });
            },
            request => {
              warn!("Unhandled request: {request:?}")
            },
          }
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
