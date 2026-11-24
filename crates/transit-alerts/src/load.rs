//! The kernel's load of a service alerts document and an alert templates document (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_service_alerts` answers: the hash, and how many alerts the
/// document holds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceAlertsLoaded {
    pub alerts_hash: String,
    pub alerts: u32,
}

/// Service alerts documents (the agency's alerts in the network's ids,
/// D-206), refused unless a write could store them, each refusal named.
impl transit_version::Load for crate::ServiceAlerts {
    const ID: &'static str = "service_alerts";
    const NAME: &'static str = "service alerts document";
    const LOAD_OP: &'static str = "load_service_alerts";
    const CODE: u32 = 272;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "alerts" };
    type Answer = ServiceAlertsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::ServiceAlerts = transit_version::parse_versioned(bytes).map_err(|e| format!("service alerts document: {e}"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("service alerts document: {}", reasons.join("; "))
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> ServiceAlertsLoaded {
        ServiceAlertsLoaded { alerts_hash: hash, alerts: self.alerts.len() as u32 }
    }
}

/// What `load_alert_templates` answers: the hash, and how many templates
/// the document holds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AlertTemplatesLoaded {
    pub templates_hash: String,
    pub templates: u32,
}

/// Alert templates documents (the agency's standard language with blanks,
/// D-210), refused unless a write could store them, each refusal named.
impl transit_version::Load for crate::AlertTemplates {
    const ID: &'static str = "alert_templates";
    const NAME: &'static str = "alert templates document";
    const LOAD_OP: &'static str = "load_alert_templates";
    const CODE: u32 = 273;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "templates" };
    type Answer = AlertTemplatesLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::AlertTemplates = transit_version::parse_versioned(bytes).map_err(|e| format!("alert templates document: {e}"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("alert templates document: {}", reasons.join("; "))
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> AlertTemplatesLoaded {
        AlertTemplatesLoaded { templates_hash: hash, templates: self.templates.len() as u32 }
    }
}
