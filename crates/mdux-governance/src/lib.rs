#![forbid(unsafe_code)]

use std::collections::HashSet;
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

/// One row of the requirement trace matrix: a requirement, its source clause, and the
/// verification cases and hazards that reference it. The structured form of a
/// `trace_matrix_export` line (ADR-016 §5), consumed by evidence reports without re-parsing text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceRow {
    pub requirement_id: String,
    pub clause: String,
    pub verification_ids: Vec<String>,
    pub hazard_ids: Vec<String>,
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

        let requirement_ids: HashSet<&RequirementId> =
            self.requirements.iter().map(|requirement| &requirement.id).collect();

        for case in &self.verifications {
            if !requirement_ids.contains(&case.requirement) {
                return Err(ValidationError::new(format!(
                    "verification {} references unknown requirement {}",
                    case.id, case.requirement
                )));
            }
        }

        let verified_requirement_ids: HashSet<&RequirementId> =
            self.verifications.iter().map(|case| &case.requirement).collect();

        for requirement in &self.requirements {
            if !verified_requirement_ids.contains(&requirement.id) {
                return Err(ValidationError::new(format!(
                    "requirement {} has no verification case",
                    requirement.id
                )));
            }
        }

        if self.device.safety_class == SafetyClass::C && self.hazards.is_empty() {
            return Err(ValidationError::new(
                "Class C programs must include at least one hazard definition",
            ));
        }

        Ok(())
    }

    /// The structured sibling of [`trace_matrix_export`](Self::trace_matrix_export): one row per
    /// requirement, in the same order, so evidence reports (ADR-016 §5) can join REQ → VER/hazard
    /// without parsing pipe-delimited text.
    pub fn trace_rows(&self) -> Vec<TraceRow> {
        self.requirements
            .iter()
            .map(|requirement| {
                let verification_ids = self
                    .verifications
                    .iter()
                    .filter(|case| case.requirement == requirement.id)
                    .map(|case| case.id.clone())
                    .collect();

                let hazard_ids = self
                    .hazards
                    .iter()
                    .filter(|hazard| hazard.controlled_by.iter().any(|id| id == &requirement.id))
                    .map(|hazard| hazard.id.clone())
                    .collect();

                TraceRow {
                    requirement_id: requirement.id.to_string(),
                    clause: requirement.source_clause.clone(),
                    verification_ids,
                    hazard_ids,
                }
            })
            .collect()
    }

    pub fn trace_matrix_export(&self) -> String {
        let mut lines = vec!["requirement_id|clause|verification_ids|hazard_ids".to_string()];

        for row in self.trace_rows() {
            lines.push(format!(
                "{}|{}|{}|{}",
                row.requirement_id,
                row.clause,
                row.verification_ids.join(","),
                row.hazard_ids.join(",")
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

    #[test]
    fn rejects_a_verification_case_referencing_an_unknown_requirement() {
        let device = DeviceContext::new("Acme", "Pump", "ui", "0.1.0", SafetyClass::B)
            .expect("device should be valid");
        let requirement_id = RequirementId::new("REQ-1").expect("id should be valid");
        let orphan_id = RequirementId::new("REQ-DOES-NOT-EXIST").expect("id should be valid");

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
        program.add_verification(
            VerificationCase::new(
                "VER-ORPHAN",
                orphan_id,
                VerificationMethod::Test,
                "Verification for a requirement that was never registered",
            )
            .expect("verification should be valid"),
        );

        let error = program
            .validate()
            .expect_err("a verification referencing an unknown requirement should be rejected");
        assert_eq!(
            error.to_string(),
            "verification VER-ORPHAN references unknown requirement REQ-DOES-NOT-EXIST"
        );
    }

    #[test]
    fn rejects_a_requirement_with_no_verification_case() {
        let device = DeviceContext::new("Acme", "Pump", "ui", "0.1.0", SafetyClass::B)
            .expect("device should be valid");
        let verified_id = RequirementId::new("REQ-VERIFIED").expect("id should be valid");
        let unverified_id = RequirementId::new("REQ-UNVERIFIED").expect("id should be valid");

        let mut program = ComplianceProgram::new(device);
        program.add_requirement(
            Requirement::new(
                verified_id.clone(),
                "Render alarm state",
                "IEC62304-5.2",
                "Verify alarm screen rendering",
            )
            .expect("requirement should be valid"),
        );
        program.add_requirement(
            Requirement::new(
                unverified_id,
                "Render dosage display",
                "IEC62304-5.2",
                "Verify dosage screen rendering",
            )
            .expect("requirement should be valid"),
        );
        program.add_verification(
            VerificationCase::new(
                "VER-1",
                verified_id,
                VerificationMethod::Test,
                "Manual simulator output",
            )
            .expect("verification should be valid"),
        );

        let error = program
            .validate()
            .expect_err("a requirement with no verification case should be rejected");
        assert_eq!(
            error.to_string(),
            "requirement REQ-UNVERIFIED has no verification case"
        );
    }

    #[test]
    fn trace_rows_match_the_text_export_line_for_line() {
        let device = DeviceContext::new("Acme", "Ventilator", "alarm-ui", "0.1.0", SafetyClass::C)
            .expect("device should be valid");
        let requirement_id = RequirementId::new("REQ-ALARM-001").expect("id should be valid");

        let mut program = ComplianceProgram::new(device);
        program.add_requirement(
            Requirement::new(
                requirement_id.clone(),
                "Render alarm state",
                "IEC62304-5.3",
                "Verify alarm screen rendering",
            )
            .expect("requirement should be valid"),
        );
        program.add_hazard(
            Hazard::new(
                "HAZ-ALARM-001",
                "Alarm suppression due to non-deterministic UI update",
                vec![requirement_id.clone()],
            )
            .expect("hazard should be valid"),
        );
        program.add_verification(
            VerificationCase::new(
                "VER-ALARM-001",
                requirement_id,
                VerificationMethod::Test,
                "Offline deterministic frame trace",
            )
            .expect("verification should be valid"),
        );

        let rows = program.trace_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].requirement_id, "REQ-ALARM-001");
        assert_eq!(rows[0].clause, "IEC62304-5.3");
        assert_eq!(rows[0].verification_ids, vec!["VER-ALARM-001".to_string()]);
        assert_eq!(rows[0].hazard_ids, vec!["HAZ-ALARM-001".to_string()]);

        let export = program.trace_matrix_export();
        let export_lines: Vec<&str> = export.lines().collect();
        assert_eq!(export_lines.len(), rows.len() + 1);
        for (row, line) in rows.iter().zip(export_lines.iter().skip(1)) {
            let expected = format!(
                "{}|{}|{}|{}",
                row.requirement_id,
                row.clause,
                row.verification_ids.join(","),
                row.hazard_ids.join(",")
            );
            assert_eq!(line, &expected);
        }
    }
}
