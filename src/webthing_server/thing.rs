use std::{
  collections::HashMap,
  sync::{Arc, RwLock, Weak},
};

use schemars::{schema, schema_for};
use serde_json::json;
use tokio::sync::oneshot;
use webthing::{BaseProperty, BaseThing, Thing, property::ValueForwarder};

use vcontrol::{
  AccessMode, Command, DataType, Device, VControl, Value,
  types::{CircuitTimes, Date, DateTime, DeviceId, DeviceIdF0, Error},
};

struct VcontrolValueForwarder {
  command_name: &'static str,
  command: &'static Command,
  vcontrol: Weak<tokio::sync::Mutex<VControl>>,
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
    let vcontrol = self.vcontrol.upgrade().ok_or("Device connection closed.")?;
    let command_name = self.command_name;

    let (tx, rx) = oneshot::channel();

    let arbiter = actix_rt::Arbiter::new();
    arbiter.spawn(async move {
      let mut vcontrol = vcontrol.lock().await;
      let res = vcontrol.set(command_name, vcontrol_value).await;
      tx.send(res).unwrap();
    });

    let res = rx.blocking_recv().unwrap();
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
  vcontrol: Weak<tokio::sync::Mutex<VControl>>,
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

  let value_forwarder = VcontrolValueForwarder { command_name, command, vcontrol: vcontrol };

  thing.add_property(Box::new(BaseProperty::new(
    command_name.to_string(),
    json!(null),
    Some(Box::new(value_forwarder)),
    Some(description),
  )));
}

pub async fn make_thing(
  vcontrol: Arc<tokio::sync::Mutex<VControl>>,
  commands: HashMap<&'static str, &'static Command>,
) -> Arc<RwLock<Box<dyn Thing + 'static>>> {
  // TODO: Get from `vcontrol`.
  let device_id = 1234;

  let device = vcontrol.lock().await.device();

  let mut thing = BaseThing::new(
    format!("urn:dev:ops:heating-{}", device_id),
    device.name().to_owned(),
    Some(vec!["ObjectProperty".to_owned()]),
    None,
  );

  for (command_name, command) in &commands {
    add_command(&mut thing, Arc::downgrade(&vcontrol), &device, command_name, command);
  }

  let thing: Box<dyn Thing + 'static> = Box::new(thing);
  let thing = Arc::new(RwLock::new(thing));

  thing
}
