use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Response, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::Url;

use crate::{
    AccessoryInfo, Brightness, DeviceEndpoint, ElgatoTemperature, LightState, LightTransport,
    TransportError,
};

const LIGHTS_OPERATION: &str = "light state";
const ACCESSORY_OPERATION: &str = "accessory info";
const IDENTIFY_OPERATION: &str = "identify";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct ReqwestLightTransport {
    client: Client,
    timeout: Duration,
}

impl ReqwestLightTransport {
    pub fn new() -> Result<Self, TransportError> {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Result<Self, TransportError> {
        let client = Client::builder()
            .timeout(timeout)
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| TransportError::InvalidResponse {
                operation: "HTTP client setup",
                message: error.to_string(),
            })?;
        Ok(Self { client, timeout })
    }

    fn endpoint_url(
        endpoint: &DeviceEndpoint,
        path: &str,
        operation: &'static str,
    ) -> Result<Url, TransportError> {
        endpoint
            .url(path)
            .map_err(|error| TransportError::InvalidResponse {
                operation,
                message: error.to_string(),
            })
    }

    async fn decode<T: DeserializeOwned>(
        &self,
        response: Response,
        endpoint: &DeviceEndpoint,
        operation: &'static str,
    ) -> Result<T, TransportError> {
        let response = self.require_success(response, endpoint, operation).await?;
        response.json::<T>().await.map_err(|error| {
            if error.is_timeout() || error.is_body() {
                self.request_error(endpoint, error)
            } else {
                TransportError::MalformedResponse {
                    operation,
                    message: error.to_string(),
                }
            }
        })
    }

    async fn require_success(
        &self,
        response: Response,
        endpoint: &DeviceEndpoint,
        operation: &'static str,
    ) -> Result<Response, TransportError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response
            .text()
            .await
            .map_err(|error| self.request_error(endpoint, error))?;
        Err(TransportError::HttpStatus {
            operation,
            status: status.as_u16(),
            body: if body.is_empty() {
                "empty response body".to_string()
            } else {
                body
            },
        })
    }

    fn request_error(&self, endpoint: &DeviceEndpoint, error: reqwest::Error) -> TransportError {
        if error.is_timeout() {
            TransportError::Timeout {
                endpoint: endpoint.clone(),
                timeout_ms: self.timeout.as_millis(),
            }
        } else {
            TransportError::Connectivity {
                endpoint: endpoint.clone(),
                message: error.to_string(),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct LightsResponse {
    #[serde(rename = "numberOfLights")]
    _number_of_lights: usize,
    lights: Vec<WireLight>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct WireLight {
    on: i32,
    brightness: u8,
    temperature: u16,
}

impl TryFrom<WireLight> for LightState {
    type Error = String;

    fn try_from(value: WireLight) -> Result<Self, Self::Error> {
        Ok(Self {
            is_on: value.on == 1,
            brightness: Brightness::try_from(value.brightness)
                .map_err(|error| error.to_string())?,
            temperature: ElgatoTemperature::try_from(value.temperature)
                .map_err(|error| error.to_string())?,
        })
    }
}

impl From<LightState> for WireLight {
    fn from(value: LightState) -> Self {
        Self {
            on: i32::from(value.is_on),
            brightness: value.brightness.get(),
            temperature: value.temperature.get(),
        }
    }
}

#[derive(Debug, Serialize)]
struct LightsRequest {
    lights: [WireLight; 1],
}

fn first_light(
    response: LightsResponse,
    operation: &'static str,
) -> Result<LightState, TransportError> {
    let wire =
        response
            .lights
            .into_iter()
            .next()
            .ok_or_else(|| TransportError::InvalidResponse {
                operation,
                message: "lights array is empty".to_string(),
            })?;
    LightState::try_from(wire)
        .map_err(|message| TransportError::InvalidResponse { operation, message })
}

#[async_trait]
impl LightTransport for ReqwestLightTransport {
    async fn accessory_info(
        &self,
        endpoint: &DeviceEndpoint,
    ) -> Result<AccessoryInfo, TransportError> {
        let url = Self::endpoint_url(endpoint, "/elgato/accessory-info", ACCESSORY_OPERATION)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| self.request_error(endpoint, error))?;
        self.decode(response, endpoint, ACCESSORY_OPERATION).await
    }

    async fn light_state(&self, endpoint: &DeviceEndpoint) -> Result<LightState, TransportError> {
        let url = Self::endpoint_url(endpoint, "/elgato/lights", LIGHTS_OPERATION)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| self.request_error(endpoint, error))?;
        let response: LightsResponse = self.decode(response, endpoint, LIGHTS_OPERATION).await?;
        first_light(response, LIGHTS_OPERATION)
    }

    async fn set_light_state(
        &self,
        endpoint: &DeviceEndpoint,
        state: LightState,
    ) -> Result<LightState, TransportError> {
        let url = Self::endpoint_url(endpoint, "/elgato/lights", LIGHTS_OPERATION)?;
        let request = LightsRequest {
            lights: [state.into()],
        };
        let response = self
            .client
            .put(url)
            .json(&request)
            .send()
            .await
            .map_err(|error| self.request_error(endpoint, error))?;
        let response: LightsResponse = self.decode(response, endpoint, LIGHTS_OPERATION).await?;
        first_light(response, LIGHTS_OPERATION)
    }

    async fn identify(&self, endpoint: &DeviceEndpoint) -> Result<(), TransportError> {
        let url = Self::endpoint_url(endpoint, "/elgato/identify", IDENTIFY_OPERATION)?;
        let response = self
            .client
            .post(url)
            .send()
            .await
            .map_err(|error| self.request_error(endpoint, error))?;
        self.require_success(response, endpoint, IDENTIFY_OPERATION)
            .await
            .map(|_| ())
    }
}
