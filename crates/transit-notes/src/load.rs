#[derive(Debug, Clone, serde::Serialize)]
pub struct NotesLoaded {
    pub notes_hash: String,
    pub notes: u32,
}

impl transit_version::Load for crate::Notes {
    const ID: &'static str = "notes";
    const NAME: &'static str = "notes document";
    const LOAD_OP: &'static str = "load_notes";
    const CODE: u32 = 274;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "notes" };
    type Answer = NotesLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::Notes = transit_version::parse_versioned(bytes).map_err(|e| format!("notes document: {e}"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("notes document: {}", reasons.join("; "))
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> NotesLoaded {
        NotesLoaded { notes_hash: hash, notes: self.notes.len() as u32 }
    }
}
