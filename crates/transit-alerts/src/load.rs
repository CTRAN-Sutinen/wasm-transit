#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceAlertsLoaded {
    pub alerts_hash: String,
    pub alerts: u32,
}

impl transit_version::Load for crate::ServiceAlerts {
    const ID: &'static str = "service_alerts";
    const NAME: &'static str = "service alerts document";
    const LOAD_OP: &'static str = "load_service_alerts";
    const CODE: u32 = 272;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "alerts" };
    type Answer = ServiceAlertsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::ServiceAlerts = transit_version::parse_versioned(bytes)
            .map_err(|e| e.context("service alerts document"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("service alerts document: {}", reasons.join("; ")).into()
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> ServiceAlertsLoaded {
        ServiceAlertsLoaded {
            alerts_hash: hash,
            alerts: self.alerts.len() as u32,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AlertTemplatesLoaded {
    pub templates_hash: String,
    pub templates: u32,
}

impl transit_version::Load for crate::AlertTemplates {
    const ID: &'static str = "alert_templates";
    const NAME: &'static str = "alert templates document";
    const LOAD_OP: &'static str = "load_alert_templates";
    const CODE: u32 = 273;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "templates" };
    type Answer = AlertTemplatesLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::AlertTemplates = transit_version::parse_versioned(bytes)
            .map_err(|e| e.context("alert templates document"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("alert templates document: {}", reasons.join("; ")).into()
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> AlertTemplatesLoaded {
        AlertTemplatesLoaded {
            templates_hash: hash,
            templates: self.templates.len() as u32,
        }
    }
}
