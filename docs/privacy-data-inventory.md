# Datenschutz-Dateninventar und Kontolöschung

Dieses Dokument ist die verbindliche Checkliste für personenbezogene Daten in LILLY. Jede neue Migration, Tabelle, Dateiablage oder Cache-Struktur mit Nutzerbezug muss hier vor der Auslieferung ergänzt werden. Die technische Umsetzung der Kontolöschung gehört zu BN-007 und NFR-PRIV-004.

## Löschablauf

Eine Löschanforderung erfordert die Phrase `KONTO LÖSCHEN` und eine höchstens zehn Minuten alte Passwort- oder OAuth-Authentifizierung. Das Konto wird unmittelbar auf `pending_deletion` gesetzt, aus öffentlichen Ansichten und dem Matching entfernt und von normalen API-Zugriffen ausgeschlossen. Alle Sessions und temporären Zugangsdaten werden widerrufen. Über einen auf die Löschroute begrenzten Recovery-Cookie kann die Anforderung sieben Tage lang widerrufen werden.

Nach Ablauf beansprucht ein Worker den Job atomar. Bevor er Daten löscht, schreibt er den zufälligen, nicht aus Kontodaten ableitbaren `erasure_subject` dauerhaft in das append-only Restore-Ledger. Erst danach löscht er eigene Daten, anonymisiert gemeinsam benötigte Historie und reiht Dateien in die persistente Medien-Löschwarteschlange ein. Ein Job gilt erst nach erfolgreicher Dateibereinigung als abgeschlossen. Fehler werden ohne personenbezogene Inhalte gespeichert und mit begrenztem exponentiellem Backoff wiederholt.

## Inventar und Strategie

| Speicher oder Entität                                | Personenbezug                                                     | Verhalten bei Anforderung                                               | Endgültige Strategie                                                                             |
| ---------------------------------------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `users`                                              | E-Mail, Anzeigename, Ort, Passwort-Hash, Avatar-Key, Sichtbarkeit | Konto deaktivieren, Sichtbarkeit ausschalten, `session_version` erhöhen | Zeile vollständig löschen                                                                        |
| `privacy_consents`                                   | Nutzer, Policy-Version und Zeitpunkt                              | bis zum möglichen Widerruf behalten                                     | kaskadierend löschen                                                                             |
| `oauth_identities`                                   | Provider und Provider-Subject                                     | normale Anmeldung sperren                                               | kaskadierend löschen                                                                             |
| `refresh_tokens`, `password_reset_tokens`            | Session-/Reset-Geheimnisse als Hash                               | sofort widerrufen beziehungsweise löschen                               | kaskadierend löschen                                                                             |
| `pending_oauth_links`, `oauth_authorization_flows`   | E-Mail beziehungsweise kurzlebiger OAuth-Kontext                  | kontobezogene Vorgänge sofort entfernen                                 | löschen/kaskadieren; reguläre TTL bleibt zusätzliche Grenze                                      |
| `account_erasure_recovery_tokens`                    | gehashter Recovery-Token und Nutzer-FK                            | nur Status und Widerruf erlauben                                        | beim Widerruf verbrauchen, sonst kaskadierend löschen                                            |
| `account_erasure_jobs`                               | technische Nutzerreferenz während der Ausführung                  | Termin und frühere Sichtbarkeit speichern                               | Nutzerreferenz und Restore-Identifier entfernen; nur technischer Status bleibt                   |
| `collection_entries`, `collection_mutation_receipts` | Sammlung, Zustand, Edition, Notiz                                 | privat halten und aus Matching entfernen                                | kaskadierend löschen                                                                             |
| `collection_photos`, Nutzeravatar                    | Storage-Key und private Bilddatei                                 | nicht mehr öffentlich ausliefern                                        | DB-Zeile löschen; Datei über `media_deletion_jobs` retry-fähig entfernen                         |
| Referenzcover                                        | öffentliches Werk-/Ausgabenbild                                   | unverändert                                                             | behalten; kein nutzereigenes Datum                                                               |
| `trade_matches`, `trade_match_items`                 | aktuelle Beziehung zweier Nutzer                                  | Matches des Kontos entfernen                                            | kaskadierend löschen                                                                             |
| offene `trades`                                      | gemeinsamer Vorgang                                               | neutral mit `account_deletion` abbrechen                                | als terminale Historie anonymisieren                                                             |
| terminale `trades`, `trade_items`, `message_threads` | gemeinsame Historie                                               | für Gegenüber neutral als „Gelöschtes Konto“ anzeigen                   | Nutzer-FKs auf `NULL`; vollständig löschen, sobald kein Teilnehmer verbleibt                     |
| vom gelöschten Konto gesendete `messages`            | Freitext und Absender                                             | keine neuen Nachrichten im geschlossenen Tausch                         | Inhalt durch festen Löschhinweis ersetzen, `sender_id` auf `NULL`                                |
| Nachrichten des Gegenübers                           | dessen Freitext                                                   | unverändert                                                             | für das Gegenüber erhalten, solange es Teilnehmer ist                                            |
| eigene `notifications`                               | persönlicher Posteingang                                          | nicht mehr zugänglich                                                   | kaskadierend löschen                                                                             |
| fremde `notifications`                               | optionaler Akteur und technischer Vorgang                         | Löschabbruch nur neutral melden                                         | Akteur durch `NULL` anonymisieren; abhängige ephemere Hinweise kaskadieren                       |
| `trade_completion_confirmations`                     | Teilnehmerbestätigung                                             | bei terminalem Tausch ohne weitere Wirkung                              | kaskadierend löschen                                                                             |
| `import_jobs.started_by` und `cancel_requested_by`   | Admin-Akteur                                                      | unverändert                                                             | Nutzer-FK auf `NULL`, technischen Lauf behalten                                                  |
| `series_publication_events`, `role_change_events`    | Audit-Akteur beziehungsweise Ziel                                 | unverändert                                                             | Nutzer-FK auf `NULL`, technische Auditentscheidung behalten                                      |
| Backend-Arbeitsspeicher                              | Rate-Limit-Fingerprints, kurzlebiger Zustand                      | normale Zugriffe werden über Kontostatus abgewiesen                     | keine dauerhafte Ablage; Prozessneustart leert den Speicher                                      |
| Browser IndexedDB                                    | Katalog, private Sammlung, Mutationswarteschlange, Konflikte      | auf dem anfordernden Client sofort löschen                              | auf weiteren Clients beim nächsten bestätigten Fehler zum deaktivierten Konto löschen            |
| Service Worker / CacheStorage                        | App-Shell und öffentliche Referenzcover                           | alle `lilly-*`-Caches vorsorglich löschen                               | private API-Antworten werden grundsätzlich nicht gecacht                                         |
| Anwendungslogs                                       | technische IDs und Fehlerkategorien                               | keine neuen Inhalte protokollieren                                      | bestehende Betriebsrotation; keine E-Mail, Tokens, Storage-Keys oder Nachrichtentexte hinzufügen |
| Datenbank-/Medienbackups                             | historischer Snapshot                                             | reguläre Aufbewahrung maximal 14 Tage                                   | nicht nachträglich verändern; Restore-Ledger vor Freigabe zwingend abspielen                     |
| Restore-Ledger                                       | zufälliger 256-Bit-`erasure_subject`                              | append-only Eintrag unmittelbar vor finaler Löschung                    | getrennt persistent und offsite sichern; niemals durch älteren Stand ersetzen                    |

