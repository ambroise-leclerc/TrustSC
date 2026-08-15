#![forbid(unsafe_code)]

use std::fmt::{self, Display};

pub type TrustScResult<T> = Result<T, ValidationError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    message: String,
    code: Option<&'static str>,
    line: Option<u32>,
    column: Option<u32>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
            line: None,
            column: None,
        }
    }

    /// An error tagged with a stable machine-readable identity assigned by the producing crate.
    ///
    /// The identity is deliberately opaque here: this crate does not own any code registry, it
    /// only carries the tag so a caller can route on it without re-deriving meaning from
    /// [`Display`] output, which is free to be reworded.
    pub fn with_code(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: Some(code),
            line: None,
            column: None,
        }
    }

    /// Attaches a structured source position supplied by the producing crate.
    pub fn with_position(mut self, line: Option<u32>, column: Option<u32>) -> Self {
        self.line = line;
        self.column = column;
        self
    }

    /// The stable identity assigned at the point the error was raised, when there is one.
    pub fn code(&self) -> Option<&'static str> {
        self.code
    }

    /// The 1-based source line assigned at the raise site, when known.
    pub fn line(&self) -> Option<u32> {
        self.line
    }

    /// The 1-based source column assigned at the raise site, when known.
    pub fn column(&self) -> Option<u32> {
        self.column
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

pub trait Validates {
    fn validate(&self) -> TrustScResult<()>;
}

pub fn validate_non_empty(field: &str, value: &str) -> TrustScResult<()> {
    if value.trim().is_empty() {
        return Err(ValidationError::new(format!("{field} must not be empty")));
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyClass {
    B,
    C,
}

impl Display for SafetyClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SafetyClass::B => f.write_str("Class B"),
            SafetyClass::C => f.write_str("Class C"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameworkIdentity {
    pub name: String,
    pub version: String,
}

impl Default for FrameworkIdentity {
    fn default() -> Self {
        Self {
            name: "TrustSC".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterminismPolicy {
    pub max_frame_time_ms: u32,
    pub runtime_allocation_allowed: bool,
    pub runtime_object_creation_allowed: bool,
    pub offline_pipeline_required: bool,
}

impl DeterminismPolicy {
    pub fn standard(max_frame_time_ms: u32) -> Self {
        Self {
            max_frame_time_ms,
            runtime_allocation_allowed: true,
            runtime_object_creation_allowed: true,
            offline_pipeline_required: false,
        }
    }

    pub fn vulkan_sc(max_frame_time_ms: u32) -> Self {
        Self {
            max_frame_time_ms,
            runtime_allocation_allowed: false,
            runtime_object_creation_allowed: false,
            offline_pipeline_required: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceContext {
    pub manufacturer: String,
    pub product_name: String,
    pub software_item: String,
    pub version: String,
    pub safety_class: SafetyClass,
}

impl DeviceContext {
    pub fn new(
        manufacturer: impl Into<String>,
        product_name: impl Into<String>,
        software_item: impl Into<String>,
        version: impl Into<String>,
        safety_class: SafetyClass,
    ) -> TrustScResult<Self> {
        let context = Self {
            manufacturer: manufacturer.into(),
            product_name: product_name.into(),
            software_item: software_item.into(),
            version: version.into(),
            safety_class,
        };

        context.validate()?;
        Ok(context)
    }

    pub fn compliance_label(&self) -> String {
        format!(
            "{} {} ({})",
            self.product_name, self.version, self.safety_class
        )
    }
}

impl Validates for DeviceContext {
    fn validate(&self) -> TrustScResult<()> {
        validate_non_empty("manufacturer", &self.manufacturer)?;
        validate_non_empty("product name", &self.product_name)?;
        validate_non_empty("software item", &self.software_item)?;
        validate_non_empty("version", &self.version)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_device_context_fields() {
        let error = DeviceContext::new("", "Infusion Pump", "ui", "0.1.0", SafetyClass::B)
            .expect_err("context should fail validation");

        assert_eq!(error.to_string(), "manufacturer must not be empty");
    }

    #[test]
    fn validation_error_carries_structured_metadata() {
        let error = ValidationError::with_code("TEST-E001", "message mentioning line 99")
            .with_position(Some(7), Some(3));

        assert_eq!(error.code(), Some("TEST-E001"));
        assert_eq!(error.line(), Some(7));
        assert_eq!(error.column(), Some(3));
        assert_eq!(error.to_string(), "message mentioning line 99");
    }
}
