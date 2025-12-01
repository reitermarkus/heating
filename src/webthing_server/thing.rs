use std::{
  collections::HashMap,
  mem,
  ops::Range,
  sync::{Arc, RwLock, Weak, mpsc::channel},
};

use itertools::Itertools;
use rangemap::RangeMap;
use schemars::{schema, schema_for};
use serde_json::json;
use tokio::sync::broadcast::{
  self,
  error::{RecvError, SendError},
};
use webthing::{BaseProperty, BaseThing, Thing, property::ValueForwarder};

use vcontrol::{
  AccessMode, Command, DataType, Device, VControl, Value,
  types::{CircuitTimes, Date, DateTime, DeviceId, DeviceIdF0, Error},
};

struct VcontrolValueForwarder {
  command_name: &'static str,
  command: &'static Command,
  vcontrol: Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
}

impl ValueForwarder for VcontrolValueForwarder {
  fn set_value(&mut self, value: serde_json::Value) -> Result<serde_json::Value, &'static str> {
    let new_value = value.clone();

    let vcontrol_value = match self.command.data_type() {
      DataType::DeviceId => serde_json::from_value::<DeviceId>(value).map(Value::DeviceId),
      DataType::DeviceIdF0 => serde_json::from_value::<DeviceIdF0>(value).map(Value::DeviceIdF0),
      DataType::Int => serde_json::from_value::<i64>(value).map(Value::Int),
      DataType::Double => serde_json::from_value::<f64>(value).map(Value::Double),
      DataType::Byte | DataType::ErrorIndex => serde_json::from_value::<u8>(value).map(|b| Value::Int(b as i64)),
      DataType::String => serde_json::from_value::<String>(value).map(Value::String),
      DataType::Date => serde_json::from_value::<Date>(value).map(Value::Date),
      DataType::DateTime => serde_json::from_value::<DateTime>(value).map(Value::DateTime),
      DataType::Error => serde_json::from_value::<Error>(value).map(Value::Error),
      DataType::CircuitTimes => serde_json::from_value::<CircuitTimes>(value).map(Box::new).map(Value::CircuitTimes),
      DataType::ByteArray => serde_json::from_value::<Vec<u8>>(value).map(Value::ByteArray),
    };

    let vcontrol_value = match vcontrol_value {
      Ok(vcontrol_value) => vcontrol_value,
      Err(err) => {
        log::error!("Failed setting property {}: {}", self.command_name, err);
        return Err("Parsing value failed");
      },
    };

    log::info!("Setting property {} to {}.", self.command_name, new_value);
    let vcontrol = self.vcontrol.clone();
    let command_name = self.command_name;

    let (tx, rx) = channel();

    let arbiter = actix_rt::Arbiter::new();
    arbiter.spawn(async move {
      let vcontrol = vcontrol.write().await;
      let mut vcontrol = vcontrol.lock().await;
      let res = vcontrol.set(command_name, vcontrol_value).await;
      tx.send(res).unwrap();
    });

    let res = rx.recv().unwrap();
    arbiter.stop();

    match res {
      Ok(_) => {
        log::info!("Property {} successfully set to {}.", command_name, new_value);
        Ok(new_value)
      },
      Err(err) => {
        log::error!("Failed setting property {}: {}", command_name, err);
        Err("Failed setting value")
      },
    }
  }
}

