# Persönliche Datenauskunft als ZIP

Unter `/account/data-export`, erreichbar aus dem eigenen Profil, können Nutzer
eine vollständige Kopie der serverseitig gespeicherten eigenen Daten anfordern.
Der Auftrag läuft im Hintergrund. Die Seite zeigt Warteschlange, Verarbeitung,
Ergebnis und die konkrete Downloadfrist.

## Anmeldung und Downloadfrist

Anforderung und Download verlangen ein aktives eigenes Konto und eine höchstens
zehn Minuten alte Passwort- oder OAuth-Anmeldung. Statusabfragen verlangen die
normale aktive Sitzung. Die bestehenden Reauthentifizierungswege werden verwendet;
eine OAuth-Bestätigung führt über das feste interne Profilziel zur Auskunft zurück.
Eine Auftragskennung allein berechtigt nicht zum Zugriff, auch nicht als Admin.

Ein fertiger Export ist genau 24 Stunden ab erfolgreicher Fertigstellung abrufbar.
Downloads verlängern die Frist nicht. Ein späterer erneuter Download kann eine neue
Anmeldebestätigung benötigen. Nach Ablauf ist jeder neue Zugriff sofort gesperrt,
unabhängig davon, ob die physische Dateibereinigung bereits erfolgreich war.

Es gelten standardmäßig ein aktiver Auftrag je Art und Nutzer, 24 neue
Transferaufträge innerhalb von 24 Stunden und maximal 2 GiB je Archiv. Der Betreiber
kann die Zahl und die Größenlimits konfigurieren. Ein Überschreiten liefert einen
Fehler und niemals einen still gekürzten Export. Große ZIP-Dateien werden durch den
normalen Browserdownload gestreamt und nicht vollständig in JavaScript gepuffert.

## Archivinhalt

Das ZIP enthält `manifest.json`, `README.txt`, `collection.json`, `collection.csv`,
thematisch gegliederte Dateien unter `data/` und eigene gespeicherte Dateien unter
`files/`. Die Fotos sind die von LILLY normalisierten gespeicherten Dateien; die
vor dem Upload vorhandenen, entfernten EXIF-Metadaten können nicht rekonstruiert werden.

| Datensatz | Inhalt |
| --- | --- |
| `account` | Eigene Konto-/Profildaten, E-Mail, Rolle, Sichtbarkeit, Verifikation und gespeicherte Zeitstempel. |
| `consents`, `oauth_identities`, `sessions` | Einwilligungen und Policy-Versionen, eigene verknüpfte Provideridentitäten und Session-Metadaten ohne Zugangsschlüssel. |
| `collection`, `collection_metadata`, `copy_keys` | Alle eigenen Exemplare einschließlich Edition, Zustand, Notiz, Status, Zeiten, Revisionen und dauerhaften Kennungen/Löschmarkierungen. Inaktive Serien bleiben enthalten. |
| `offline_receipts` | Eigene bestätigte Offline-Mutationen und ihre Ergebnisse ohne interne Anforderungs-Fingerprints. Noch nicht synchronisierte lokale Änderungen sind nicht auf dem Server vorhanden. |
| `matches`, `match_items`, `trades`, `trade_items`, `confirmations` | Eigene Tauschbeziehungen und der für die Teilnahme berechtigte vereinbarte Kontext. |
| `threads`, `messages` | Alle sichtbaren Unterhaltungen in zeitlicher Reihenfolge, einschließlich empfangener Nachrichten, ohne Begrenzung auf eine API-Seite. |
| `notifications` | Eigene Hinweise und Lesestände. |
| `photos`, `avatar` | Eigene Dateimetadaten und relative Verweise auf enthaltene Dateien. |
| `role_changes`, `wiki_import_activity`, `publication_activity` | Eigene administrative beziehungsweise historische Aktionen, soweit gespeichert. |
| `transfer_jobs`, `import_results`, `account_deletion` | Noch gespeicherte eigene Auftragsdaten und Ergebnisse. Aufbewahrungsgrenzen gelten auch für diese Bestände. |
| `comments`, `ratings` | Derzeit leere, ausdrücklich als noch nicht implementiert ausgewiesene Bereiche. |

Eigene Fotos liegen unter `files/photos/`, der Avatar unter `files/avatar.jpg`.
Noch aktive gespeicherte Importdateien werden unter `files/imports/` beigelegt.
Die Sammlung verwendet das [versionierte Übertragungsformat](collection-transfer-format.md).
`data/copy_keys.json` verbindet die Exemplar-Kennungen mit den Eintragsreferenzen
der Foto- und Metadatenlisten.

