use anyhow::{anyhow, Result};
use core::str;
use embedded_svc::{http::Method, http::Headers, io::Write};
use esp_idf_hal::{
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
    gpio::OutputPin,
    modem::Modem,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::{Configuration, EspHttpServer},
    tls::X509,
    netif::EspNetif,
    wifi::{BlockingWifi, EspWifi},
    timer::EspTaskTimerService,
    nvs::EspDefaultNvsPartition,
};
use esp_idf_sys::esp_timer_get_time;
use std::{
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
    collections::HashMap,
    cell::RefCell,
    time::Instant,
};
use embedded_svc::{
	io::Read,
};
use embedded_svc::wifi::{
    AccessPointConfiguration, AuthMethod, Configuration as WifiConfiguration,
};
use stepgen::Stepgen;

// If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_sys as _;

pub mod interrupts;
pub mod planner;
pub mod serial;
pub mod pins;
pub mod commandbuffer;
pub mod wifi;

use crate::planner::Planner;
use crate::interrupts::Stepper;

const BLOCK_BUFFER_SIZE: usize = 20;
const WIFI_SSID: &str = "Alumina";
const WIFI_PSK: &str = "";

pub struct Movement {
    vectors: Vec<i32>, // Reduced vectors for each axis
    cycles: i32,       // Number of cycles to repeat
}

pub struct MotionPlanner {
    movements: Vec<Movement>,
    // Other fields as needed
}

pub enum MotorType {
    Stepper { step_angle: f32, gear_ratio: f32 },
    Servo { max_speed: f32, torque: f32 },
}

pub enum ToolType {
    Extruder { temp_sensor: TemperatureSensor, material: String },
    Laser { power: f32, max_power: f32, min_power: f32, pulse_duration: u32, pulses_per_pm: f32, mode: String},
    PlasmaCutter { power: f32, max_power: f32, min_power: f32, pierce_delay: f32, cut_height: f32, hop_distance: f32, gas_type: String },
    Mill { spindle_speed: f32, max_spindle_speed: f32, min_spindle_speed: f32, direction: String, material: String },
    Heater { temp_sensor: TemperatureSensor, temperature: f32, max_temp: f32, min_temp: f32 },
}

pub struct Axis {
    motor_type: MotorType,
    endstops: HashMap<String, Endstop>,
    axis_position: f32, // Machine Coordinate
}

pub struct Endstop {
    start: bool,
    end: bool,
    pin: u32,
    inverted: bool,
    pullup: bool,
}

pub struct TemperatureSensor {
    current_temperature: f32,
    target_temperature: f32,
}

pub struct Camera {
    resolution: (u32, u32),
    fps: u32,
}

pub struct CNCMachineState {
    pub axes: HashMap<String, Axis>, // up to 12 axes
    pub toolheads: HashMap<String, ToolType>, // multiple toolheads
    pub workpiece_coordinates: (f32, f32, f32), // Workpiece coordinates
    pub sensors: HashMap<String, Sensor>,
    pub peripherals: HashMap<String, Peripheral>,
    pub pendant: Pendant,
    pub endstops: HashMap<String, Endstop>,
    pub fan_speed: f64,
    pub feedrate: f64,
    pub relative_mode: bool,
}

pub enum Sensor {
    ToolHeight { height: f32 },
    Temperature(TemperatureSensor),
    Camera(Camera),
    CoolantFlow { rate: f32 },
    AirFlow { speed: f32 },
}

pub enum Peripheral {
    Status(String),
    Activation(String),
    Deactivation(String),
}

pub struct Pendant {
    connected: bool,
    command: Option<PendantCommand>,
}

pub enum PendantCommand {
    Move { axis: String, distance: f32 },
    Stop,
    // other commands as needed
}

impl CNCMachineState {
    pub fn new() -> Self {
        Self {
            // Initialize state
            axes: HashMap::new(),
            toolheads: HashMap::new(),
            workpiece_coordinates: (0.0, 0.0, 0.0),
            sensors: HashMap::new(),
            peripherals: HashMap::new(),
            pendant: Pendant {
                connected: false,
                command: None,
            },
            endstops: HashMap::new(),
            fan_speed: 0.0,
            feedrate: 0.0,
            relative_mode: false,
        }
    }
}

impl MotionPlanner {
    pub fn new() -> Self {
        Self {
            movements: Vec::new(),
            // Initialize other fields
        }
    }