## Gemeinsame Historie

Abgeschlossene und abgebrochene Tausche gehören beiden Teilnehmern. Deshalb bleibt der sachliche Vorgang für das verbleibende Konto bestehen, verliert aber alle Referenzen und Texte des gelöschten Kontos. Die UI und API verwenden ausschließlich den neutralen Namen „Gelöschtes Konto“. Aus dem verbleibenden Datensatz darf weder E-Mail noch Anzeigename, Profil-ID, Avatar, Ort oder Sammlung des gelöschten Kontos rekonstruierbar sein.

## Backup und Wiederherstellung

Das Live-Ledger liegt in Produktion standardmäßig unter `/opt/lilly/shared/erasure-ledger/account-erasure.log` und im Container unter `/erasure-ledger/account-erasure.log`. Die Backup-Prüfsumme umfasst eine Kopie des Ledgers; die Live-Datei bleibt davon unabhängig. `restore.sh` verweigert einen Restore ohne Live-Ledger und führt vor dem Start der öffentlichen Dienste folgenden fail-closed Replay aus:

```bash
lilly-backend privacy replay-erasure-ledger
```

Der Replay sucht alle im Ledger vermerkten Subjects in einem restaurierten Datenbankstand und führt die Löschung erneut aus. Ein ungültiger Ledger-Eintrag, ein Schreib-/Lesefehler oder ein danach noch erreichbares Subject bricht den Restore ab.

## Prüfkriterien

Die automatisierten Tests müssen mindestens diese Invarianten schützen:

- Deaktivierung, Session-Widerruf, öffentliche Unsichtbarkeit und sieben Tage Karenz erfolgen atomar.
- Ein rechtzeitiger Widerruf stellt Sichtbarkeit wieder her, reaktiviert aber keine abgebrochenen Tausche.
- Finalisierung entfernt das Konto und eigene Daten, anonymisiert gemeinsame Historie und erhält fremde Nachrichtentexte.
- Dateijobs sind dem Löschjob zugeordnet; `completed` ist erst nach der Storage-Phase möglich.
- Ledger-Schreiben ist idempotent, validiert ausschließlich 64-stellige Hex-Subjects und geschieht vor der Datenbanklöschung.
- Restore-Replay ist idempotent und lässt keinen im Ledger aufgeführten Nutzer erreichbar.
- Öffentliche Profile, Avatare, Sammlungen, Statistiken und Matching berücksichtigen nur aktive Konten.
- Browser-Bereinigung entfernt private IndexedDB-Daten und LILLY-Caches auf allen erreichbaren Clients.

Zusätzlich ist ein Restore aus einem realen Backup regelmäßig in einer isolierten Umgebung zu testen. Das Ergebnis ist erst verwendbar, wenn der Ledger-Replay erfolgreich beendet wurde und die aufgeführten Subjects nicht mehr in `users` vorkommen.