Das Manifest nennt Snapshot und Erstellungszeit, Datensatzzahlen und SHA-256-Prüfsummen
mit Bytezahlen für die enthaltenen Dateien. Das Manifest selbst ist wegen der
Selbstreferenz nicht Teil seiner eigenen Prüfsummenliste. SQL-Zeitstempel sind UTC;
boolesche Datenbankwerte können je Feld als `true`/`false` oder `1`/`0` erscheinen.

Der Snapshot wird während der tatsächlichen Verarbeitung aufgenommen, nicht beim
Anfordern. Die Datenbank wird konsistent gelesen. Fotos und Avatar werden passend
dazu privat zwischengespeichert. Verschwindet dabei eine erwartete Datei, wird der
Snapshot begrenzt wiederholt oder der Auftrag schlägt fehl. Ein unvollständiges
Archiv erhält niemals den Status „Download bereit“.

## Datenschutzgrenzen

Nachrichten enthalten die vom Teilnehmer sichtbaren gesendeten und empfangenen
Texte. Stammdaten des Gegenübers werden ausschließlich nach der bestehenden
Tauschansicht projiziert; fremde E-Mails, Standorte, Fotos und sonstige private
Sammlungsdaten werden nicht zusätzlich exportiert. Bereits anonymisierte Partner
bleiben anonymisiert; entfernte Nachrichtentexte werden nicht wiederhergestellt.

Passwort-Hashes, OAuth-Client-Geheimnisse, Zugangstokens und ihre Hashes, Reset- und
Recovery-Werte, Cookies, Restore-Ledger-Identifier, interne Storage-Schlüssel und
Hostpfade sind ausgeschlossen. Die Auswahl erfolgt über ausdrückliche Feldlisten,
nicht durch unkontrolliertes Serialisieren von Datenbankzeilen. Sicherheitszustände,
technische Caches und abgeleitete Bestände sind im [Dateninventar](privacy-data-inventory.md)
klassifiziert. Das Archiv enthält keine nachzuladenden externen Ressourcen.

Bei einer eigenen Kontolöschanforderung werden Aufträge ungültig und Dateien
retry-fähig entfernt. Eine Rücknahme der Löschung reaktiviert alte Archive nicht.
Betroffene noch abrufbare Archive anderer Teilnehmer werden bei Partnerlöschung
ebenfalls ungültig. Vor Veröffentlichung und Download werden die erfassten
Kontozustände erneut geprüft. Bereits heruntergeladene lokale Kopien können
serverseitig nicht zurückgerufen werden.

## Betrieb und Wiederherstellung

`transfer_data` ist ausschließlich im Backend unter `/data-transfer` gemountet.
Es wird weder von Caddy statisch ausgeliefert noch in die dauerhaften Medienbackups
aufgenommen. Downloads passieren die API-Berechtigungsprüfung und verwenden
`Cache-Control: no-store`; Service Worker und IndexedDB speichern keine ZIP-Dateien.

Der Worker verarbeitet anfangs einen aufwendigen Transfer gleichzeitig. Er prüft
Fristen und offene Löschungen regelmäßig. Laufende Dateiproduzenten halten eine
interne Schreibsperre gegen die Bereinigung; auch ein abgebrochener HTTP-Upload
lässt seinen begonnenen privaten Schreibvorgang kontrolliert auslaufen.

Beim Neustart werden unterbrochene Aufträge als fehlgeschlagen kenntlich gemacht.
Fehlende Dateien nach Restore oder Volume-Verlust führen nicht zu gültigen
Downloadangeboten. Verwaiste Auftragsverzeichnisse ohne Datenbankeintrag werden beim
Start entfernt. Physische Löschfehler bleiben retry-fähig; die Kontolöschung gilt
erst nach abgeschlossener Dateibereinigung als vollständig verarbeitet.

Operative Auftragsmetadaten bleiben höchstens sieben Tage erhalten. Das ZIP ist eine
Datenauskunft; ein Import von Konto, Nachrichten oder Bildern aus diesem Archiv ist
keine unterstützte Kontowiederherstellung. Das [Server-Runbook](server-deployment.md)
beschreibt Volume, Konfiguration und die Schritte bei Rollback oder Restore.