    pub fn plan_movement(&mut self, vectors: Vec<i32>) {
        // Calculate the greatest common divisor
        let gcd = vec_gcd(&vectors);

        // Divide each vector by the greatest common divisor to get reduced vectors
        //let reduced_vectors: Vec<i32> = vectors.into_iter().map(|v| v / gcd).collect();

        // Push the movement into the buffer
        //let movement = Movement {
        //    vectors: reduced_vectors,
        //    cycles: gcd,
        //};
        //self.movements.push(movement);
    }

    pub fn next_movement(&mut self) -> Option<Movement> {
        self.movements.pop()
    }
}

fn vec_gcd(numbers: &[i32]) -> i32 {
    // Function to calculate the greatest common divisor of the vectors
    //numbers.iter().fold(numbers[0], |acc, &x| gcd(acc, x))
    0
}

/// Start the radio as a Soft-AP and return the running `EspWifi`.
pub fn wifi_ap(
    ssid: &str,
    psk: &str,
    modem: Modem,
    sysloop: EspSystemEventLoop,
) -> Result<EspWifi<'static>> {
    // 1) Take default NVS and pass it to the Wi-Fi driver
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let ap_cfg = AccessPointConfiguration {
        ssid: ssid.try_into().map_err(|_| anyhow!("SSID too long (max 32)"))?,
        password: psk.try_into().map_err(|_| anyhow!("password too long (max 64)"))?,
        auth_method: if psk.is_empty() { AuthMethod::None } else { AuthMethod::WPA2Personal },
        ssid_hidden: false,
        channel: 1,
        max_connections: 4,
        ..Default::default()
    };

    wifi.set_configuration(&WifiConfiguration::AccessPoint(ap_cfg))?;
    wifi.start()?;

	// Wait up to 10s for SoftAP netif up
	let deadline = Instant::now() + Duration::from_secs(10);
	while !wifi.ap_netif().is_up()? {
		if Instant::now() >= deadline {
			anyhow::bail!("SoftAP netif did not come up");
		}
		std::thread::sleep(Duration::from_millis(100));
	}

    Ok(wifi)
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;

    // Connect to the Wi-Fi network
    let _wifi = wifi_ap(
		WIFI_SSID,
		WIFI_PSK,
		peripherals.modem,
		sysloop,
	)?;

    let planner = RefCell::new(Planner::new(BLOCK_BUFFER_SIZE));
    let mut stepper = Stepper::new();

    // Initialize temperature sensor
    // let sda = peripherals.pins.gpio10;
    // let scl = peripherals.pins.gpio8;
    // let i2c = peripherals.i2c0;
    // let config = I2cConfig::new().baudrate(100.kHz().into());
    // let i2c = I2cDriver::new(i2c, sda, scl, &config)?;

    // ===== ESP32-WROOM-32U SAFE PIN SELECTION =====
	// D0..D7 remapped to safe GPIOs on classic ESP32.
	// If you change wiring, update the numbers here.
	let d0_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio2)?));   // D0  -> GPIO2
	let d0 = d0_main.clone();

	let d1_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio23)?));  // D1  -> GPIO23 (relay)
	let d1 = d1_main.clone();

	let d3_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio4)?));   // D3  -> GPIO4
	let d3 = d3_main.clone();

	let d4_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio5)?));   // D4  -> GPIO5
	let d4 = d4_main.clone();

	let d5_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio16)?));  // D5  -> GPIO16
	let d5 = d5_main.clone();

	let d6_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio17)?));  // D6  -> GPIO17
	let d6 = d6_main.clone();

	let d7_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio18)?));  // D7  -> GPIO18
	let d7 = d7_main.clone();

	let d12_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio19)?)); // status LED -> GPIO19
	let d12 = d12_main.clone();
    //let d13_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio13)?));
    //let d13 = d13_main.clone();
    //let d14_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio14)?));
    //let d14 = d14_main.clone();
    //let d15_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio15)?));
    //let d15 = d15_main.clone();
    //let d16_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio16)?));
    //let d16 = d16_main.clone();
    //let d17_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio17)?));
    //let d17 = d17_main.clone();
    //let d18_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio18)?));
    //let d18 = d18_main.clone();
    //let d19_main = Arc::new(Mutex::new(esp_idf_hal::gpio::PinDriver::output(peripherals.pins.gpio19)?));
    //let d19 = d19_main.clone();

    let status_led = d12_main.clone();
    let relay = d1_main.clone();

    /*
    let temp_sensor_main = Arc::new(Mutex::new(shtc3(i2c)));
    let mut temp_sensor = temp_sensor_main.clone();
    temp_sensor
        .lock()
        .unwrap()
        .start_measurement(PowerMode::NormalMode)
        .unwrap();
     */

    // 1.Create a `EspHttpServer` instance using a default configuration
    let mut webserver_configuration: Configuration = Default::default();
    //webserver_configuration.server_certificate = Some(X509::pem(CStr::from_bytes_with_nul(include_bytes!("../public.pem")).unwrap()));
    //webserver_configuration.private_key = Some(X509::pem(CStr::from_bytes_with_nul(include_bytes!("../private.pem")).unwrap()));

    //let mut server = EspHttpServer::new(&Default::default())?;
    let mut server = EspHttpServer::new(&webserver_configuration)?;

    // 2. Write a handler that returns the index page
    server.fn_handler("/", Method::Get, |request| -> anyhow::Result<()> {  // User interface index.html
        let response = request.into_response(200, Some("OK"), &[("Content-Type", "text/html"), ("Content-Encoding", "text")]);
        response?.write_all(include_bytes!("../../alumina-ui/dist/index.html"))?;
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/alumina-ui.js", Method::Get, |request| -> anyhow::Result<()> {  // User interface index.js
        let response = request.into_response(200, Some("OK"), &[("Content-Type", "text/javascript; charset=utf-8"), ("Content-Encoding", "gzip")]);
        response?.write_all(include_bytes!("../../alumina-ui/dist/alumina-ui.js.gz"))?;
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/alumina-ui_bg.wasm", Method::Get, |request| -> anyhow::Result<()> {  // User interface wasm binary
        let response = request.into_response(200, Some("OK"), &[("Content-Type", "application/wasm"), ("Content-Encoding", "br")]);
        response?.write_all(include_bytes!("../../alumina-ui/dist/alumina-ui_bg.wasm.br"))?;
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/favicon.ico", Method::Get, |request| -> anyhow::Result<()> {  // User interface icon
        let response = request.into_response(200, Some("OK"), &[("Content-Type", "image/gif")]);
        response?.write_all(include_bytes!("../../alumina-ui/dist/favicon.ico"))?;
        Ok::<(), anyhow::Error>(())
    })?;

	server.fn_handler("/time", Method::Get, |request| -> anyhow::Result<()> {
		let us = unsafe { esp_timer_get_time() } as u64;
		let ms = us / 1000;
		// Return a plain number that the UI can parse easily:
		let body = format!("{}\n", ms);
		let mut resp = request.into_response(200, Some("OK"),
			&[("Content-Type","text/plain")])?;
		resp.write_all(body.as_bytes())?;
		resp.flush()?;
		Ok::<(), anyhow::Error>(())
	})?;

    server.fn_handler("/files", Method::Post, move |mut request| -> anyhow::Result<()> {
		let mut buf = [0u8; 2048];
		let mut data: Vec<u8> = Vec::new();
		loop {
			let n = request.read(&mut buf)?;
			if n == 0 { break; }
			data.extend_from_slice(&buf[..n]);
		}
		let mut resp = request.into_response(200, Some("OK"),
			&[("Content-Type","text/plain")])?;
		let body = format!("received: {} bytes\n", data.len());
		resp.write_all(body.as_bytes())?;
		resp.flush()?;
		Ok::<(), anyhow::Error>(())
	})?;

	server.fn_handler("/queue", Method::Get, |request| -> anyhow::Result<()> {
		let mut resp = request.into_response(200, Some("OK"),
			&[("Content-Type","text/plain")])?;
		resp.write_all(b"Queue: []\n")?;
		resp.flush()?;
		Ok::<(), anyhow::Error>(())
	})?;

	{
        // Clone the pin handles into this closure so we can read levels
        let d0 = d0_main.clone();
        let d1 = d1_main.clone();
        let d3 = d3_main.clone();
        let d4 = d4_main.clone();
        let d5 = d5_main.clone();
        let d6 = d6_main.clone();
        let d7 = d7_main.clone();
        let d12 = d12_main.clone();
		server.fn_handler("/pins", Method::Get, move |request| -> anyhow::Result<()> {
			// Output-mode friendly: read the output latch
			let d0v  = d0.lock().unwrap().is_set_high()  as u8;
			let d1v  = d1.lock().unwrap().is_set_high()  as u8;
			let d3v  = d3.lock().unwrap().is_set_high()  as u8;
			let d4v  = d4.lock().unwrap().is_set_high()  as u8;
			let d5v  = d5.lock().unwrap().is_set_high()  as u8;
			let d6v  = d6.lock().unwrap().is_set_high()  as u8;
			let d7v  = d7.lock().unwrap().is_set_high()  as u8;
			let d12v = d12.lock().unwrap().is_set_high() as u8;

			let body = format!(
				r#"{{"D0":{},"D1":{},"D3":{},"D4":{},"D5":{},"D6":{},"D7":{},"D12":{}}}"#,
				d0v, d1v, d3v, d4v, d5v, d6v, d7v, d12v
			);

			let response = request.into_response(
				200,
				Some(&body),
				&[("Content-Type", "application/json")],
			);
			response?.flush()?;
			Ok::<(), anyhow::Error>(())
		})?;
	}

    server.fn_handler("/queue", Method::Post, move|mut request| -> anyhow::Result<()> {

        let header = request.header("Accept").unwrap().to_string();

        // Create a buffer to store the payload
        let mut buffer = [0; 1024];  // Adjust the size as needed

        // Read the payload into the buffer
        let bytes_read = request.read(&mut buffer)?;

        // Convert the bytes to a string
        let payload = std::str::from_utf8(&buffer[..bytes_read])?;

        println!("Received payload: {}", payload);

        match payload.trim() {
            "status_on" => {
                println!("Turning status LED on");
                // ... Set pin 12/13 high ...
                status_led.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("Status on"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "status_off" => {
                println!("Turning status LED off");
                // ... Set pin 12/13 low ...
                status_led.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("Status off"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "relay_on" => {
                println!("Turning relay on");
                // ... Set pin 1 high ...
                relay.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("Relay on"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "relay_off" => {
                println!("Turning relay off");
                // ... Set pin 1 low ...
                relay.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("Relay off"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "scan_wifi" => {
                println!("Scanning Wifi");
                //let scan_results = wifi::scan(|ap_info| {
                    // Store SSID, signal strength, etc
                //})?;

                //let ssid_ron_string = format_scan_results(&scan_results);

                //let response = request.into_response(200, Some(ssid_ron_string), &[("Content-Type", "text/ron")]);
                let response = request.into_response(200, Some("Wifi scan results"), &[("Content-Type", "text/ron")]);
                response?.flush()?;
            },
            "set_wifi" => {
                println!("Setting Wifi");
                // set wifi network parameters

                let response = request.into_response(200, Some("Wifi network settings accepted"), &[("Content-Type", "text/ron")]);
                response?.flush()?;
            },
            "d0_high" => {
                println!("Setting D0 high");
                // ... Set pin D0 high ...
                d0.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D0 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d0_low" => {
                println!("Setting D0 low");
                // ... Set pin D0 low ...
                d0.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D0 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d1_high" => {
                println!("Setting D1 high");
                // ... Set pin D1 high ...
                d1.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D1 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d1_low" => {
                println!("Setting D1 low");
                // ... Set pin D1 low ...
                d1.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D1 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d3_high" => {
                println!("Setting D3 high");
                // ... Set pin D3 high ...
                d3.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D3 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d3_low" => {
                println!("Setting D3 low");
                // ... Set pin D3 low ...
                d3.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D3 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d4_high" => {
                println!("Setting D4 high");
                // ... Set pin D4 high ...
                d4.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D4 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d4_low" => {
                println!("Setting D4 low");
                // ... Set pin D4 low ...
                d4.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D4 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d5_high" => {
                println!("Setting D5 high");
                // ... Set pin D5 high ...
                d5.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D5 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d5_low" => {
                println!("Setting D5 low");
                // ... Set pin D5 low ...
                d5.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D5 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d6_high" => {
                println!("Setting D6 high");
                // ... Set pin D6 high ...
                d6.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D6 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d6_low" => {
                println!("Setting D6 low");
                // ... Set pin D6 low ...
                d6.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D6 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d7_high" => {
                println!("Setting D7 high");
                // ... Set pin D7 high ...
                d7.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D7 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d7_low" => {
                println!("Setting D7 low");
                // ... Set pin D7 low ...
                d7.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D7 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            }, /*
            "d8_high" => {
                println!("Setting D8 high");
                // ... Set pin D8 high ...
                d8.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D8 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d8_low" => {
                println!("Setting D8 low");
                // ... Set pin D8 low ...
                d8.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D8 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d9_high" => {
                println!("Setting D9 high");
                // ... Set pin D9 high ...
                d9.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D9 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d9_low" => {
                println!("Setting D9 low");
                // ... Set pin D9 low ...
                d9.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D9 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d10_high" => {
                println!("Setting D10 high");
                // ... Set pin D10 high ...
                d10.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D10 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d10_low" => {
                println!("Setting D10 low");
                // ... Set pin D10 low ...
                d10.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D10 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d11_high" => {
                println!("Setting D11 high");
                // ... Set pin D11 high ...
                d11.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D11 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d11_low" => {
                println!("Setting D11 low");
                // ... Set pin D11 low ...
                d11.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D11 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d12_high" => {
                println!("Setting D12 high");
                // ... Set pin D12 high ...
                d12.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D12 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d12_low" => {
                println!("Setting D12 low");
                // ... Set pin D12 low ...
                d12.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D12 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d13_high" => {
                println!("Setting D13 high");
                // ... Set pin D13 high ...
                d13.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D13 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d13_low" => {
                println!("Setting D13 low");
                // ... Set pin D13 low ...
                d13.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D13 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d14_high" => {
                println!("Setting D14 high");
                // ... Set pin D14 high ...
                d14.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D14 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d14_low" => {
                println!("Setting D14 low");
                // ... Set pin D14 low ...
                d14.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D14 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d15_high" => {
                println!("Setting D15 high");
                // ... Set pin D15 high ...
                d15.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D15 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d15_low" => {
                println!("Setting D15 low");
                // ... Set pin D15 low ...
                d15.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D15 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d16_high" => {
                println!("Setting D16 high");
                // ... Set pin D16 high ...
                d16.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D16 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d16_low" => {
                println!("Setting D16 low");
                // ... Set pin D16 low ...
                d16.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D16 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d17_high" => {
                println!("Setting D17 high");
                // ... Set pin D17 high ...
                d17.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D17 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d17_low" => {
                println!("Setting D17 low");
                // ... Set pin D17 low ...
                d17.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D17 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d18_high" => {
                println!("Setting D18 high");
                // ... Set pin D18 high ...
                d18.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D18 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d18_low" => {
                println!("Setting D18 low");
                // ... Set pin D18 low ...
                d18.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D18 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d19_high" => {
                println!("Setting D19 high");
                // ... Set pin D19 high ...
                d19.lock().unwrap().set_high()?;

                let response = request.into_response(200, Some("D19 high"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
            "d19_low" => {
                println!("Setting D19 low");
                // ... Set pin D19 low ...
                d19.lock().unwrap().set_low()?;

                let response = request.into_response(200, Some("D19 low"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },*/
            s if s == "g0" || s.starts_with("g0 ") => {
				// Formats: "g0" or "g0 x10 y0 z0 f1500"
				let mut x = 10.0;
				let mut y = 0.0;
				let mut z = 0.0;
				let mut f = 1500.0;

				for tok in s.split_whitespace().skip(1) {
					if let Some(val) = tok.get(1..).and_then(|v| v.parse::<f32>().ok()) {
						match &tok[..1] {
							"x" | "X" => x = val,
							"y" | "Y" => y = val,
							"z" | "Z" => z = val,
							"f" | "F" => f = val,
							_ => {}
						}
					}
				}

				planner.borrow_mut().buffer_line(x, y, z, 0.0, f, 0);
				planner.borrow_mut().recalculate_trapezoids();

				let response = request.into_response(
					200,
					Some("Queued G0"),
					&[("Content-Type", "text/plain")],
				);
				response?.flush()?;
			}
            _ => {
                println!("Unknown command: {}", payload);
                // ... handle unknown command ...

                let response = request.into_response(200, Some("OK"), &[("Content-Type", "text/plain")]);
                response?.flush()?;
            },
        }

        Ok::<(), anyhow::Error>(())
    })?;

    println!("Server awaiting connection");

    // Prevent program from exiting
    loop {
        sleep(Duration::from_millis(1000));
    }
}

fn temperature(val: f32) -> String {
    format!("chip temperature: {:.2}°C", val)
}
