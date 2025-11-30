use std::collections::HashMap;

use esphome_native_api::{
  parser::ProtoMessage,
  proto::version_2025_6_3::{
    EntityCategory, ListEntitiesBinarySensorResponse, ListEntitiesNumberResponse, ListEntitiesSensorResponse,
    NumberMode,
  },
};
use log::warn;
use vcontrol::Command;

pub fn entities(commands: HashMap<&'static str, &'static Command>) -> HashMap<&'static str, ProtoMessage> {
  let device_name = "vitoligno_300c";

  let numbers = [
    "Ecotronic_BedienNeigung_HK1",
    "Ecotronic_BedienNeigung_HK2",
    "Ecotronic_Brennstofflager_Füllstand",
    "Ecotronic_Brennstofflager_Maximalbegrenzung",
    "Ecotronic_Brennstofflager_Minimalbegrenzung",
  ];
  let sensors = [
    "Ecotronic_Puffertemperatur_1",
    "Ecotronic_Puffertemperatur_2",
    "Ecotronic_Puffertemperatur_3",
    "VT_SolltemperaturA1M1",
    "VT_SolltemperaturM2",
    "Ecotronic_Vorlauftemperatur_HK1",
    "Ecotronic_Vorlauftemperatur_HK2",
  ];
  let binary_sensors = ["Ecotronic_Pumpe_HK1", "Ecotronic_Pumpe_HK2"];

  let mut entity_map = HashMap::new();

  for number in numbers {
    let key = entity_map.len() as u32;

    let command = commands[number];
    let unit = command.unit().unwrap_or_default();

    let device_class = match unit {
      "" => {
        warn!("Unknown device class for number: {number}");
        ""
      },
      "°C" => "temperature",
      "kg" => "weight",
      unit => {
        warn!("Unknown device class for unit: {unit}");
        "None"
      },
    };

    let entity_category =
      if command.access_mode().is_write() { EntityCategory::Config } else { EntityCategory::Diagnostic };

    entity_map.insert(
      number,
      ProtoMessage::ListEntitiesNumberResponse(ListEntitiesNumberResponse {
        object_id: format!("{device_name}_{number}"), // TODO
        key,
        name: number.to_string(),
        unique_id: format!("number_{number}"),
        icon: "".into(), // TODO
        unit_of_measurement: unit.to_owned(),
        device_class: device_class.to_owned(),
        min_value: command.lower_bound().map(|v| v as f32).unwrap_or(f32::MIN),
        max_value: command.upper_bound().map(|v| v as f32).unwrap_or(f32::MAX),
        step: 0.1,
        disabled_by_default: false,
        entity_category: entity_category as i32, // EntityCategory::None as i32 // TODO
        mode: NumberMode::Box as i32,
      }),
    );
  }

  for sensor in sensors {
    let key = entity_map.len() as u32;

    let command = commands[sensor];
    let unit = command.unit().unwrap_or_default();

    let device_class = match unit {
      "" => {
        warn!("Unknown device class for sensor: {sensor}");
        ""
      },
      "°C" => "temperature",
      "kg" => "weight",
      unit => {
        warn!("Unknown device class for unit: {unit}");
        "None"
      },
    };

    let entity_category =
      if command.access_mode().is_write() { EntityCategory::Config } else { EntityCategory::Diagnostic };

    entity_map.insert(
      sensor,
      ProtoMessage::ListEntitiesSensorResponse(ListEntitiesSensorResponse {
        object_id: format!("{device_name}_{sensor}"), // TODO
        key,
        name: sensor.to_string(),
        unique_id: format!("sensor_{sensor}"),
        icon: "".into(), // TODO
        unit_of_measurement: unit.to_owned(),
        accuracy_decimals: 2, // TODO
        force_update: false,
        device_class: device_class.to_owned(),
        state_class: 1,            // SensorStateClass::StateClassMeasurement as i32 // TODO
        legacy_last_reset_type: 0, // SensorLastResetType::LastResetNone as i32      // TODO
        disabled_by_default: false,
        entity_category: entity_category as i32, // EntityCategory::None as i32 // TODO
      }),
    );
  }

  for binary_sensor in binary_sensors {
    let key = entity_map.len() as u32;
    entity_map.insert(
      binary_sensor,
      ProtoMessage::ListEntitiesBinarySensorResponse(ListEntitiesBinarySensorResponse {
        object_id: format!("{device_name}_{binary_sensor}"), // TODO
        key,
        name: binary_sensor.to_string(),
        unique_id: format!("binary_sensor_{binary_sensor}"), // TODO
        icon: "".into(),                                     // TODO
        device_class: "".into(),                             // TODO
        is_status_binary_sensor: true,
        disabled_by_default: false,
        entity_category: 0, // EntityCategory::None as i32
      }),
    );
  }

  entity_map
}
