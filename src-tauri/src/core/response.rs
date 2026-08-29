//! Unified API response wrapper (ported from electron/utils/ResponseUtils.ts).
//!
//! Every IPC command returns an `ApiResponse` serialized as JSON with the same
//! shape as the Electron version: `{ bizCode, data, message }` where
//! `bizCode == "A1000"` means success.

use serde::Serialize;

use super::business_error::{BusinessError, ResponseCode};

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T = serde_json::Value> {
    pub biz_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub message: String,
}

impl ApiResponse {
    pub fn success() -> Self {
        Self {
            biz_code: ResponseCode::Success.biz_code().to_string(),
            data: None,
            message: ResponseCode::Success.message().to_string(),
        }
    }

    pub fn success_data<T: Serialize>(data: T) -> ApiResponse<serde_json::Value> {
        ApiResponse {
            biz_code: ResponseCode::Success.biz_code().to_string(),
            data: Some(serde_json::to_value(data).unwrap_or(serde_json::Value::Null)),
            message: ResponseCode::Success.message().to_string(),
        }
    }

    pub fn fail(code: ResponseCode, message: Option<String>) -> Self {
        Self {
            biz_code: code.biz_code().to_string(),
            data: None,
            message: message.unwrap_or_else(|| code.message().to_string()),
        }
    }

    pub fn fail_error(err: &BusinessError) -> Self {
        Self {
            biz_code: err.biz_code.clone(),
            data: None,
            message: err.message.clone(),
        }
    }

    pub fn fail_internal(msg: impl Into<String>) -> Self {
        Self::fail_error(&BusinessError::internal(msg))
    }

    pub fn is_success(&self) -> bool {
        self.biz_code == "A1000"
    }
}

/// Convenience alias used by command handlers.
pub type CmdResult = Result<ApiResponse<serde_json::Value>, ApiResponse<serde_json::Value>>;

/// Wrap a fallible operation into a `CmdResult` following the Electron
/// `ResponseUtils.success/fail` semantics.
pub fn wrap<T, E>(result: Result<T, E>) -> CmdResult
where
    T: Serialize,
    E: Into<BusinessError>,
{
    match result {
        Ok(data) => Ok(ApiResponse::success_data(data)),
        Err(err) => {
            let be: BusinessError = err.into();
            Err(ApiResponse::fail_error(&be))
        }
    }
}

/// Wrap a unit operation (no data payload) into a `CmdResult`.
pub fn wrap_unit<E>(result: Result<(), E>) -> CmdResult
where
    E: Into<BusinessError>,
{
    match result {
        Ok(()) => Ok(ApiResponse::success()),
        Err(err) => {
            let be: BusinessError = err.into();
            Err(ApiResponse::fail_error(&be))
        }
    }
}
