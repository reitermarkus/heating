#![feature(proc_macro_hygiene, decl_macro)]

use std::env;
use std::io::Read;
use std::sync::{mpsc::channel, Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use fc113::Fc113;
use hc_sr04::HcSr04;
use lazy_static::lazy_static;
use lru_time_cache::LruCache;
use medianheap::MedianHeap;
use measurements::Length;
use ordered_float::NotNan;
use rocket_contrib::json::{Json};
use rocket::{self, get, post, routes, State, Request, Data, Outcome::*, data::{self, FromDataSimple}, http::Status};
use rppal::gpio::Gpio;
use rppal::i2c::I2c;
use serde_json::json;
use simple_signal::{self, Signal};
use vcontrol::{self, Optolink, VControl, Device, device::V200KW2, Value};
use vessel::{CuboidTank, Tank};

mod lcd;
use self::lcd::{update_lcd, Symbol::*};

const TRIGGER_PIN: u8 = 17;
const ECHO_PIN:    u8 = 18;

const CACHE_DURATION: Duration = Duration::from_secs(60);

lazy_static! {
  static ref TANK: CuboidTank = CuboidTank::new(
    Length::from_centimeters(298.0),
    Length::from_centimeters(148.0),
    Length::from_centimeters(150.0),
  );

  static ref SENSOR_OFFSET: Length = Length::from_centimeters(4.0);
}

fn tank_level(median: NotNan<f64>) -> (f64, f64, f64) {
  let millimeters = (median * 1000.0).round();

  let mut distance = Length::from_millimeters(millimeters) - *SENSOR_OFFSET;

  if distance < Length::from_meters(0.0) {
    distance = Length::from_meters(0.0)
  } else if distance > TANK.height() {
    distance = TANK.height()
  }

  let filled_height = TANK.height() - distance;
  let level = TANK.level(filled_height);

  (filled_height.as_centimeters(), level.volume().as_liters(), level.percentage() * 100.0)
}

#[get("/oiltank")]
fn oiltank(heap: State<Arc<RwLock<MedianHeap<NotNan<f64>>>>>) -> Option<Json<serde_json::value::Value>> {
  heap.read().unwrap().median().map(|median| {
    let (height, volume, percentage) = tank_level(median);

    Json(json!({
      "fill_height": height,
      "volume": volume,
      "percentage": percentage,
    }))
  })
}

#[get("/vcontrol/commands")]
fn vcontrol_commands(commands: State<Vec<&'static str>>) -> Option<Json<Vec<&'static str>>> {
  if commands.is_empty() {
    return None
  }

  Some(Json(commands.to_vec()))
}

#[get("/vcontrol/<command>")]
fn vcontrol_get(command: String, vcontrol: State<Mutex<VControl<V200KW2>>>, cache: State<RwLock<LruCache<String, Value>>>) -> Option<Result<Json<Value>, vcontrol::Error>> {
  if let Some(value) = cache.read().unwrap().peek(&command) {
    eprintln!("Using cached value for command '{}': {:?}", command, value);
    return Some(Ok(Json(value.clone())))
  }

  let mut vcontrol = vcontrol.lock().unwrap();

  match vcontrol.get(&command) {
    Err(vcontrol::Error::UnsupportedCommand(_)) => None,
    Ok(value) => {
      eprintln!("Getting fresh value for command '{}': {:?}", command, value);

      let mut cache = cache.write().unwrap();
      cache.insert(command, value.clone());

      Some(Ok(Json(value)))
    },
    Err(err) => Some(Err(err))
  }
}

struct DataValue(Value);

impl FromDataSimple for DataValue {
  type Error = String;

  fn from_data(req: &Request, data: Data) -> data::Outcome<Self, String> {
    let mut string = String::new();

    if let Err(e) = data.open().read_to_string(&mut string) {
      return Failure((Status::InternalServerError, format!("{:?}", e)));
    }

    Success(DataValue(string.parse::<Value>().unwrap()))
  }
}

#[post("/vcontrol/<command>", format = "plain", data = "<value>")]
fn vcontrol_set_text(command: String, value: DataValue, vcontrol: State<Mutex<VControl<V200KW2>>>, cache: State<RwLock<LruCache<String, Value>>>) -> Option<Result<(), vcontrol::Error>> {
  vcontrol_set(command, value.0, vcontrol, cache)
}

#[post("/vcontrol/<command>", format = "json", data = "<value>")]
fn vcontrol_set_json(command: String, value: Json<Value>, vcontrol: State<Mutex<VControl<V200KW2>>>, cache: State<RwLock<LruCache<String, Value>>>) -> Option<Result<(), vcontrol::Error>> {
  vcontrol_set(command, value.0, vcontrol, cache)
}

fn vcontrol_set(command: String, value: Value, vcontrol: State<Mutex<VControl<V200KW2>>>, cache: State<RwLock<LruCache<String, Value>>>) -> Option<Result<(), vcontrol::Error>> {
  let mut vcontrol = vcontrol.lock().unwrap();

  match vcontrol.set(&command, &value) {
    Err(vcontrol::Error::UnsupportedCommand(_)) => None,
    res => {
      let mut cache = cache.write().unwrap();
      cache.remove(&command);
      Some(res)
    }
  }
}

fn main() {
  let commands = V200KW2::commands();

  let device = Optolink::open(env::var("OPTOLINK_DEVICE").unwrap()).expect("Failed to open Optolink device");
  let vcontrol = Mutex::new(VControl::<V200KW2>::new(device));

  let vcontrol_cache = RwLock::new(LruCache::<String, Value>::with_expiry_duration(CACHE_DURATION));

  let heap = Arc::new(RwLock::new(MedianHeap::with_max_size(10000)));
  let heap_clone = heap.clone();

  let gpio = Gpio::new().expect("failed to access GPIO");
  let trigger = gpio.get(TRIGGER_PIN).unwrap().into_output();
  let echo = gpio.get(ECHO_PIN).unwrap().into_input();

  let mut sensor = HcSr04::new(trigger, echo).expect("failed to set up sensor");

  let i2c = I2c::new().expect("failed to access I2C bus");
  let mut lcd = Fc113::new(i2c, 2).unwrap();

  lcd.create_char(Droplet      as usize, Droplet).unwrap();
  lcd.create_char(OeLowercase  as usize, OeLowercase).unwrap();
  lcd.create_char(Liter        as usize, Liter).unwrap();
  lcd.create_char(PercentLeft  as usize, PercentLeft).unwrap();
  lcd.create_char(PercentRight as usize, PercentRight).unwrap();

  update_lcd(&mut lcd, 0.0, 0.0).unwrap();

  thread::spawn(move || {
    let (sig_tx, sig_rx) = channel();

    simple_signal::set_handler(&[Signal::Int], move |_| {
      sig_tx.send(true).unwrap();
    });

    let heap = heap_clone;

    let mut i = 0;

    loop {
      if sig_rx.try_recv().unwrap_or(false) {
        break
      }

      if let Ok(distance) = sensor.measure() {
        i = (i + 1) % 100;

        let mut heap = heap.write().unwrap();
        heap.push(distance);

        if i == 0 {
          if let Some(median) = heap.median() {
            let (height, volume, percentage) = tank_level(median);

            println!("Updating LCD …");
            update_lcd(&mut lcd, volume, percentage).unwrap();
          }
        }
      }
    }

    std::process::exit(1);
  });

  rocket::ignite()
    .manage(vcontrol)
    .manage(commands)
    .manage(vcontrol_cache)
    .manage(heap.clone())
    .mount("/", routes![oiltank, vcontrol_commands, vcontrol_get, vcontrol_set_json, vcontrol_set_text])
    .launch();
}