fn add_command(
  thing: &mut dyn Thing,
  vcontrol: Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
  device: &Device,
  command_name: &'static str,
  command: &'static Command,
) {
  let mut root_schema = match command.data_type() {
    DataType::DeviceId => schema_for!(DeviceId),
    DataType::DeviceIdF0 => schema_for!(DeviceIdF0),
    DataType::Int => schema_for!(i64),
    DataType::Double => schema_for!(f64),
    DataType::Byte | DataType::ErrorIndex => schema_for!(u8),
    DataType::String => schema_for!(String),
    DataType::Date => schema_for!(Date),
    DataType::DateTime => schema_for!(DateTime),
    DataType::Error => schema_for!(Error),
    DataType::CircuitTimes => schema_for!(CircuitTimes),
    DataType::ByteArray => schema_for!(Vec<u8>),
  };

  match command.access_mode() {
    AccessMode::Read => {
      root_schema.schema.metadata().read_only = true;
    },
    AccessMode::Write => {
      root_schema.schema.metadata().write_only = true;
    },
    AccessMode::ReadWrite => (),
  }

  root_schema.schema.extensions.insert("@type".into(), json!("LevelProperty"));

  if let Some(unit) = &command.unit() {
    root_schema.schema.extensions.insert("unit".into(), json!(unit));
  }

  let create_enum = |enum_schema: &mut schema::SchemaObject, mapping: &'static phf::Map<i32, &'static str>| {
    // Use `oneOf` schema in order to add description for enum values.
    // https://github.com/json-schema-org/json-schema-spec/issues/57#issuecomment-815166515
    let subschemas = mapping
      .entries()
      .map(|(k, v)| {
        schema::SchemaObject {
          const_value: Some(json!(k)),
          metadata: Some(Box::new(schemars::schema::Metadata {
            description: Some(v.to_string()),
            ..Default::default()
          })),
          ..Default::default()
        }
        .into()
      })
      .collect();

    enum_schema.subschemas =
      Some(Box::new(schemars::schema::SubschemaValidation { one_of: Some(subschemas), ..Default::default() }));
  };

  if let Some(mapping) = command.mapping() {
    create_enum(&mut root_schema.schema, mapping);
  } else if command.data_type() == DataType::ErrorIndex {
    create_enum(&mut root_schema.schema, device.errors());
  } else if command.data_type() == DataType::Error {
    if let Some(ref mut validation) = root_schema.schema.object {
      if let Some(schema::Schema::Object(index_schema)) = validation.properties.get_mut("index") {
        create_enum(index_schema, device.errors());
      }
    }
  };

  let schema = serde_json::to_value(root_schema).unwrap().as_object().unwrap().clone();
  let description = schema;

  let value_forwarder = VcontrolValueForwarder { command_name, command, vcontrol: vcontrol.clone() };

  thing.add_property(Box::new(BaseProperty::new(
    command_name.to_string(),
    json!(null),
    Some(Box::new(value_forwarder)),
    Some(description),
  )));
}

pub fn make_thing(
  vcontrol: VControl,
) -> (
  Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
  Arc<RwLock<Box<dyn Thing + 'static>>>,
  HashMap<&'static str, &'static Command>,
) {
  let device = vcontrol.device();
  let mut commands = HashMap::<&'static str, &'static Command>::new();

  for (command_name, command) in vcontrol::commands::system_commands() {
    commands.insert(command_name, command);
  }

  for (command_name, command) in device.commands() {
    commands.insert(command_name, command);
  }

  let vcontrol = Arc::new(tokio::sync::RwLock::new(tokio::sync::Mutex::new(vcontrol)));

  // TODO: Get from `vcontrol`.
  let device_id = 1234;

  let mut thing = BaseThing::new(
    format!("urn:dev:ops:heating-{}", device_id),
    device.name().to_owned(),
    Some(vec!["ObjectProperty".to_owned()]),
    None,
  );

  for (command_name, command) in &commands {
    add_command(&mut thing, vcontrol.clone(), &device, command_name, command);
  }

  let thing: Box<dyn Thing + 'static> = Box::new(thing);
  let thing = Arc::new(RwLock::new(thing));

  (vcontrol, thing, commands)
}

