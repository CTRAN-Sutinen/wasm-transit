# cap-alerts

The service alerts capability: what is the agency telling riders? transit-alerts is the library.
- `validate` gives what a write must pass, or the canonical document and its hash. This is the document that `load_service_alerts` answers.
- `put` places one alert by id, checked. With no document, the first alert begins one. `end` ends one alert at a moment.
- `list` gives the agency's list at a moment.
- `flags` gives what a person is told about the alerts that pass.
- `validate_templates`, `put_template`, `templates`, and `fill` are the templates: what a write must pass, one template placed, the menu, and one template written out over the network.
- `receive` writes the alerts log whole (D-228): the write's ops (`put`, `end`, `put_template`, and the whole documents `validate` and `validate_templates`, each with the head it read) over the documents at the head, then the entry. An older service alerts document comes forward in the op.

cap-rt's `encode_alerts` writes the document as an Alerts feed, and `read_alerts` reads one into it.
