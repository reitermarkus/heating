require 'securerandom'
require 'shellwords'

TARGET = ENV['TARGET'] || 'arm-unknown-linux-gnueabihf'

RPI = ENV['RPI'] || 'heizung.local'
HOST = "pi@#{RPI}"

def ssh(*args)
  sh 'ssh', HOST, *args
end

desc 'compile binary'
task :build do
  sh 'cross', 'build', '--release', '--target', TARGET
end

desc 'set time zone on Raspberry Pi'
task :setup_timezone do
  sh 'ssh', HOST, 'sudo', 'timedatectl', 'set-timezone', 'Europe/Vienna'
end

desc 'set hostname on Raspberry Pi'
task :setup_hostname do
  sh 'ssh', HOST, <<~SH
    if ! dpkg -s dnsutils >/dev/null; then
      sudo apt-get update
      sudo apt-get install -y dnsutils
    fi

    hostname="$(dig -4 +short -x "$(hostname -I | awk '{print $1}')")"
    hostname="${hostname%%.local.}"

    if [ -n "${hostname}" ]; then
      echo "${hostname}" | sudo tee /etc/hostname >/dev/null
    fi
  SH
end

desc 'set up watchdog on Raspberry Pi'
task :setup_watchdog do
  sh 'ssh', HOST, <<~SH
    if ! dpkg -s watchdog >/dev/null; then
      sudo apt-get update
      sudo apt-get install -y watchdog
    fi
  SH

  r, w = IO.pipe

  w.puts 'bcm2835_wdt'
  w.close

  sh 'ssh', HOST, 'sudo', 'tee', '/etc/modules-load.d/bcm2835_wdt.conf', in: r

  gateway_ip = %x(#{['ssh', HOST, 'ip', 'route'].shelljoin})[/via (\d+.\d+.\d+.\d+) /, 1]

  raise if gateway_ip.empty?

  r, w = IO.pipe

  w.puts <<~CFG
    watchdog-device	= /dev/watchdog
    ping = #{gateway_ip}
  CFG
  w.close

  sh 'ssh', HOST, 'sudo', 'tee', '/etc/watchdog.conf', in: r
  sh 'ssh', HOST, 'sudo', 'systemctl', 'enable', 'watchdog'
end

desc 'set up device symlink for Optolink serial port'
task :setup_optolink do
  r, w = IO.pipe

  w.puts <<~CFG
    SUBSYSTEM=="tty", ATTRS{idVendor}=="0403", ATTRS{idProduct}=="6001", ATTRS{serial}=="A902YK66", SYMLINK+="optolink", TAG+="systemd"
  CFG
  w.close

  sh 'ssh', HOST, 'sudo', 'tee', '/lib/udev/rules.d/99-optolink.rules', in: r
end

task :setup => [:setup_timezone, :setup_hostname, :setup_optolink, :setup_watchdog]

desc 'deploy binary and service configuration to Raspberry Pi'
task :deploy => :build  do
  sh 'rsync', '-z', '--rsync-path', 'sudo rsync', "target/#{TARGET}/release/heating", "#{HOST}:/usr/local/bin/heating"

  r, w = IO.pipe

  w.puts <<~CFG
    [Unit]
    Description=heating
    BindsTo=dev-optolink.device
    After=dev-optolink.device
    StartLimitAction=reboot
    StartLimitIntervalSec=60
    StartLimitBurst=10

    [Service]
    Type=simple
    Environment=OPTOLINK_DEVICE=/dev/optolink
    Environment=RUST_LOG=info
    Environment=ROCKET_PORT=80
    Environment=ROCKET_SECRET_KEY="#{SecureRandom.base64(32)}"
    ExecStart=/usr/local/bin/heating
    Restart=always
    RestartSec=1

    [Install]
    WantedBy=multi-user.target
  CFG
  w.close

  sh 'ssh', HOST, 'sudo', 'tee', '/etc/systemd/system/heating.service', in: r
  sh 'ssh', HOST, 'sudo', 'systemctl', 'enable', 'heating'
  sh 'ssh', HOST, 'sudo', 'systemctl', 'restart', 'heating'
end

desc 'show service log'
task :log do
  sh 'ssh', HOST, '-t', 'journalctl', '-f', '-u', 'heating'
end

task :default => :build
