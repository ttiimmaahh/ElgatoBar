use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::{AccessoryInfo, DeviceEndpoint, LightState};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("request to {endpoint} timed out after {timeout_ms} ms")]
    Timeout {
        endpoint: DeviceEndpoint,
        timeout_ms: u128,
    },
    #[error("could not connect to {endpoint}: {message}")]
    Connectivity {
        endpoint: DeviceEndpoint,
        message: String,
    },
    #[error("device returned HTTP {status} for {operation}: {body}")]
    HttpStatus {
        operation: &'static str,
        status: u16,
        body: String,
    },
    #[error("device returned malformed JSON for {operation}: {message}")]
    MalformedResponse {
        operation: &'static str,
        message: String,
    },
    #[error("device returned an invalid response for {operation}: {message}")]
    InvalidResponse {
        operation: &'static str,
        message: String,
    },
}

impl TransportError {
    #[must_use]
    pub fn is_connectivity(&self) -> bool {
        match self {
            Self::Timeout { .. } | Self::Connectivity { .. } => true,
            Self::HttpStatus { .. }
            | Self::MalformedResponse { .. }
            | Self::InvalidResponse { .. } => false,
        }
    }
}

#[async_trait]
pub trait LightTransport: Send + Sync {
    async fn accessory_info(
        &self,
        endpoint: &DeviceEndpoint,
    ) -> Result<AccessoryInfo, TransportError>;

    async fn light_state(&self, endpoint: &DeviceEndpoint) -> Result<LightState, TransportError>;

    async fn set_light_state(
        &self,
        endpoint: &DeviceEndpoint,
        state: LightState,
    ) -> Result<LightState, TransportError>;

    async fn identify(&self, endpoint: &DeviceEndpoint) -> Result<(), TransportError>;
}

#[async_trait]
impl<T> LightTransport for Arc<T>
where
    T: LightTransport + ?Sized,
{
    async fn accessory_info(
        &self,
        endpoint: &DeviceEndpoint,
    ) -> Result<AccessoryInfo, TransportError> {
        (**self).accessory_info(endpoint).await
    }

    async fn light_state(&self, endpoint: &DeviceEndpoint) -> Result<LightState, TransportError> {
        (**self).light_state(endpoint).await
    }

    async fn set_light_state(
        &self,
        endpoint: &DeviceEndpoint,
        state: LightState,
    ) -> Result<LightState, TransportError> {
        (**self).set_light_state(endpoint, state).await
    }

    async fn identify(&self, endpoint: &DeviceEndpoint) -> Result<(), TransportError> {
        (**self).identify(endpoint).await
    }
}
