# Betriebsabnahme: Installation, Backups und Health-Checks

Stand: 22. November 2026. Zugeordnet zu
[#60](https://github.com/McNamara84/lilly/issues/60),
[#61](https://github.com/McNamara84/lilly/issues/61) und
[#62](https://github.com/McNamara84/lilly/issues/62).

## Betriebsmodell und Umsetzung

Die bestätigte Konfiguration verwendet ausschließlich lokale Sicherungen:
täglich gegen 03:30 Uhr in `Europe/Berlin`, bis zu 15 Minuten Streuung und
14 Tage Aufbewahrung. Fehler-E-Mails gehen über den vorhandenen SMTP-Zugang
an ausdrücklich konfigurierte Betriebsadressen. Der vollständige Einrichtungs-,
Migrations- und Wiederherstellungsablauf steht im
[Server-Runbook](server-deployment.md).

Die Umsetzung ergänzt die vorhandenen Produktionsskripte um eine gemeinsame
Host-Konfiguration, sichere Rotation, versionierte Wiederherstellungsdateien,
E-Mail-Fehlerpfade und die getrennte Readiness-/Liveness-Prüfung. Private
Konfigurationsdateien werden als Daten eingelesen; Shell-Substitutionen werden
nicht ausgeführt. Für Compose erzeugte Werte schützen auch literale Dollarzeichen.

## Tatsächlich geprüfter Containerablauf

Der Runner [operations_acceptance.py](../scripts/operations_acceptance.py) wurde
mit echten MariaDB-, Backend-, Frontend-, Caddy- und Mailpit-Containern ausgeführt.
Die lokale Linux-Steuerumgebung verwendet Python 3.14.10 und Compose 5.1.3 auf
Debian 13 innerhalb von Docker Desktop. Compose ist im Werkzeugimage mit Digest
festgelegt und entspricht der beim CI-Fehler geprüften Runner-Version.
Die Images sind aus dem Arbeitsstand gebaut; Image-IDs, tatsächlich verwendete
Compose-Version und UTC-Zeitpunkte stehen im erzeugten JSON-Bericht.

Die sieben Stufen des Durchlaufs sind:

1. Leere persistente Volumes, produktives Backend-Image, erzeugte private
   Konfiguration, unveränderte SMTP-Sonderzeichen im tatsächlichen Container
   und Prüfung des systemd-Kalenders. Der Ausgangsserver läuft mit MariaDB 12.3.3.
   Seine Infrastruktur enthält absichtlich alte Restore-Skripte ohne die neuen
   Python-Helfer; der gepflegte Werkzeugstand wird separat bereitgestellt.
2. Registrierung, zugestellte Verifikationsmail, Bestätigung, Anmeldung und
   Adminvergabe über den vorhandenen Backend-Befehl; erlaubter Admin-API-Zugriff.
3. Sammlungsanlage und Foto-Upload über die API; Erhalt nach Backend-Neustart.
   Nur der kleine öffentliche Katalogfixture wird per SQL eingerichtet, ohne
   Live-Wiki-Zugriffe oder Demo-Konten als Voraussetzung des Bootstraps.
4. Fehlender Schreibzugriff auf Fotoablage beziehungsweise Staging-Verzeichnis:
   Readiness liefert 503, Liveness weiterhin 200. Ein zweiter Backend-Prozess
   prüft zusätzlich einen tatsächlich schreibgeschützt gemounteten Medienbestand.
   Der Mount wird über eine Compose-Override-Datei gesetzt und anschließend mit
   `docker inspect` auf genau einen `/media`-Mount mit `RW=false` geprüft. Damit
   erkennt die Abnahme auch eine verlorene Fehler-Injektion: Compose 5.1.3 behielt
   beim bisherigen `run --volume …:ro` den schreibbaren Service-Mount bei, während
   Compose 2.26.1 ihn ersetzte. Die Override-Datei wurde mit beiden Versionen geprüft.
5. Echter Datenbankdump und Medienarchiv; Wiederaufbau in einen zweiten leeren
   Stack aus den gesicherten Dateien. Prüfung von Notiz, Auflage und unabhängig
   aufgezeichnetem Foto-Hash. Ein nach dem Snapshot gelöschtes Konto und dessen
   Foto dürfen nach Replay des neueren Live-Ledgers weder als Datensatz noch
   als gespeicherte JPEG-Datei zurückkehren. Der gesunde Neuaufbau erhält einen
   aktiven Konfigurationsverweis für Vorabbackup und Rollback beim nächsten Update.
   Das erste Backup über den wiederhergestellten gemeinsamen Einstiegspunkt
   besteht; der wiederhergestellte Fehlerhandler stellt seine Testmail zu.
6. Gestoppte Datenbank: Readiness liefert 503, Liveness weiterhin 200 und der
   fehlgeschlagene Backup-Lauf liefert tatsächlich eine bereinigte E-Mail im
   Test-Mailserver ab. Nach Datenbank-Wiederanlauf folgt wieder Readiness 200.
7. Tatsächlicher Deployment-Aufruf mit Upgrade von MariaDB 12.3.3 auf 13.0.2
   und absichtlich fehlschlagendem Anwendungskandidaten.
   Anwendungsrollback erhält den aktualisierten Datenbankserver und den vorherigen aktiven
   Release-Verweis; Infrastruktur und Backup-Werkzeuge bleiben auf dem versuchten
   Release. Nach Entfernen des im Test injizierten Startfehlers wird der echte
   Vorabdump von 12.3.3 logisch in den weiterlaufenden 13.0.2-Server importiert.
   Datensätze, unabhängiger Foto-Hash und Datenbankversion werden geprüft.
   Ein anschließendes echtes Backup mit diesen Werkzeugen besteht.

Im Docker-Desktop-Controller ist kein systemd als Host-Init aktiv. Ausschließlich
die Abfrage des dort nicht vorhandenen Backup-Dienstes wird beim letzten Schritt
simuliert; Docker-Start, Backup, Fehler und Rollback laufen tatsächlich. Die
Unit-Migrationsfehler und systemd-Invokationskennung werden zusätzlich separat
getestet. Produktiver DNS-/TLS-Ingress, echte Betriebsadressen und ein echter
VPS-Neustart gehören zur nachfolgenden Inbetriebnahme durch den Betreiber und
werden durch diesen Containerlauf nicht als bestanden ausgegeben.

## Automatische Tests und Coverage

- Der Rust-Workspace besteht mit echter isolierter MariaDB: 335 Backend-,
  80 Adapter- und 30 Core-Tests. Linux-Coverage: **88,15 % Zeilen** insgesamt,
  **98,18 %** im Health-Modul, gemessen mit frisch instrumentierten Linux-Builds.
  Die zusätzlichen Tests prüfen Schreibrechte mit einem unprivilegierten Prozess
  und die Probe-Bereinigung nach Abbruch des wartenden Requests.
- Frontend: **1.076 Tests** in 74 Dateien, **98,83 % Statements**, **99,51 % Zeilen**.
  ESLint, Prettier und Svelte-/Service-Worker-Typprüfung bestehen.
- Die API-Vertragstests einschließlich Liveness und lokalem Swagger UI bestehen
  in Chromium, Firefox und WebKit: **sechs Tests**.
- Die **166 Linux-Skripttests** prüfen zusätzlich Konfiguration, unveränderte SMTP-Werte,
  atomare Erstinstallation, beschädigte/gefährliche Archive, Aufbewahrungsgrenzen,
  Eigentümerrechte, Fehler-E-Mails, Sperrkoordination, Datenbank-Imageprüfung
  sowie fehlgeschlagenes Fortsetzen des Backends. Gemeinsam mit dem echten
  Containerlauf ergeben sich **93,27 % kombinierte Anweisungs-/Zweigabdeckung**
  für Python-Betriebswerkzeuge und Abnahmerunner, **99,12 %** für die gemeinsame
  Betriebslogik in `operations.py`. Die Messung verwendet frische Coverage-Daten.

Die neue erforderliche CI-Prüfung [operations.yml](../.github/workflows/operations.yml)
baut die Kandidatenimages, führt Skript- und Containerprüfungen aus und erzwingt
mindestens **90 % kombinierte Anweisungs-/Zweigabdeckung** der Python-Betriebswerkzeuge.
Ein Fehlschlag verhindert Image-Veröffentlichung und Deployment im Main-Workflow.
Reports werden ohne Passwörter, Cookies oder private Sicherungsinhalte gespeichert.

### Reproduzieren

Mit Docker unter Linux oder Windows:

```bash
python scripts/operations_acceptance.py --build
python scripts/operations_acceptance.py --coverage
```

Windows verwendet ein eigenes Linux-Werkzeugimage und ein markiertes,
anschließend entferntes Zustandsvolume. Ports werden im Windows-Host geprüft,
damit Hyper-V-reservierte Ports nicht versehentlich ausgewählt werden. Teststacks
verwenden eigene Netzwerke und Volumes sowie deaktivierte Traefik-Labels.
Aufräumen prüft die Ressourcen-Zugehörigkeit und verwendet keine globale
Docker-Bereinigung.

Unter Linux entspricht die Coverage-Prüfung der CI:

```bash
mkdir -p .tmp
export COVERAGE_FILE="$PWD/.tmp/.coverage-operations"
python3 -m pip install coverage==7.14.1
python3 -m coverage run --rcfile=deploy/tests/coverage.ini -m unittest discover -s scripts/tests -p 'test_*.py'
python3 -m coverage run --rcfile=deploy/tests/coverage.ini scripts/operations_acceptance.py --build
python3 -m coverage combine --rcfile=deploy/tests/coverage.ini
python3 -m coverage report --rcfile=deploy/tests/coverage.ini --fail-under=90
python3 -m coverage json --rcfile=deploy/tests/coverage.ini -o .tmp/operations-coverage.json
```

Wenn bereits zusammengeführte Daten um weitere gezielte Tests ergänzt werden,
`coverage combine --append` verwenden. Für eine unabhängige neue Messung ein
frisches `COVERAGE_FILE` wählen. Rohberichte und Sicherungen gehören nicht in Git.

Weitere relevante Prüfungen:

```bash
cargo fmt --check --all
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python scripts/openapi.py --check
```

## Inbetriebnahme und monatliche Restore-Probe

Vor dem ersten produktiven Lauf richtet der Betreiber `.env.operations`, die
benötigten aktualisierten Units und den Timer-Drop-in gemäß Runbook ein. Die
produktiven Empfänger werden mit einer expliziten Testmail und einem manuellen
Backup geprüft. Die Unit-Migration muss vor dem nächsten Deployment abgeschlossen
sein; fehlende Konfiguration oder ein alter Fehlerhandler blockieren das Upgrade
vor Datenänderungen.

Für den tatsächlichen Linux-Host werden Betriebssystem-/systemd-Version,
Traefik-/HTTPS-Zugriff, Dateirechte, Admin-Anmeldung, Foto und Datenerhalt nach
Host-Neustart protokolliert. Ein isolierter Restore mit erhaltenem aktuellem
Ledger erfolgt mindestens monatlich, koordiniert durch die Betriebsadressen.
Das Protokoll enthält Datum, Backup-ID, geprüfte Revision, Datensatz-/Fotoprüfung,
Ledger-Ergebnis und verantwortliche Rolle; personenbezogene Inhalte und
Zugangsdaten werden nicht aufgenommen.
