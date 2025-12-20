use esphome_native_api::proto::version_2025_6_3::EntityCategory;

pub enum EntityType {
  Number { step: f32 },
  Sensor { accuracy_decimals: i32, category: EntityCategory },
  BinarySensor { category: EntityCategory },
  TextSensor { category: EntityCategory },
  DateTime { category: EntityCategory },
  Select { category: EntityCategory },
  Switch,
  Date,
}

pub struct Entity {
  pub entity_name: &'static str,
  pub entity_type: EntityType,
}

impl Entity {
  pub fn category(&self) -> EntityCategory {
    match self.entity_type {
      EntityType::Number { .. } => EntityCategory::Config,
      EntityType::Sensor { category, .. } => category,
      EntityType::BinarySensor { category } => category,
      EntityType::TextSensor { category } => category,
      EntityType::DateTime { category } => category,
      EntityType::Select { category } => category,
      EntityType::Switch => EntityCategory::Config,
      EntityType::Date => EntityCategory::Config,
    }
  }
}
