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
    "Ecotronic_BedienNiveauM1",
    "Ecotronic_BedienNiveauM2",
    "Ecotronic_BedienNeigung_HK1",
    "Ecotronic_BedienNeigung_HK2",
    "Ecotronic_Brennstofflager_Füllstand",
    "Ecotronic_Brennstofflager_Maximalbegrenzung",
    "Ecotronic_Brennstofflager_Minimalbegrenzung",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_1",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_2",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_3",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_4",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_5",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_6",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_7",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_8",
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_Soll",
    "Ecotronic_Puffer_Betriebsart",
    "Ecotronic_Puffer_Niveau",
    "Ecotronic_Puffer_Neigung",
    "Ecotronic_Kesselsolltemperatur",
    "Ecotronic_Kessel_Rücklauf_Soll",
    "Ecotronic_Heizung_Wunschtemperatur_HK1",
    "Ecotronic_Heizung_Wunschtemperatur_HK2",
    "Ecotronic_Betriebsart_HK1",
    "Ecotronic_Betriebsart_HK2",
    "Ecotronic_Bedien_WW_Solltemperatur",
    "Ecotronic_BedienSparbetrieb_HK1",
    "Ecotronic_BedienSparbetrieb_HK2",
    "Ecotronic_BedienPartybetriebM1",
    "Ecotronic_BedienPartybetriebM2",
  ];
  let sensors = [
    "Ecotronic_Puffertemperatur_1",
    "Ecotronic_Puffertemperatur_2",
    "Ecotronic_Puffertemperatur_3",
    "Ecotronic_Puffertemperatur_Ist",
    "Ecotronic_Puffertemperatur_Mittelwert",
    "Ecotronic_Puffertemperatur_Soll",
    "Ecotronic_Puffersoll_Maximal",
    "Ecotronic_Puffersoll_Minimal",
    "VT_SolltemperaturA1M1",
    "VT_SolltemperaturM2",
    "Ecotronic_Vorlauftemperatur_HK1",
    "Ecotronic_Vorlauftemperatur_HK2",
    "Temperatur_2_M1",
    "Temperatur_2_M2",
    "SC100_KesselIsttemperatur",
    "SC100_Lambdasonde",
    "Gemischte_AT",
    "Ecotronic_Gemischte_AT",
    "Ecotronic_Umschalteinheit_Sonde",
    "Ecotronic_Umschalteinheit_Sonde_Laufzeit",
    "Ecotronic_Mischerposition_HK1",
    "Ecotronic_Mischerposition_HK2",
    "Ecotronic_Kesselrücklauftemperatur",
    "Ecotronic_Füllstand_Entaschung",
    "Ecotronic_Füllstand_Pellet",
    "Ecotronic_Brennstoffverbrauch",
    "Ecotronic_Betriebsstunden_Volllast",
    "Ecotronic_Betriebsstunden_Teillast",
    "Ecotronic_Betriebsstunden_Saugmodul",
    "Ecotronic_Betriebsstunden_Kessel",
    "Ecotronic_Betriebsstunden_Einschubschnecke",
    "Ecotronic_Betriebsminuten_Einschubschnecke",
  ];
  let binary_sensors =
    ["Ecotronic_Pumpe_HK1", "Ecotronic_Pumpe_HK2", "Ecotronic_Heizungstatus", "Ecotronic_Heizungstatus_HK2"];

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
