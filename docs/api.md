# Gemeinsame REST-API

Die PWA und andere Clients verwenden dieselbe API unter `/api/v1`. Der verbindliche,
maschinenlesbare Vertrag steht in [openapi.json](openapi.json) (OpenAPI 3.1). Er beschreibt
die implementierten Operationen für Anmeldung/OAuth, Kontolöschung, Serien und Hefte,
Sammlung und Offline-Abgleich, Profile und Medien, Tausch, Nachrichten,
Benachrichtigungen sowie administrative Importe und Freigaben.

## Erzeugen und prüfen

Router und Dokumentation entstehen gemeinsam aus den Handler-Annotationen in
`backend/src/routes/`. `backend/src/api.rs` führt diese Registrierungen zusammen;
Request- und Response-Schemas werden aus den Rust-DTOs abgeleitet.

```sh
python scripts/openapi.py
python scripts/openapi.py --check
```

Der zugrunde liegende Befehl `cargo run --locked -p lilly-backend -- openapi export`
benötigt weder eine laufende Datenbank noch Zugangsdaten, SMTP oder ein Erasure-Ledger.
Das Skript schreibt deterministisches UTF-8 mit LF-Zeilenenden. CI prüft die eingecheckte
Datei auf Übereinstimmung mit dem Code. Vertragsänderungen müssen mitexportiert werden.

## Lokale interaktive Dokumentation

`API_DOCS_ENABLED=true` aktiviert `/api/v1/docs/` und `/api/v1/openapi.json`.
Standardwert ist `false`; beide Ressourcen sind dann nicht vorhanden. Die lokale
Abnahme- und E2E-Konfiguration aktiviert sie ausdrücklich. In Produktivumgebungen
soll die Variable ausgeschaltet bleiben, solange interaktive Dokumentation nicht
bewusst benötigt wird.

Swagger UI, JavaScript und CSS werden aus dem Backend ausgeliefert. Es gibt keinen
CDN-Zugriff und keinen externen Schema-Validator. Nur die Dokumentationsrouten erhalten
eine für diese lokalen Ressourcen passende Content Security Policy; die restriktiven
Header der fachlichen API bleiben erhalten. „Try it out“ verwendet dieselben Cookies,
Berechtigungsprüfungen, Origin-Prüfungen und Ratenlimits wie die PWA. Schreibende
Operationen verändern echte Daten der angesprochenen Umgebung.

## Authentifizierung und Berechtigungen

- Passwort-Login setzt `access_token` und `refresh_token` als HttpOnly-Cookies.
  Clients verwenden Cookie-Sitzungen; ein Bearer-Token ist kein alternativer API-Vertrag.
- `POST /auth/refresh` erneuert die Sitzung und übernimmt die aktuelle Datenbankrolle.
  Ein bestehender Access-Token behält seine Rolle bis zum Ablauf, standardmäßig 15 Minuten.
  Kontodeaktivierung widerruft den Zugang zusätzlich über den vorhandenen Sitzungsmechanismus.
- Neue Passwort- und OAuth-Konten erhalten die Rolle `user`. Administratoren entstehen
  über den dokumentierten Bootstrap-/CLI-Weg. Allgemeine Konto- und Profiloperationen
  vergeben keine Rollen.
- Alle `/admin/`-Operationen verlangen `admin`. Ohne Anmeldung antworten sie mit `401`,
  bei einer normalen Nutzerrolle mit `403`.
- Sammlungen, Nachrichten, Medien und Tauschvorgänge prüfen zusätzlich Eigentum,
  Teilnahme und Sichtbarkeit. Verborgene Ressourcen können mit `404` antworten.
- OAuth verwendet zusätzliche kurzlebige Flow-/Link-Cookies. Die Linkbestätigung
  benötigt auch `X-CSRF-Token`; der OAuth-Flow arbeitet mit State und PKCE.
- Die Kontolöschung verlangt eine frische Anmeldung. Zur Wiederherstellung dient der
  gesondert dokumentierte Recovery-Cookie. Die Operationen definieren ihre jeweiligen
  Cookie-Anforderungen im OpenAPI-Vertrag.

Bei Browserzugriffen müssen schreibende Anfragen vom konfigurierten `APP_BASE_URL`-Origin
stammen. Beispiele sind relative URLs aus der PWA oder `fetch('/api/v1/auth/me',
{ credentials: 'same-origin' })` in derselben Umgebung.

Ein unabhängiger CLI-Client verwendet ebenfalls einen Cookie-Jar. Dieses Beispiel für
Bash, Python 3 und curl fragt das Passwort verdeckt ab und hält es aus den
Befehlsargumenten heraus. Die Basisadresse muss mit `APP_BASE_URL` übereinstimmen:

```sh
(
  set -euo pipefail
  umask 077
  mkdir -p .tmp
  LILLY_API_ORIGIN=http://localhost:18080
  python3 -c 'import getpass,json,sys; print("E-Mail: ", end="", file=sys.stderr); print(json.dumps({"email": input(), "password": getpass.getpass("Passwort: ")}))' |
    curl --fail-with-body --silent --show-error \
      --header "Origin: $LILLY_API_ORIGIN" \
      --header 'Content-Type: application/json' \
      --cookie-jar .tmp/lilly-api.cookies \
      --data-binary @- "$LILLY_API_ORIGIN/api/v1/auth/login"
  curl --fail-with-body --silent --show-error \
    --cookie .tmp/lilly-api.cookies "$LILLY_API_ORIGIN/api/v1/auth/me"
)
```

