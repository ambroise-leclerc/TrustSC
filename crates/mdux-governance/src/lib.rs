#![forbid(unsafe_code)]

use std::fmt::{self, Display};

use mdux_core::{
    DeviceContext, MduxResult, SafetyClass, Validates, ValidationError, validate_non_empty,
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct RequirementId(String);

impl RequirementId {
    pub fn new(value: impl Into<String>) -> MduxResult<Self> {
        let value = value.into();
        validate_non_empty("requirement id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RequirementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Requirement {
    pub id: RequirementId,
    pub title: String,
    pub source_clause: String,
    pub verification_intent: String,
}

impl Requirement {
    pub fn new(
        id: RequirementId,
        title: impl Into<String>,
        source_clause: impl Into<String>,
        verification_intent: impl Into<String>,
    ) -> MduxResult<Self> {
        let requirement = Self {
            id,
            title: title.into(),
            source_clause: source_clause.into(),
            verification_intent: verification_intent.into(),
        };

        requirement.validate()?;
        Ok(requirement)
    }
}

impl Validates for Requirement {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("requirement title", &self.title)?;
        validate_non_empty("source clause", &self.source_clause)?;
        validate_non_empty("verification intent", &self.verification_intent)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hazard {
    pub id: String,
    pub description: String,
    pub controlled_by: Vec<RequirementId>,
}

impl Hazard {
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        controlled_by: Vec<RequirementId>,
    ) -> MduxResult<Self> {
        let hazard = Self {
            id: id.into(),
            description: description.into(),
            controlled_by,
        };

        hazard.validate()?;
        Ok(hazard)
    }
}

impl Validates for Hazard {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("hazard id", &self.id)?;
        validate_non_empty("hazard description", &self.description)?;
        if self.controlled_by.is_empty() {
            return Err(ValidationError::new(
                "hazard must be controlled by at least one requirement",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationMethod {
    Analysis,
    Inspection,
    Test,
    Demonstration,
}

impl Display for VerificationMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerificationMethod::Analysis => f.write_str("analysis"),
            VerificationMethod::Inspection => f.write_str("inspection"),
            VerificationMethod::Test => f.write_str("test"),
            VerificationMethod::Demonstration => f.write_str("demonstration"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationCase {
    pub id: String,
    pub requirement: RequirementId,
    pub method: VerificationMethod,
    pub evidence: String,
}

impl VerificationCase {
    pub fn new(
        id: impl Into<String>,
        requirement: RequirementId,
        method: VerificationMethod,
        evidence: impl Into<String>,
    ) -> MduxResult<Self> {
        let case = Self {
            id: id.into(),
            requirement,
            method,
            evidence: evidence.into(),
        };

        case.validate()?;
        Ok(case)
    }
}

impl Validates for VerificationCase {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("verification id", &self.id)?;
        validate_non_empty("verification evidence", &self.evidence)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProblemReport {
    pub id: String,
    pub summary: String,
    pub closed: bool,
}

impl ProblemReport {
    pub fn new(
        id: impl Into<String>,
        summary: impl Into<String>,
        closed: bool,
    ) -> MduxResult<Self> {
        let report = Self {
            id: id.into(),
            summary: summary.into(),
            closed,
        };

        report.validate()?;
        Ok(report)
    }
}

impl Validates for ProblemReport {
    fn validate(&self) -> MduxResult<()> {
        validate_non_empty("problem report id", &self.id)?;
        validate_non_empty("problem report summary", &self.summary)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditCategory {
    Lifecycle,
    Verification,
    Release,
    Runtime,
}

impl Display for AuditCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditCategory::Lifecycle => f.write_str("lifecycle"),
            AuditCategory::Verification => f.write_str("verification"),
            AuditCategory::Release => f.write_str("release"),
            AuditCategory::Runtime => f.write_str("runtime"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEvent {
    pub sequence: u64,
    pub category: AuditCategory,
    pub message: String,
}

pub trait AuditEventSink {
    fn write_event(&mut self, category: AuditCategory, message: String);
    fn events(&self) -> &[AuditEvent];
}

#[derive(Default)]
pub struct MemoryAuditSink {
    next_sequence: u64,
    events: Vec<AuditEvent>,
}

impl MemoryAuditSink {
    pub fn new() -> Self {
        Self {
            next_sequence: 1,
            events: Vec::new(),
        }
    }
}

impl AuditEventSink for MemoryAuditSink {
    fn write_event(&mut self, category: AuditCategory, message: String) {
        let event = AuditEvent {
            sequence: self.next_sequence,
            category,
            message,
        };

        self.events.push(event);
        self.next_sequence += 1;
    }

    fn events(&self) -> &[AuditEvent] {
        &self.events
    }
}

pub struct ComplianceProgram {
    device: DeviceContext,
    requirements: Vec<Requirement>,
    hazards: Vec<Hazard>,
    verifications: Vec<VerificationCase>,
    problems: Vec<ProblemReport>,
    audit_sink: MemoryAuditSink,
}

impl ComplianceProgram {
    pub fn new(device: DeviceContext) -> Self {
        let mut program = Self {
            device,
            requirements: Vec::new(),
            hazards: Vec::new(),
            verifications: Vec::new(),
            problems: Vec::new(),
            audit_sink: MemoryAuditSink::new(),
        };

        program.record_event(
            AuditCategory::Lifecycle,
            format!(
                "created compliance program for {}",
                program.device.software_item
            ),
        );
        program
    }

    pub fn device(&self) -> &DeviceContext {
        &self.device
    }

    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }

    pub fn audit_events(&self) -> &[AuditEvent] {
        self.audit_sink.events()
    }

    pub fn has_requirement(&self, requirement_id: &RequirementId) -> bool {
        self.requirements
            .iter()
            .any(|requirement| &requirement.id == requirement_id)
    }

    pub fn add_requirement(&mut self, requirement: Requirement) {
        self.record_event(
            AuditCategory::Lifecycle,
            format!("registered requirement {}", requirement.id),
        );
        self.requirements.push(requirement);
    }

    pub fn add_hazard(&mut self, hazard: Hazard) {
        self.record_event(
            AuditCategory::Lifecycle,
            format!("registered hazard {}", hazard.id),
        );
        self.hazards.push(hazard);
    }

    pub fn add_verification(&mut self, case: VerificationCase) {
        self.record_event(
            AuditCategory::Verification,
            format!("registered verification {}", case.id),
        );
        self.verifications.push(case);
    }

    pub fn add_problem_report(&mut self, report: ProblemReport) {
        self.record_event(
            AuditCategory::Lifecycle,
            format!("recorded problem report {}", report.id),
        );
        self.problems.push(report);
    }

    pub fn record_event(&mut self, category: AuditCategory, message: impl Into<String>) {
        self.audit_sink.write_event(category, message.into());
    }

    pub fn validate(&self) -> MduxResult<()> {
        self.device.validate()?;

        if self.requirements.is_empty() {
            return Err(ValidationError::new(
                "compliance program must include at least one requirement",
            ));
        }

        if self.verifications.is_empty() {
            return Err(ValidationError::new(
                "compliance program must include at least one verification case",
            ));
        }

        if self.device.safety_class == SafetyClass::C && self.hazards.is_empty() {
            return Err(ValidationError::new(
                "Class C programs must include at least one hazard definition",
            ));
        }

        Ok(())
    }

    pub fn trace_matrix_export(&self) -> String {
        let mut lines = vec!["requirement_id|clause|verification_ids|hazard_ids".to_string()];

        for requirement in &self.requirements {
            let verification_ids = self
                .verifications
                .iter()
                .filter(|case| case.requirement == requirement.id)
                .map(|case| case.id.as_str())
                .collect::<Vec<_>>()
                .join(",");

            let hazard_ids = self
                .hazards
                .iter()
                .filter(|hazard| hazard.controlled_by.iter().any(|id| id == &requirement.id))
                .map(|hazard| hazard.id.as_str())
                .collect::<Vec<_>>()
                .join(",");

            lines.push(format!(
                "{}|{}|{}|{}",
                requirement.id, requirement.source_clause, verification_ids, hazard_ids
            ));
        }

        lines.join("\n")
    }

    pub fn audit_export(&self) -> String {
        let mut lines = vec!["sequence|category|message".to_string()];

        for event in self.audit_sink.events() {
            lines.push(format!(
                "{}|{}|{}",
                event.sequence, event.category, event.message
            ));
        }

        lines.join("\n")
    }

    pub fn release_evidence_summary(&self) -> String {
        format!(
            "device={} class={} requirements={} hazards={} verifications={} problems={} audit_events={}",
            self.device.software_item,
            self.device.safety_class,
            self.requirements.len(),
            self.hazards.len(),
            self.verifications.len(),
            self.problems.len(),
            self.audit_sink.events().len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdux_core::SafetyClass;

    #[test]
    fn class_c_program_requires_hazard() {
        let device = DeviceContext::new("Acme", "Pump", "ui", "0.1.0", SafetyClass::C)
            .expect("device should be valid");
        let requirement_id = RequirementId::new("REQ-1").expect("id should be valid");

        let mut program = ComplianceProgram::new(device);
        program.add_requirement(
            Requirement::new(
                requirement_id.clone(),
                "Render alarm state",
                "IEC62304-5.2",
                "Verify alarm screen rendering",
            )
            .expect("requirement should be valid"),
        );
        program.add_verification(
            VerificationCase::new(
                "VER-1",
                requirement_id,
                VerificationMethod::Test,
                "Manual simulator output",
            )
            .expect("verification should be valid"),
        );

        let error = program
            .validate()
            .expect_err("Class C should require a hazard");
        assert_eq!(
            error.to_string(),
            "Class C programs must include at least one hazard definition"
        );
    }
}
