# Lokale Vollimport-Abnahme

Diese Abnahme liest echte Wiki-Quellen. Sie ist von reproduzierbaren Fixture-Tests in CI
getrennt und wird mit eigenen Docker-Volumes, Zugangsdaten, Image-Tags und Ports ausgeführt.
Ergebnisse gelten für den protokollierten Quellenstand; laufende Serien können danach wachsen.

## Umgebung vorbereiten

1. `.env.acceptance.example` nach `.env.acceptance.local` kopieren und alle `CHANGE_ME`-Werte
   ersetzen. Für Datenbankkennwörter URL-taugliche Zufallswerte verwenden und denselben Wert
   in `DATABASE_URL` und `MARIADB_PASSWORD` eintragen. Die lokale Datei ist Git-ignoriert.
2. Die isolierte Umgebung starten:

   ```sh
   docker compose --project-name lilly-mvp-acceptance --env-file .env.acceptance.local -f docker-compose.yml -f docker-compose.acceptance.yml up -d --build --wait
   ```

3. `http://localhost:18080` öffnen, das unter `ADMIN_EMAIL` konfigurierte Konto registrieren
   und die Bestätigungsmail in Mailpit unter `http://localhost:18025` öffnen.
4. Das bestätigte Konto bei Bedarf ausdrücklich hochstufen und anschließend neu anmelden:

   ```sh
   docker compose --project-name lilly-mvp-acceptance --env-file .env.acceptance.local -f docker-compose.yml -f docker-compose.acceptance.yml exec backend ./lilly-backend admin promote --email acceptance@example.test
   ```

Demo-Seeding, E2E-Adapter und Scheduler sind ausgeschaltet. Portbindungen sind auf Loopback
beschränkt. Andere Projekte oder vorhandene Entwicklungsdaten werden nicht verwendet.
Die interaktive API-Dokumentation steht hier unter `/api/v1/docs/` zur Verfügung.

## Erstimport und Wiederholung

1. In der Adminansicht die Adapter `maddrax` und `john-sinclair` starten. Beide dürfen
   parallel laufen, jeweils mit ihren bestehenden Abfragepausen. Laufende Jobs vor
   einem Backend-Neustart beenden lassen.
2. Job-IDs, Start-/Endzeit, vollständige Ergebniszähler, Fehlerphasen und Coverwarnungen
   protokollieren. Quellliste und Prüfliste müssen vollständig zusammenpassen.
3. Alle sechs Referenzhefte prüfen. Fehlende Datumsangaben allein blockieren nicht;
   ungültige vorhandene Daten, fehlende Pflichtmetadaten oder Referenzabweichungen dagegen schon.
   Übersprungene Hefte müssen ein nachweislich zukünftiges Erscheinungsdatum besitzen.
4. Vor Freigabe mit einem anonymen Browser kontrollieren, dass die Serie nicht im Katalog
   erscheint. Nach erfolgreicher Prüfung aktivieren, begründete Coverwarnungen gegebenenfalls
   ausdrücklich bestätigen und die öffentliche Darstellung prüfen.
5. Mindestens einen Sammlungsdatensatz pro Serie mit Zustand und Notiz anlegen. Einen
   vollständigen Ausgangssnapshot erzeugen; das Skript fragt Zugangsdaten verdeckt ab:

   ```sh
   python scripts/verify-mvp-imports.py --jobs 1 2 --output acceptance-first.json
   ```

6. Beide Vollimporte wiederholen. Nach Abschluss die tatsächlichen neuen Job-IDs einsetzen:

   ```sh
   python scripts/verify-mvp-imports.py --jobs 3 4 --baseline acceptance-first.json --output acceptance-repeat.json
   ```

Die Dateinamen und Job-IDs sind Beispiele. Snapshots außerhalb von Git bzw. im ignorierten
`.tmp/`-Verzeichnis ablegen. Alternativ zum Prompt sind `LILLY_ACCEPTANCE_EMAIL` und
`LILLY_ACCEPTANCE_PASSWORD` möglich; Passwörter gehören nicht in Befehlsargumente.

Das Prüfskript liest alle Seiten, kontrolliert vollständige Verarbeitung, eindeutige
Heftnummern/IDs, Titel, Autoren, Quellenidentitäten und gültige Datumsangaben. Bei den beiden
Startserien prüft es die Nummernfolge ab Heft 1; ungeklärte Lücken verhindern die Abnahme
auch dann, wenn alle vom Importer erfassten Ergebniszähler übereinstimmen. Zusätzlich
vergleicht es die sechs fest vorgegebenen Referenzen unabhängig vom Server, prüft den öffentlichen Katalog
und erkennt bei Wiederholung zusätzliche Hefte, geänderte IDs, Metadaten/Cover,
unberechtigt gezählte Aktualisierungen oder veränderte Nutzerdaten
der jeweils ausgewählten Serien.
Es startet keine Imports und ändert keine fachlichen Daten. Seine Berichte enthalten
Zähler und SHA-256-Vergleichswerte, keine Zugangsdaten oder Sammlungsnotizen.

Quelländerungen zwischen Läufen können einen Vergleich berechtigt scheitern lassen. In
diesem Fall die Unterschiede dokumentieren und einen neuen, konsistenten Ausgangssnapshot
aufnehmen. Fehler oder blockierende Quellenlücken dürfen nicht durch künstliche Metadaten
oder Änderungen an den Abnahmekriterien verdeckt werden.

## Nachweis und Abschluss

Der Abnahmebericht hält je Serie Quellenumfang, persistierte Hefte, Zukunftshefte,
fehlende optionale Daten, Cover-/Metadatenbefunde, Referenzen, Veröffentlichung sowie
Ergebnis des zweiten Laufs fest. Code-/Image-Stand und Testbefehle gehören ebenfalls hinein.
Erst wenn diese Nachweise vorliegen, ist der vollständige Live-Import nachgewiesen.

Die Umgebung kann mit demselben Compose-Projektnamen gestoppt werden. Volumes bleiben
für Nachprüfungen erhalten. Eine Löschung der Abnahmevolumes ist ein gesonderter Schritt;
keine globalen Docker-Bereinigungsbefehle verwenden.