Die Cookie-Datei enthält Sitzungsdaten und bleibt im Git-ignorierten `.tmp/`-Verzeichnis.
Für einen Refresh wird sie mit `--cookie` gelesen und mit `--cookie-jar` aktualisiert;
`POST /api/v1/auth/refresh` verwendet denselben Origin-Header. Fachliche Operationen
wie `GET /api/v1/me/collection` verwenden anschließend denselben Cookie-Jar.

## Antworten und Fehler

JSON ist das Standardformat. Bilder werden als `image/jpeg`, Datei-Uploads als
`multipart/form-data` übertragen. Erfolgreiche Löschungen können `204` ohne Body liefern;
Importstarts antworten asynchron mit `202`; OAuth- und Verifikationsabläufe verwenden
Redirects. Die konkrete Status-/Inhaltstyp-Kombination steht je Operation im Vertrag.

Jede Operation dokumentiert nur erreichbare Statuscodes aus ihrem Handler, den
aufgerufenen Diensten, der Eingabeprüfung und der Middleware. Das allgemeine
Ratenlimit kann bei allen fachlichen Operationen `429` liefern; die Origin-Prüfung
betrifft nur schreibende Methoden (`403`). Geschützte Leseoperationen können dennoch
`401` oder `403` liefern, etwa bei fehlender Anmeldung oder vorgemerkter Kontolöschung.
JSON-Eingaben berücksichtigen `400`, `413`, `415` und `422`; Multipart-Uploads folgen
ihrer eigenen Eingabeprüfung. Fachliche Fehler, die als Redirect oder als Ergebnis
innerhalb einer erfolgreichen Sync-Antwort behandelt werden, sind keine zusätzlichen
HTTP-Fehlerantworten dieser Operation.

Fehler verwenden ein gemeinsames JSON-Objekt:

```json
{
  "error": "Validation failed",
  "fields": { "email": "Invalid email format" }
}
```

`error` ist erforderlich. `code` und `fields` sind optionale Erweiterungen. Auch
Framework-Ablehnungen, beispielsweise ungültiges JSON (`422`), werden in dieses Format
überführt. Bestehende Statuscodes und Header bleiben erhalten; interne Fehlermeldungen
werden nicht an Clients weitergegeben. `429` enthält `Retry-After` in Sekunden.

Listen verwenden die bestehenden Formate: gewöhnlich `data`, `page`, `per_page`, `total`,
bei der Import-Prüfliste `items` statt `data`. Nicht paginierte Endpunkte liefern Arrays.
Seiten sind 1-basiert; Grenzen und Normalisierung können je Endpunkt abweichen.
Clients sollen den jeweiligen Vertrag beachten und nicht allein aus der Länge der
ersten Seite auf Vollständigkeit schließen.

Kalenderdaten verwenden `YYYY-MM-DD`. `published_at: null` bedeutet ein unbekanntes
Erscheinungsdatum. Vorhandene Angaben müssen gültig sein. Zeitstempel mit UTC-Offset
verwenden `date-time`; historisch ohne Offset serialisierte UTC-Datenbankwerte werden
als `local-date-time` dokumentiert. Das ist eine Beschreibung des bestehenden Formats;
Clients dürfen diese Werte nicht als lokale Uhrzeit des Nutzers interpretieren.

Medien-Uploads verwenden das Feld `photo`. Die bestehenden Uploadgrenzen, MIME-/Bildprüfung,
Neukodierung und Zugriffsregeln gelten unverändert; die Fotopolicy ist über
`GET /media/photo-policy` abrufbar. Avatare und Sammlungsfotos besitzen eigene Operationen.

## Verträge testen und erweitern

- Rust prüft das Dokument gegen das offizielle OpenAPI-3.1-Schema, alle lokalen Referenzen,
  eindeutige Operation-IDs, Pfadparameter und Cookie-Security.
- Die gesamte registrierte Admin-API wird automatisch auf `401`/`403` geprüft.
- Reale Routerantworten und JSON-Fehler werden gegen die Response-Schemas validiert.
- Die Playwright-Fixture prüft abgeschlossene API-Antworten der bestehenden angemeldeten
  Browserabläufe. Zusätzliche Tests prüfen öffentliche/private Antworten, fehlerhafte
  Eingaben und Swagger UI einschließlich CSP und authentifizierter Ausführung.
- Unit-Tests prüfen auch den Vertragsvalidator selbst, insbesondere Nullable-Felder,
  Formate, Pfadzuordnung, Statuscodes, leere und binäre Antworten.

Neue Handler werden ausschließlich über die gemeinsame OpenAPI-Registrierung eingebunden.
Breaking Changes an Pfaden, Eingaben, Cookies, Statuscodes oder Antwortformaten benötigen
eine bewusste Kompatibilitätsentscheidung und gegebenenfalls eine neue API-Version.
Neue optionale Antwortfelder sind additive Änderungen. Noch nicht implementierte
Produktanforderungen werden nicht als verfügbare Operationen dokumentiert.