pub async fn update_thread(
  vcontrol: Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
  weak_thing: Weak<RwLock<Box<dyn Thing + 'static>>>,
  commands: HashMap<&'static str, &'static Command>,
) {
  let mut commands = commands
    .into_iter()
    .filter(|(_, command)| command.access_mode().is_read())
    .collect::<Vec<(&'static str, &'static Command)>>();

  commands.sort_by_key(|(_, command)| (command.addr(), command.block_len()));

  let mut command_ranges = RangeMap::new();
  let mut current_range = None;

  const MAX_BLOCK_LEN: u16 = 119;

  for (command_name, command) in &commands {
    let addr = command.addr();
    let block_len = command.block_len() as u16;

    let (range, commands) =
      current_range.get_or_insert_with(|| (addr..(addr + block_len), vec![(*command_name, *command)]));

    let combined_len = (addr + block_len) - range.start;

    if combined_len <= MAX_BLOCK_LEN {
      range.end = range.end.max(addr + block_len);
      commands.push((*command_name, *command));
    } else {
      let range = mem::replace(range, addr..(addr + block_len));
      let commands = mem::replace(commands, vec![(*command_name, *command)]);
      command_ranges.insert(range, commands);
    }
  }
  if let Some((range, commands)) = current_range.take() {
    command_ranges.insert(range, commands);
  }

  eprintln!("commands: {}, command_ranges: {}", commands.len(), command_ranges.len());
  for command_range in command_ranges.iter().map(|(range, _)| range) {
    eprintln!("{command_range:#04X?} {command_range:#05?}");
  }

  let range_lengths = command_ranges.iter().map(|(range, _)| range).counts_by(|range| range.end - range.start);
  dbg!(range_lengths);

  let (tx, mut rx) = broadcast::channel(96);

  let poll_thread = async {
    'outer: loop {
      for (range, commands) in command_ranges.iter() {
        let vcontrol = vcontrol.read().await;
        let mut vcontrol = vcontrol.lock().await;

        let protocol = vcontrol.protocol();
        let mut buffer = vec![0; (range.end - range.start) as usize];
        protocol.get(vcontrol.optolink(), range.start, &mut buffer).await.unwrap();

        let start_addr = commands[0].1.addr();

        for (command_name, command) in commands {
          let addr = command.addr();
          let block_len = command.block_len() as usize;

          let start = (addr - start_addr) as usize;

          let bytes = &buffer[start..(start + block_len)];

          let value = match command.deserialize(bytes) {
            Ok(value) => value,
            Err(err) => {
              log::error!("Failed to deserialize value for command {command_name}: {}", err);
              continue;
            },
          };

          match tx.send((command_name, value)) {
            Ok(_receivers) => continue,
            Err(SendError((command_name, value))) => {
              log::error!("Failed to send value for command {command_name}: {value:?}");
              break 'outer;
            },
          }
        }
      }
    }
  };

  let update_thread = async {
    loop {
      let recv_res = match rx.recv().await {
        Ok(res) => res,
        Err(RecvError::Closed) => break,
        Err(RecvError::Lagged(n)) => {
          log::warn!("Receiver lagged, {n} messages skipped.");
          continue;
        },
      };

      let thing = if let Some(thing) = weak_thing.upgrade() { thing } else { return };

      let (command_name, new_value) = recv_res;

      // let new_value = {
      //   let vcontrol = vcontrol.read().await;
      //   let mut vcontrol = vcontrol.lock().await;
      //   match vcontrol.get(command_name).await {
      //     Ok(value) => Some(value),
      //     Err(err) => {
      //       log::error!("Failed getting value for property '{}': {}", command_name, err);
      //       None
      //     },
      //   }
      // };

      let thing = thing.clone();
      let command_name = command_name.to_owned();
      actix_rt::task::spawn_blocking(move || {
        let mut t = thing.write().unwrap();
        let prop = t.find_property(command_name).unwrap();

        let old_value = prop.get_value();

        let new_value = if let Some(value) = Some(new_value) {
          // OutputValue
          let value = value; // value.value
          let mapping: Option<&'static phf::map::Map<i32, &'static str>> = None; // value.mapping

          let new_value = json!(value);

          if old_value != new_value {
            if let Some(mapping) = mapping {
              let mapping_found = match value {
                Value::Int(value) => {
                  if let Ok(value) = i32::try_from(value) {
                    mapping.contains_key(&value)
                  } else {
                    false
                  }
                },
                _ => false,
              };

              if !mapping_found {
                log::warn!("Property '{}' does not have an enum mapping for {:?}.", command_name, value);
              }
            }
          }

          new_value
        } else {
          json!(null)
        };

        if let Err(err) = prop.set_cached_value(new_value.clone()) {
          log::error!("Failed setting cached value for property '{}': {}", command_name, err)
        }

        if old_value != new_value {
          log::info!("Property '{}' changed from {} to {}.", command_name, old_value, new_value);
        }

        t.property_notify(command_name.to_string(), new_value);
      })
      .await
      .unwrap();
    }
  };

  tokio::select! {
    _ = poll_thread => (),
    _ = update_thread => (),
  }
}
