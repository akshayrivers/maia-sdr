//! maia-httpd application.
//!
//! This module contains a top-level structure [`App`] that represents the whole
//! maia-httpd application and a structure [`AppState`] that contains the
//! application state.

use crate::{
    args::Args,
    fpga::{InterruptHandler, IpCore},
    httpd::{self, RecorderFinishWaiter, RecorderState},
    iio::Ad9361,
    spectrometer::{Spectrometer, SpectrometerConfig},
};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// maia-httpd application.
///
/// This struct represents the maia-sdr application. It owns the different
/// objects of which the application is formed, and runs them concurrently.
#[derive(Debug)]
pub struct App {
    httpd: httpd::Server,
    interrupt_handler: InterruptHandler,
    recorder_finish: RecorderFinishWaiter,
    spectrometer: Spectrometer,
}

impl App {
    /// Creates a new application.
    #[tracing::instrument(name = "App::new", level = "debug")]
    pub async fn new(args: &Args) -> Result<App> {
        // Initialize and build application state
        let (ip_core, interrupt_handler) = IpCore::take().await?;
        let ip_core = std::sync::Mutex::new(ip_core);
        let ad9361 = tokio::sync::Mutex::new(Ad9361::new().await?);
        let recorder = RecorderState::new(&ad9361, &ip_core).await?;
        let state = AppState(Arc::new(State {
            ad9361,
            ip_core,
            geolocation: std::sync::Mutex::new(None),
            recorder,
            spectrometer_config: Default::default(),
        }));
        // Initialize spectrometer sample rate and mode
        state.spectrometer_config().set_samp_rate_mode(
            state.ad9361().lock().await.get_sampling_frequency().await? as f32,
            state.ip_core().lock().unwrap().spectrometer_mode(),
        );

        // Build application objects

        let (waterfall_sender, _) = broadcast::channel(16);
        use bytes::Bytes;
        use tokio::time::{Duration, sleep};

        let fake_sender = waterfall_sender.clone();

        tokio::spawn(async move {
            let mut frame: u32 = 0;

            loop {
                let mut data = Vec::with_capacity(4096 * 4);

                for i in 0..4096 {
                    let x = i as f32;
                    let t = frame as f32;

                    // -------------------------------
                    // Base noise floor (slow drift)
                    // -------------------------------
                    let mut val = 20.0
                        + (x / 200.0 + t / 100.0).sin() * 3.0
                        + (x / 80.0 + t / 60.0).cos() * 2.0;

                    // -------------------------------
                    // Strong moving carrier #1
                    // -------------------------------
                    let carrier1_pos = (t * 3.0) % 4096.0;
                    let d1 = (x - carrier1_pos).abs();
                    val += 80.0 * (-d1 / 18.0).exp();

                    // -------------------------------
                    // Strong moving carrier #2 (opposite direction)
                    // -------------------------------
                    let carrier2_pos = 4096.0 - ((t * 2.0) % 4096.0);
                    let d2 = (x - carrier2_pos).abs();
                    val += 65.0 * (-d2 / 25.0).exp();

                    // -------------------------------
                    // Wideband sweep signal
                    // -------------------------------
                    let sweep_center = 2048.0 + (t / 8.0).sin() * 1500.0;

                    let d3 = (x - sweep_center).abs();
                    val += 30.0 * (-d3 / 150.0).exp();

                    // -------------------------------
                    // Burst signal (appears periodically)
                    // -------------------------------
                    let burst = ((frame / 30) % 4) == 0;
                    if burst {
                        let burst_center = 1000.0;
                        let d4 = (x - burst_center).abs();
                        val += 90.0 * (-d4 / 12.0).exp();
                    }

                    // -------------------------------
                    // Multi-tone comb (radio-like)
                    // -------------------------------
                    val += ((x / 40.0 + t / 10.0).sin() * 6.0).max(0.0);
                    val += ((x / 18.0 + t / 6.0).cos() * 4.0).max(0.0);

                    // -------------------------------
                    // Clamp
                    // -------------------------------
                    let val = val.max(0.0);

                    data.extend_from_slice(&val.to_le_bytes());
                }

                let _ = fake_sender.send(Bytes::from(data));

                frame = frame.wrapping_add(1);
                sleep(Duration::from_millis(40)).await;
            }
        });
        let spectrometer = Spectrometer::new(
            state.clone(),
            interrupt_handler.waiter_spectrometer(),
            waterfall_sender.clone(),
        );

        let recorder_finish =
            RecorderFinishWaiter::new(state.clone(), interrupt_handler.waiter_recorder());

        let httpd = httpd::Server::new(
            args.listen,
            args.listen_https,
            args.ssl_cert.as_ref(),
            args.ssl_key.as_ref(),
            args.ca_cert.as_ref(),
            state,
            waterfall_sender,
        )
        .await?;

        Ok(App {
            httpd,
            interrupt_handler,
            recorder_finish,
            spectrometer,
        })
    }

    /// Runs the application.
    ///
    /// This only returns if one of the objects that form the application fails.
    #[tracing::instrument(name = "App::run", level = "debug", skip_all)]
    pub async fn run(self) -> Result<()> {
        tokio::select! {
            ret = self.httpd.run() => ret,
            ret = self.interrupt_handler.run() => ret,
            ret = self.recorder_finish.run() => ret,
            ret = self.spectrometer.run() => ret,
        }
    }
}

/// Application state.
///
/// This struct contains the application state that needs to be shared between
/// different modules, such as different Axum handlers in the HTTP server. The
/// struct behaves as an `Arc<...>`. It is cheaply clonable and clones represent
/// a reference to a shared object.
#[derive(Debug, Clone)]
pub struct AppState(Arc<State>);

#[derive(Debug)]
struct State {
    ad9361: tokio::sync::Mutex<Ad9361>,
    ip_core: Mutex<IpCore>,
    geolocation: Mutex<Option<maia_json::Geolocation>>,
    recorder: RecorderState,
    spectrometer_config: SpectrometerConfig,
}

impl AppState {
    /// Gives access to the [`Ad9361`] object of the application.
    pub fn ad9361(&self) -> &tokio::sync::Mutex<Ad9361> {
        &self.0.ad9361
    }

    /// Gives access to the [`IpCore`] object of the application.
    pub fn ip_core(&self) -> &Mutex<IpCore> {
        &self.0.ip_core
    }

    /// Gives access to the current geolocation of the device.
    ///
    /// The geolocation is `None` if it has never been set or if it has been
    /// cleared, or a valid [`Geolocation`](maia_json::Geolocation) otherwise.
    pub fn geolocation(&self) -> &Mutex<Option<maia_json::Geolocation>> {
        &self.0.geolocation
    }

    /// Gives access to the [`RecorderState`] object of the application.
    pub fn recorder(&self) -> &RecorderState {
        &self.0.recorder
    }

    /// Gives access to the [`SpectrometerConfig`] object of the application.
    pub fn spectrometer_config(&self) -> &SpectrometerConfig {
        &self.0.spectrometer_config
    }

    /// Returns the AD9361 sampling frequency.
    pub async fn ad9361_samp_rate(&self) -> Result<f64> {
        Ok(self.ad9361().lock().await.get_sampling_frequency().await? as f64)
    }
}
