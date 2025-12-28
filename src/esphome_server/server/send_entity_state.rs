use std::collections::HashMap;
use std::sync::Weak;

use esphome_native_api::parser::ProtoMessage;
use esphome_native_api::proto::version_2025_12_1::{
  BinarySensorStateResponse, DateStateResponse, DateTimeStateResponse, NumberStateResponse, SelectStateResponse,
  SensorStateResponse, SwitchStateResponse, TextSensorStateResponse,
};
use tokio::sync::mpsc::error::SendError;
use vcontrol::{Command, VControl};

fn bool_state(value: &vcontrol::Value) -> Option<bool> {
  match value {
    vcontrol::Value::Int(0) => Some(false),
    vcontrol::Value::Int(1) => Some(true),
    _ => None,
  }
}

pub async fn send_entity_state(
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
          log::warn!("Unsupported value for binary sensor {command_name}: {value:?}");
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
        log::warn!("Unsupported value for number: {value:?}");
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
        log::warn!("Unsupported value for date: {value:?}");
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
        log::warn!("Unsupported value for sensor: {value:?}");
        return Ok(());
      };

      let message = SelectStateResponse { device_id, key: res.key, missing_state, state };
      tx.send(ProtoMessage::SelectStateResponse(message)).await?;
    },
    _ => (),
  }

  Ok(())
}
