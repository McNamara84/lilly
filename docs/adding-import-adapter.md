# Neuen Import-Adapter hinzufügen

Quellenspezifischer Code liegt ausschließlich im Crate `importer-adapters`. Das Crate
`importer-core` definiert nur den stabilen Vertrag, Datentypen, Normalisierung und die
wiederverwendbare Vertragsprüfung. Das Backend kennt konkrete Adapter nur über
`lilly_importer_adapters::builtin_registry()`.

## 1. Quelle und Identität festlegen

Vor dem ersten Netzwerkzugriff braucht jeder Adapter einen unveränderlichen
`SourceDescriptor`:

```rust
use lilly_importer_core::SourceDescriptor;

const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    source_key: "example-wiki",
    display_name: "Example Wiki",
    allowed_host: "wiki.example.org",
    series_name: "Example Series",
    series_slug: "example-series",
    series_record_id: "Series:Example",
    series_url: "https://wiki.example.org/Series:Example",
};
```

`source_key`, `series_slug` und `series_record_id` sind persistierte Identitäten. Eine
spätere Änderung benötigt eine Datenmigration. Alle Serien- und Heft-URLs müssen HTTPS
verwenden und exakt zu `allowed_host` gehören.

## 2. Trait implementieren

Die Implementierung kommt in `importer-adapters/src/adapters/<name>.rs` und erfüllt
`WikiAdapter` vollständig:

```rust
use async_trait::async_trait;
use lilly_importer_core::{
    AdapterError, CoverData, IssueData, SeriesData, SourceDescriptor, WikiAdapter,
};

pub struct ExampleAdapter;

#[async_trait]
impl WikiAdapter for ExampleAdapter {
    fn name(&self) -> &str { "example" }
    fn display_name(&self) -> &str { "Example Series" }
    fn version(&self) -> &str { "1.0" }
    fn source_descriptor(&self) -> SourceDescriptor { DESCRIPTOR }

    async fn fetch_series_metadata(&self) -> Result<SeriesData, AdapterError> {
        todo!()
    }

    async fn fetch_issue_list(&self) -> Result<Vec<u32>, AdapterError> {
        todo!()
    }

    async fn fetch_issue_details(&self, number: u32) -> Result<IssueData, AdapterError> {
        todo!()
    }

    async fn fetch_cover(&self, number: u32) -> Result<Option<CoverData>, AdapterError> {
        todo!()
    }
}
```

Pflichtfelder eines Hefts sind positive Heftnummer, Titel, mindestens ein Autor,
Erscheinungsdatum und vollständige Provenienz. Adapter liefern Transportfehler als
`AdapterError::Network`, nicht gefundene Datensätze als `NotFound` und eine Antwort, die sich
gar nicht als Heft interpretieren lässt, als `Parse`. Eine syntaktisch interpretierbare
Antwort darf fehlende Pflichtfelder zunächst als leere Werte beziehungsweise `None` abbilden.
Die generische Orchestrierung ruft anschließend `normalize_and_validate_issue()` auf und
verwirft solche Teilrecords vor jeder Persistenz als recordbezogenen Fehler.

Für jede produktive Quelle sollen mindestens drei `reference_records()` gepflegt werden:
ein früher Datensatz, ein Datensatz aus der Mitte und ein aktueller Datensatz. Diese Werte
sind der fachliche Alarm bei unbeabsichtigten Mappingänderungen.

## 3. Deterministische Tests ergänzen

Tests greifen niemals auf das Live-Wiki zu. Antwortdaten liegen unter
`importer-adapters/tests/fixtures/<name>/`; ein lokaler Fixture-Transport oder HTTP-Testserver
liefert sie an den echten Parser. Jede Adapter-Suite muss mindestens abdecken:

- einen vollständigen Erfolgsfall mit allen Referenzrecords,
- wiederholte, identische Ergebnisse für Serie, Liste, Details und Cover,
- fehlende Pflichtfelder einschließlich der Ablehnung durch die generische Validierung,
- HTTP- und Parserfehler,
- Quellhost und Quellidentität,
- quellenspezifische Randfälle des Mappings.

Die gemeinsame Vertragsprüfung wird zusätzlich aufgerufen:

```rust
#[tokio::test]
async fn example_passes_shared_contract() {
    let adapter = fixture_backed_example_adapter();
    lilly_importer_core::verify_adapter_contract(&adapter)
        .await
        .unwrap();
}
```

Ein Backend-Integrationstest führt die Referenzrecords außerdem durch Orchestrierung und
MariaDB und prüft einen idempotenten Zweitlauf. Große Quelllisten werden in den
Import-Review-Zeilen in Batches geschrieben; Änderungen daran benötigen einen Test oberhalb
von 1.000 Datensätzen.

## 4. Registrieren und prüfen

Das Modul wird in `importer-adapters/src/adapters/mod.rs` exportiert und genau einmal in
`importer-adapters/src/lib.rs::builtin_registry()` registriert. Doppelte Namen werden vom
`AdapterRegistry` abgelehnt; die Admin-Liste ist deterministisch sortiert. Änderungen am
Backend sind für einen weiteren eingebauten Adapter nicht nötig.

Vor dem Merge:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Für Datenbanktests muss `DATABASE_URL` auf eine eigene Testdatenbank zeigen. Danach wird der
neue Adapter bei deaktiviertem Scheduler manuell importiert, fachlich geprüft, ein zweites Mal
unverändert ausgeführt und erst anschließend für geplante Läufe freigeschaltet.
