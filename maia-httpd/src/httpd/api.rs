// use super::{
//     ad9361::ad9361_json,
//     ddc::ddc_json,
//     geolocation::device_geolocation,
//     json_error::JsonError,
//     recording::{recorder_json, recording_metadata_json},
//     spectrometer::spectrometer_json,
//     time::time_json,
//     version,
// };
// use crate::app::AppState;
// use anyhow::Result;
// use axum::{Json, extract::State};

// async fn api_json(state: &AppState) -> Result<maia_json::Api> {
//     let ad9361 = {
//         let ad9361 = state.ad9361().lock().await;
//         ad9361_json(&ad9361).await
//     }?;
//     let ddc = ddc_json(state).await?;
//     let spectrometer = spectrometer_json(state).await?;
//     let recorder = recorder_json(state).await?;
//     let recording_metadata = recording_metadata_json(state).await;
//     let geolocation = device_geolocation(state);
//     let time = time_json()?;
//     let versions = version::versions(state.ip_core()).await?;
//     Ok(maia_json::Api {
//         ad9361,
//         ddc,
//         geolocation,
//         spectrometer,
//         recorder,
//         recording_metadata,
//         time,
//         versions,
//     })
// }

// pub async fn get_api(State(state): State<AppState>) -> Result<Json<maia_json::Api>, JsonError> {
//     api_json(&state)
//         .await
//         .map_err(JsonError::server_error)
//         .map(Json)
// }
use super::{
    ad9361::ad9361_json,
    ddc::ddc_json,
    geolocation::device_geolocation,
    json_error::JsonError,
    recording::{recorder_json, recording_metadata_json},
    spectrometer::spectrometer_json,
    time::time_json,
    version,
};
use crate::app::AppState;
use anyhow::{Ok, Result};
use axum::{Json, extract::State};
use maia_json::*; // Import all your JSON schemas

/// Returns a stubbed or full JSON snapshot of the API
async fn api_json(state: &AppState) -> Result<Api> {
    // AD9361 stubbed or real
    let ad9361 = 
        // lock only if real hardware
        // let ad9361 = state.ad9361().lock().await;
        // ad9361_json(&ad9361).await
        Ad9361 {
            sampling_frequency: 0,
            rx_rf_bandwidth: 0,
            tx_rf_bandwidth: 0,
            rx_lo_frequency: 0,
            tx_lo_frequency: 0,
            rx_gain: 0.0,
            rx_gain_mode: Ad9361GainMode::Manual,
            tx_gain: 0.0,
        };

    // DDC stubbed
    let ddc = DDCConfigSummary {
        enabled: false,
        frequency: 0.0,
        decimation: 1,
        input_sampling_frequency: 0.0,
        output_sampling_frequency: 0.0,
        max_input_sampling_frequency: 0.0,
    };

    // Spectrometer stubbed
    let spectrometer = Spectrometer {
        input: SpectrometerInput::AD9361,
        input_sampling_frequency: 0.0,
        output_sampling_frequency: 0.0,
        number_integrations: 0,
        fft_size: 0,
        mode: SpectrometerMode::Average,
    };

    // Recorder stubbed
    let recorder = Recorder {
        state: RecorderState::Stopped,
        mode: RecorderMode::IQ8bit,
        prepend_timestamp: false,
        maximum_duration: 0.0,
    };

    // Recording metadata stubbed
    let recording_metadata = RecordingMetadata {
        filename: "".to_string(),
        description: "".to_string(),
        author: "".to_string(),
        geolocation: DeviceGeolocation::default(),
    };

    // Geolocation stubbed
    let geolocation = DeviceGeolocation { point: Some(Geolocation{latitude:89.0,longitude:90.0,altitude:Some(20.0)}) }; // you can stub as needed

    // Time
    let time = time_json()?; // you can stub this if you want

    // Versions stubbed or real
    let versions = Versions { firmware_version: "stub".to_string(), maia_httpd_git: "stub".to_string(), maia_httpd_version: "stub".to_string(), maia_hdl_version: "stub".to_string() };

    Ok(Api {
        ad9361,
        ddc,
        geolocation,
        spectrometer,
        recorder,
        recording_metadata,
        time,
        versions,
    })
}

/// Axum handler
pub async fn get_api(State(state): State<AppState>) -> Result<Json<Api>, JsonError> {
    api_json(&state)
        .await
        .map_err(JsonError::server_error)
        .map(Json)
}
