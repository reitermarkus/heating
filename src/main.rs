use std::sync::{mpsc::channel, Arc, Mutex};
use std::thread;

use hc_sr04::HcSr04;
use medianheap::MedianHeap;
use measurements::Length;
use rppal::gpio::Gpio;
use simple_signal::{self, Signal};
use vessel::{CuboidTank, Tank};

const TRIGGER_PIN: u8 = 17;
const ECHO_PIN:    u8 = 18;

fn main() {
  let (sig_tx, sig_rx) = channel();

  simple_signal::set_handler(&[Signal::Int], move |_| {
    sig_tx.send(true).unwrap();
  });

  let heap = Arc::new(Mutex::new(MedianHeap::with_max_size(10000)));

  thread::spawn(move || {
    let gpio = Gpio::new().expect("failed to access GPIO");
    let trigger = gpio.get(TRIGGER_PIN).unwrap().into_output();
    let echo = gpio.get(ECHO_PIN).unwrap().into_input();
    let mut sensor = HcSr04::new(trigger, echo).expect("failed to set up sensor");

    let tank = CuboidTank::new(Length::from_centimeters(298.0), Length::from_centimeters(148.0), Length::from_centimeters(150.0));
    let sensor_offset = Length::from_centimeters(4.0);

    let heap = heap.clone();

    loop {
      if sig_rx.try_recv().unwrap_or(false) {
        break
      }

      if let Ok(distance) = sensor.measure() {
        let mut heap = heap.lock().unwrap();
        heap.push(distance);

        let buffer = 100;

        if heap.len() < buffer {
          println!("Waiting for first 1000 measurements: {:>#4}/{}", heap.len(), buffer);
        } else {
          let median_distance = Length::from_millimeters((heap.median().unwrap() * 1000.0).round()) - sensor_offset;

          let filled_height = tank.height() - median_distance;

          let level = tank.level(filled_height);

          println!("Median1: {}", median_distance);
          println!("Tank: {} l ({} %)", level.volume().as_liters(), level.percentage() * 100.0)
        }
      }
    }
  }).join().unwrap();
}
