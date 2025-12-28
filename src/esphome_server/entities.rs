use std::collections::{BTreeMap, HashMap};

use esphome_native_api::{
  parser::ProtoMessage,
  proto::version_2025_12_1::{
    EntityCategory, ListEntitiesBinarySensorResponse, ListEntitiesDateResponse, ListEntitiesDateTimeResponse,
    ListEntitiesNumberResponse, ListEntitiesSelectResponse, ListEntitiesSensorResponse, ListEntitiesSwitchResponse,
    ListEntitiesTextSensorResponse, NumberMode,
  },
};
use log::warn;
use vcontrol::{Command, DataType};

use super::entity::{Entity, EntityType};

const ENTITIES: &[(&'static str, Entity)] = &[
  // Buffer
  ("Ecotronic_Kessel_Ein_Aus", Entity { entity_name: "Boiler", entity_type: EntityType::Switch }),
  (
    "Ecotronic_Puffer_Betriebsart",
    Entity {
      entity_name: "Buffer Operating Mode",
      entity_type: EntityType::Select { category: EntityCategory::Config },
    },
  ),
  (
    "Ecotronic_Pufferladezustand",
    Entity {
      entity_name: "Buffer Load State",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Puffertemperatur_Mittelwert",
    Entity {
      entity_name: "Buffer Mean Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Puffersoll_Minimal",
    Entity { entity_name: "Buffer Minimum Temperature", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Puffersoll_Maximal",
    Entity { entity_name: "Buffer Maximum Temperature", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Puffertemperatur_Soll",
    Entity {
      entity_name: "Buffer Desired Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Puffertemperatur_Ist",
    Entity {
      entity_name: "Buffer Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Puffertemperatur_1",
    Entity {
      entity_name: "Buffer Temperature 1",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Puffertemperatur_2",
    Entity {
      entity_name: "Buffer Temperature 2",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Puffertemperatur_3",
    Entity {
      entity_name: "Buffer Temperature 3",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  ("Ecotronic_Puffer_Niveau", Entity { entity_name: "Buffer Niveau", entity_type: EntityType::Number { step: 1.0 } }),
  ("Ecotronic_Puffer_Neigung", Entity { entity_name: "Buffer Incline", entity_type: EntityType::Number { step: 0.1 } }),
  // Hot Water
  (
    "Ecotronic_Bedien_WW_Solltemperatur",
    Entity { entity_name: "Hot Water Desired Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  // Heating Circuit 1
  (
    "Ecotronic_Betriebsart_HK1",
    Entity { entity_name: "HC1 Operating Mode", entity_type: EntityType::Select { category: EntityCategory::Config } },
  ),
  (
    "Ecotronic_Raumsoll_Normal_HK1",
    Entity { entity_name: "HC1 Desired Room Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  (
    "Ecotronic_Raumsoll_Reduziert_HK1",
    Entity { entity_name: "HC1 Desired Reduced Room Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  (
    "VT_SolltemperaturA1M1",
    Entity {
      entity_name: "HC1 Desired Flow Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Heizung_Wunschtemperatur_HK1",
    Entity {
      entity_name: "HC1 Desired Heating Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Vorlauftemperatur_HK1",
    Entity {
      entity_name: "HC1 Flow Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  // (
  //   "Temperatur_2_M1", // Same as `Ecotronic_Vorlauftemperatur_HK1`.
  //   Entity {
  //     entity_name: "hc1_temperature_2",
  //     entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
  //   },
  // ),
  (
    "Ecotronic_HK_Ferienbetrieb_HK1",
    Entity {
      entity_name: "HC1 Vacation Mode",
      entity_type: EntityType::BinarySensor { category: EntityCategory::None },
    },
  ),
  ("Ecotronic_FerienBeginn_HK1", Entity { entity_name: "HC1 Vacaction Mode Begin", entity_type: EntityType::Date }),
  ("Ecotronic_FerienEnde_HK1", Entity { entity_name: "HC1 Vacation Mode End", entity_type: EntityType::Date }),
  (
    "Ecotronic_BedienPartybetriebM1",
    Entity { entity_name: "HC1 Desired Party Mode Temperature", entity_type: EntityType::Number { step: 1.0 } },
  ),
  ("Ecotronic_BedienSparbetrieb_HK1", Entity { entity_name: "HC1 Energy Saver Mode", entity_type: EntityType::Switch }),
  ("Ecotronic_BedienNiveauM1", Entity { entity_name: "HC1 Niveau", entity_type: EntityType::Number { step: 1.0 } }),
  ("Ecotronic_BedienNeigung_HK1", Entity { entity_name: "HC1 Incline", entity_type: EntityType::Number { step: 0.1 } }),
  (
    "Ecotronic_Pumpe_HK1",
    Entity { entity_name: "HC1 Pump", entity_type: EntityType::BinarySensor { category: EntityCategory::Diagnostic } },
  ),
  (
    "Ecotronic_Mischerposition_HK1",
    Entity {
      entity_name: "HC1 Mixer Position",
      entity_type: EntityType::Sensor { accuracy_decimals: 0, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Heizungstatus",
    Entity {
      entity_name: "HC1 Heating Status",
      entity_type: EntityType::TextSensor { category: EntityCategory::Diagnostic },
    },
  ),
  // Heating Circuit 2
  (
    "Ecotronic_Betriebsart_HK2",
    Entity { entity_name: "HC2 Operating Mode", entity_type: EntityType::Select { category: EntityCategory::Config } },
  ),
  (
    "Ecotronic_Raumsoll_Normal_HK2",
    Entity { entity_name: "HC2 Desired Room Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  (
    "Ecotronic_Raumsoll_Reduziert_HK2",
    Entity { entity_name: "HC2 Desired Reduced Room Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  (
    "Ecotronic_Vorlauftemperatur_HK2",
    Entity {
      entity_name: "HC2 Flow Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  // (
  //   "Temperatur_2_M2", // Same as `Ecotronic_Vorlauftemperatur_HK2`.
  //   Entity { entity_name: "hc2_temperature_2", entity_type: EntityType::Sensor { accuracy_decimals: 1 },
  //     category: EntityCategory::None },
  // ),
  (
    "Ecotronic_Heizung_Wunschtemperatur_HK2",
    Entity {
      entity_name: "HC2 Desired Heating Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "VT_SolltemperaturM2",
    Entity {
      entity_name: "HC2 Desired Flow Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_HK_Ferienbetrieb_HK2",
    Entity {
      entity_name: "HC2 Vacation Mode",
      entity_type: EntityType::BinarySensor { category: EntityCategory::None },
    },
  ),
  ("Ecotronic_FerienBeginn_HK2", Entity { entity_name: "HC2 Vacation Mode Begin", entity_type: EntityType::Date }),
  ("Ecotronic_FerienEnde_HK2", Entity { entity_name: "HC2 Vacation Mode End", entity_type: EntityType::Date }),
  (
    "Ecotronic_BedienPartybetriebM2",
    Entity { entity_name: "HC2 Desired Party Mode Temperature", entity_type: EntityType::Number { step: 1.0 } },
  ),
  ("Ecotronic_BedienSparbetrieb_HK2", Entity { entity_name: "HC2 Energy Saver Mode", entity_type: EntityType::Switch }),
  ("Ecotronic_BedienNiveauM2", Entity { entity_name: "HC2 Niveau", entity_type: EntityType::Number { step: 1.0 } }),
  ("Ecotronic_BedienNeigung_HK2", Entity { entity_name: "HC2 Incline", entity_type: EntityType::Number { step: 0.1 } }),
  (
    "Ecotronic_Pumpe_HK2",
    Entity { entity_name: "HC2 Pump", entity_type: EntityType::BinarySensor { category: EntityCategory::Diagnostic } },
  ),
  (
    "Ecotronic_Mischerposition_HK2",
    Entity {
      entity_name: "HC2 Mixer Position",
      entity_type: EntityType::Sensor { accuracy_decimals: 0, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Heizungstatus_HK2",
    Entity {
      entity_name: "HC2 Heating Status",
      entity_type: EntityType::TextSensor { category: EntityCategory::Diagnostic },
    },
  ),
  // Boiler
  (
    "Ecotronic_Kesseltype",
    Entity { entity_name: "Boiler Type", entity_type: EntityType::TextSensor { category: EntityCategory::Diagnostic } },
  ),
  (
    "Ecotronic_Kesselstatus",
    Entity {
      entity_name: "Boiler Status",
      entity_type: EntityType::TextSensor { category: EntityCategory::Diagnostic },
    },
  ),
  (
    "SC100_KesselIsttemperatur",
    Entity {
      entity_name: "Boiler Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Abgastemperatur",
    Entity {
      entity_name: "Boiler Exhaust Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "SC100_Lambdasonde",
    Entity {
      entity_name: "Boiler Exhaust Rest O2",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "SC100_PositionPrimaerluftklappe",
    Entity {
      entity_name: "Boiler Primary Flap Position",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "SC100_PositionSekundaerluftklappe",
    Entity {
      entity_name: "Boiler Secondary Flap Position",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Kesselsolltemperatur",
    Entity { entity_name: "Boiler Desired Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  (
    "Ecotronic_Kessel_Rücklauf_Soll",
    Entity { entity_name: "Boiler Desired Return Temperature", entity_type: EntityType::Number { step: 0.1 } },
  ),
  (
    "Ecotronic_Kesselrücklauftemperatur",
    Entity {
      entity_name: "Boiler Return Temperature",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  ("Ecotronic_Kesselstarts", Entity { entity_name: "Boiler Starts", entity_type: EntityType::Number { step: 1.0 } }),
  (
    "Ecotronic_Betriebsstunden_Volllast",
    Entity {
      entity_name: "Operating Hours Full Load",
      entity_type: EntityType::Sensor { accuracy_decimals: 3, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Betriebsstunden_Teillast",
    Entity {
      entity_name: "Operating Hours Partial Load",
      entity_type: EntityType::Sensor { accuracy_decimals: 3, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Betriebsstunden_Kessel",
    Entity {
      entity_name: "Boiler Operating Hours",
      entity_type: EntityType::Sensor { accuracy_decimals: 3, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Betriebsstunden_Einschubschnecke",
    Entity {
      entity_name: "Pellet Worm Drive Operating Hours",
      entity_type: EntityType::Sensor { accuracy_decimals: 3, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Betriebsminuten_Einschubschnecke",
    Entity {
      entity_name: "Pellet Worm Drive Operating Minutes",
      entity_type: EntityType::Sensor { accuracy_decimals: 3, category: EntityCategory::Diagnostic },
    },
  ),
  // Ash
  (
    "Ecotronic_Füllstand_Entaschung",
    Entity {
      entity_name: "Ash Level",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  // Pellets
  (
    "Ecotronic_Brennstofflager_Füllstand",
    Entity { entity_name: "Pellet Silo Level", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Brennstofflager_Minimalbegrenzung",
    Entity { entity_name: "Pellet Silo Minimum Level", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Brennstofflager_Maximalbegrenzung",
    Entity { entity_name: "Pellet Silo Maximum Level", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Füllstand_Pellet",
    Entity {
      entity_name: "Pellet Level",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Brennstoffverbrauch",
    Entity {
      entity_name: "Pellet Consumption per Hour",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "NRF_Brennstoffverbrauch_Bedien",
    Entity {
      entity_name: "Pellet Consumption",
      entity_type: EntityType::Sensor { accuracy_decimals: 0, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Pellet_Leerfahrzeit",
    Entity { entity_name: "Pellet Hopper Empty Time", entity_type: EntityType::Number { step: 1.0 } },
  ),
  // Outside Temperature
  (
    "NRF_TemperaturFehler_ATS",
    Entity {
      entity_name: "Outside Temperature Status",
      entity_type: EntityType::BinarySensor { category: EntityCategory::None },
    },
  ),
  (
    "NRF_TiefpassTemperaturwert_ATS",
    Entity {
      entity_name: "Outside Temperature Lowpass",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Gemischte_AT",
    Entity {
      entity_name: "Outside Temperature Mixed",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  (
    "Ecotronic_Gemischte_AT",
    Entity {
      entity_name: "Outside Temperature Mixed 2",
      entity_type: EntityType::Sensor { accuracy_decimals: 1, category: EntityCategory::None },
    },
  ),
  // Changeover Unit
  (
    "Ecotronic_Umschalteinheit_Sonde",
    Entity {
      entity_name: "Changeover Unit Current Probe",
      entity_type: EntityType::Sensor { accuracy_decimals: 0, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Umschalteinheit_Sonde_Laufzeit",
    Entity {
      entity_name: "Changeover Unit Current Probe Runtime",
      entity_type: EntityType::Sensor { accuracy_decimals: 0, category: EntityCategory::Diagnostic },
    },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_1",
    Entity { entity_name: "Changeover Unit Probe 1 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_2",
    Entity { entity_name: "Changeover Unit Probe 2 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_3",
    Entity { entity_name: "Changeover Unit Probe 3 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_4",
    Entity { entity_name: "Changeover Unit Probe 4 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_5",
    Entity { entity_name: "Changeover Unit Probe 5 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_6",
    Entity { entity_name: "Changeover Unit Probe 6 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_7",
    Entity { entity_name: "Changeover Unit Probe 7 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_8",
    Entity { entity_name: "Changeover Unit Probe 8 Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Umschalteinheit_Laufzeit_Sonde_Soll",
    Entity { entity_name: "Changeover Unit Desired Probe Runtime", entity_type: EntityType::Number { step: 1.0 } },
  ),
  (
    "Ecotronic_Betriebsstunden_Saugmodul",
    Entity {
      entity_name: "Changeover Unit Operating Hours",
      entity_type: EntityType::Sensor { accuracy_decimals: 3, category: EntityCategory::Diagnostic },
    },
  ),
  // Errors
  (
    "ecnsysEventType~ErrorIndex",
    Entity { entity_name: "Error", entity_type: EntityType::TextSensor { category: EntityCategory::Diagnostic } },
  ),
  (
    "ecnsysEventType~Error",
    Entity {
      entity_name: "Error History",
      entity_type: EntityType::TextSensor { category: EntityCategory::Diagnostic },
    },
  ),
  ("Ecotronic_Fehler_Quittierung", Entity { entity_name: "Error Acknowledgement", entity_type: EntityType::Switch }),
  (
    "NRF_Uhrzeit",
    Entity { entity_name: "System Time", entity_type: EntityType::DateTime { category: EntityCategory::Config } },
  ),
];

fn unit_to_device_class(unit: &str, entity_name: &str) -> &'static str {
  match unit {
    "" => {
      warn!("Unknown device class for entity without unit: {entity_name}");
      ""
    },
    "°C" | "K" => "temperature",
    "kg" => "weight",
    "h" | "min" | "s" => "duration",
    "kg/h" => "volume_flow_rate",
    unit => {
      warn!("Unknown device class for entity {entity_name} unit: {unit}");
      "None"
    },
  }
}

pub enum MultiEntity {
  Single(ProtoMessage),
  Multiple(Vec<ProtoMessage>),
}

impl From<ProtoMessage> for MultiEntity {
  fn from(message: ProtoMessage) -> Self {
    Self::Single(message)
  }
}

pub fn entities(commands: &HashMap<&'static str, &'static Command>) -> HashMap<&'static str, MultiEntity> {
  let device_id = 0;

  let mut entity_map = HashMap::new();

  let mut key = 0;

  for &(command_name, ref entity) in ENTITIES {
    let Some(command) = commands.get(command_name) else {
      log::warn!("Command '{command_name}' not found.");
      continue;
    };
    let unit = command.unit().unwrap_or_default();

    let name = entity.entity_name.to_owned();
    let entity_id = entity.entity_name.to_lowercase().split(' ').collect::<Vec<&str>>().join("_");
    let device_class = unit_to_device_class(unit, &entity_id);

    if command.access_mode().is_write() {
      assert_eq!(entity.category(), EntityCategory::Config, "Wrong category for {}", entity.entity_name);
    } else {
      assert_ne!(entity.category(), EntityCategory::Config, "Wrong category for {}", entity.entity_name);
    };

    match entity.entity_type {
      EntityType::Number { step } => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesNumberResponse(ListEntitiesNumberResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "".into(), // TODO
            unit_of_measurement: unit.to_owned(),
            device_class: device_class.to_owned(),
            min_value: command.lower_bound().map(|v| v as f32).unwrap_or(f32::MIN),
            max_value: command.upper_bound().map(|v| v as f32).unwrap_or(f32::MAX),
            step,
            disabled_by_default: false,
            entity_category: EntityCategory::Config as i32,
            mode: NumberMode::Box as i32,
          })
          .into(),
        );
      },
      EntityType::Sensor { accuracy_decimals, category } => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesSensorResponse(ListEntitiesSensorResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "".into(), // TODO
            unit_of_measurement: unit.to_owned(),
            accuracy_decimals,
            force_update: false,
            device_class: device_class.to_owned(),
            state_class: 1,            // SensorStateClass::StateClassMeasurement as i32 // TODO
            legacy_last_reset_type: 0, // SensorLastResetType::LastResetNone as i32      // TODO
            disabled_by_default: false,
            entity_category: category as i32, // EntityCategory::None as i32 // TODO
          })
          .into(),
        );
      },
      EntityType::BinarySensor { category } => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesBinarySensorResponse(ListEntitiesBinarySensorResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "".into(),         // TODO
            device_class: "".into(), // TODO
            is_status_binary_sensor: false,
            disabled_by_default: false,
            entity_category: category as i32,
          })
          .into(),
        );
      },
      EntityType::Switch => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesSwitchResponse(ListEntitiesSwitchResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "".into(),         // TODO
            device_class: "".into(), // TODO
            disabled_by_default: false,
            entity_category: EntityCategory::Config as i32,
            assumed_state: false,
          })
          .into(),
        );
      },
      EntityType::Date => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesDateResponse(ListEntitiesDateResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "mdi:calendar".into(), // TODO
            disabled_by_default: false,
            entity_category: EntityCategory::Config as i32,
          })
          .into(),
        );
      },
      EntityType::Select { category } => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesSelectResponse(ListEntitiesSelectResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "".into(), // TODO
            disabled_by_default: false,
            entity_category: category as i32,
            options: {
              let mapping = commands[command_name].mapping().unwrap();
              mapping
                .entries()
                .map(|(&key, &value)| (key, value.to_owned()))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect()
            },
          })
          .into(),
        );
      },
      EntityType::DateTime { category } => {
        entity_map.insert(
          command_name,
          ProtoMessage::ListEntitiesDateTimeResponse(ListEntitiesDateTimeResponse {
            device_id,
            object_id: entity_id,
            key,
            name,
            icon: "mdi:calendar-clock".into(),
            disabled_by_default: false,
            entity_category: category as i32,
          })
          .into(),
        );
      },
      EntityType::TextSensor { category } => {
        let command = &commands[command_name];

        if let Some(block_count) = command.block_count() {
          entity_map.insert(
            command_name,
            MultiEntity::Multiple(
              (0..block_count)
                .into_iter()
                .flat_map(|i| {
                  let mut entities = Vec::new();

                  if command.data_type() == DataType::Error {
                    entities.push(ProtoMessage::ListEntitiesDateTimeResponse(ListEntitiesDateTimeResponse {
                      device_id,
                      object_id: format!("{entity_id}_{i}"), // TODO
                      key,
                      name: format!("{name} {i} Time"),
                      icon: "".into(), // TODO
                      disabled_by_default: false,
                      entity_category: category as i32,
                    }));

                    key += 1;
                  }

                  entities.push(ProtoMessage::ListEntitiesTextSensorResponse(ListEntitiesTextSensorResponse {
                    device_id,
                    object_id: format!("{entity_id}_{i}"), // TODO
                    key,
                    name: format!("{name} {i} Message"),
                    icon: "".into(),         // TODO
                    device_class: "".into(), // TODO
                    disabled_by_default: false,
                    entity_category: category as i32,
                  }));
                  key += 1;

                  entities
                })
                .collect(),
            ),
          );

          // `key` already incremented.
          continue;
        } else {
          entity_map.insert(
            command_name,
            ProtoMessage::ListEntitiesTextSensorResponse(ListEntitiesTextSensorResponse {
              device_id,
              object_id: entity_id,
              key,
              name,
              icon: "".into(),         // TODO
              device_class: "".into(), // TODO
              disabled_by_default: false,
              entity_category: category as i32,
            })
            .into(),
          );
        }
      },
    };

    key += 1;
  }

  entity_map
}
