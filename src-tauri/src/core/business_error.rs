//! Business errors and response codes (ported from electron/core/BusinessError.ts).
//!
//! Response codes are kept byte-for-byte identical to the Electron version so
//! that renderer error handling (bizCode switches) keeps working unchanged.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    Success,
    InternalError,
    NotConfig,
    VersionExists,
    VersionArgsError,
    UnknownVersion,
    NotFoundVersion,
    WebServerPortInUse,
}

impl ResponseCode {
    /// The raw "CODE;message" payload used by the renderer.
    pub fn raw(&self) -> &'static str {
        match self {
            ResponseCode::Success => "A1000;successful.",
            ResponseCode::InternalError => "B1000;internal error.",
            ResponseCode::NotConfig => "B1001;未配置",
            ResponseCode::VersionExists => "B1002;导入失败，版本已存在",
            ResponseCode::VersionArgsError => "B1003;所选 frp 架构与操作系统不符",
            ResponseCode::UnknownVersion => "B1004;无法识别文件",
            ResponseCode::NotFoundVersion => "B1005;未找到版本",
            ResponseCode::WebServerPortInUse => "B1006;WebServer Port In Use",
        }
    }

    pub fn biz_code(&self) -> &'static str {
        let raw = self.raw();
        let idx = raw.find(';').unwrap_or(raw.len());
        &raw[..idx]
    }

    pub fn message(&self) -> &'static str {
        let raw = self.raw();
        let idx = raw.find(';').unwrap_or(raw.len());
        if idx < raw.len() {
            &raw[idx + 1..]
        } else {
            ""
        }
    }
}

/// Business error carrying a `ResponseCode`.
#[derive(Debug, Clone, Serialize)]
pub struct BusinessError {
    pub biz_code: String,
    pub message: String,
}

impl BusinessError {
    pub fn new(code: ResponseCode) -> Self {
        Self {
            biz_code: code.biz_code().to_string(),
            message: code.message().to_string(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            biz_code: ResponseCode::InternalError.biz_code().to_string(),
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for BusinessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{};{}", self.biz_code, self.message)
    }
}

impl std::error::Error for BusinessError {}

impl From<BusinessError> for String {
    fn from(e: BusinessError) -> String {
        format!("{};{}", e.biz_code, e.message)
    }
}
