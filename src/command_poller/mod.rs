use std::{collections::HashMap, mem, sync::Arc};

use itertools::Itertools;
use rangemap::RangeMap;
use tokio::sync::broadcast::{self, Receiver, error::SendError};

use vcontrol::{Command, VControl, Value};

pub async fn poll_thread(
  vcontrol: VControl,
) -> (
  Arc<tokio::sync::RwLock<tokio::sync::Mutex<VControl>>>,
  Receiver<(&'static str, Value)>,
  impl Future<Output = ()>,
  HashMap<&'static str, &'static Command>,
) {
  let mut commands = HashMap::<&'static str, &'static Command>::new();

  for (command_name, command) in vcontrol::commands::system_commands() {
    commands.insert(command_name, command);
  }

  for (command_name, command) in vcontrol.device().commands() {
    commands.insert(command_name, command);
  }

  let vcontrol = Arc::new(tokio::sync::RwLock::new(tokio::sync::Mutex::new(vcontrol)));

  let mut commands_sorted = commands
    .iter()
    .filter(|(_, command)| command.access_mode().is_read())
    .map(|(&k, &v)| (k, v))
    .collect::<Vec<(&'static str, &'static Command)>>();
  commands_sorted.sort_by_key(|(_, command)| (command.addr(), command.block_len()));

  let mut command_ranges = RangeMap::new();
  let mut current_range = None;

  const MAX_BLOCK_LEN: usize = 119;

  for (command_name, command) in &commands_sorted {
    let addr = command.addr();
    let block_len = command.block_len() as u16;

    let (range, range_commands) =
      current_range.get_or_insert_with(|| (addr..(addr + block_len), vec![(*command_name, *command)]));

    let combined_len = (addr + block_len) - range.start;

    if combined_len as usize <= MAX_BLOCK_LEN {
      range.end = range.end.max(addr + block_len);
      range_commands.push((*command_name, *command));
    } else {
      let range = mem::replace(range, addr..(addr + block_len));
      let range_commands = mem::replace(range_commands, vec![(*command_name, *command)]);
      command_ranges.insert(range, range_commands);
    }
  }
  if let Some((range, commands)) = current_range.take() {
    command_ranges.insert(range, commands);
  }

  eprintln!("commands: {}, command_ranges: {}", commands_sorted.len(), command_ranges.len());
  for command_range in command_ranges.iter().map(|(range, _)| range) {
    eprintln!("{command_range:#04X?} {command_range:#05?}");
  }

  let range_lengths = command_ranges.iter().map(|(range, _)| range).counts_by(|range| range.end - range.start);
  dbg!(range_lengths);

  let (tx, rx) = broadcast::channel((MAX_BLOCK_LEN * 2).next_power_of_two());

  let vcontrol_clone = vcontrol.clone();
  let poll_thread = async move {
    log::info!("Starting poll thread.");

    let vcontrol = vcontrol_clone;

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

          match tx.send((*command_name, value)) {
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

  (vcontrol, rx, poll_thread, commands)
}
