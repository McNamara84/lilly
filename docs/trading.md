# Matching, Tausche und Nachrichten

Dieses Dokument beschreibt den mit Issues #18, #32, #33 und #34 umgesetzten Tauschablauf – von der editionsbewussten Match-Ermittlung bis zur atomaren Übertragung nach beidseitiger Bestätigung.

## Matching-Regel

Ein Match zwischen zwei Konten entsteht nur, wenn beide Richtungen gleichzeitig erfüllt sind:

- Konto A besitzt mindestens einen `duplicate`-Eintrag zu einem `wanted`-Eintrag von Konto B.
- Konto B besitzt mindestens einen `duplicate`-Eintrag zu einem `wanted`-Eintrag von Konto A.
- Das angebotene und gesuchte Heft muss dieselbe `issue_id` besitzen.
- Ein Wunsch ohne `edition_label` akzeptiert jede Edition. Ist eine Edition angegeben, matcht nur ein Angebot mit derselben, bei der Eingabe getrimmten Bezeichnung. Der MariaDB-Vergleich ist gemäß der verwendeten Collation nicht case-sensitiv.
- Selbst-Matches und Einträge in inaktiven Serien sind ausgeschlossen.

Pro ungeordnetem Kontopaar existiert genau ein persistentes Match. Seine Items werden aus den aktuellen Collection Entries projiziert. Ein SHA-256-Fingerprint erkennt Änderungen; `revision` steigt bei jeder fachlichen Änderung. Verliert das Paar eine Richtung, wird das Match `stale`. Wird es später wieder vollständig, wird derselbe Datensatz reaktiviert.

Beide Konten erhalten beim ersten Treffer eine `trade_match`-Benachrichtigung. `trade_match_updated` wird nur bei einer Erweiterung oder Reaktivierung erzeugt. Der eindeutige Schlüssel aus Match und Revision verhindert doppelte Benachrichtigungen bei wiederholter Reconciliation.

Relevante Collection- und Wunschlistenmutationen, Vorschlagsinvalidierung, Match-Neuberechnung und daraus entstehende Benachrichtigungen laufen in derselben Datenbanktransaktion. Zusätzlich wird beim Serverstart eine vollständige Reconciliation ausgeführt.

## Datenschutzprojektion

Eine Match- oder Tauschantwort enthält nur:

- ID und Anzeigename des konkreten Partners;
- Avatar und Standort, sofern dessen Profil öffentlich ist;
- die konkret passenden beziehungsweise ausgewählten Hefte;
- Match-Score, Status und fachliche Zeitstempel.

E-Mail-Adresse, persönliche Notizen, persönliche Fotos und alle nicht beteiligten Sammlungseinträge werden nicht ausgegeben. Nachrichten sind ausschließlich für Initiator und Empfänger des zugehörigen Tauschs abrufbar. Es gibt keinen Admin-Endpunkt für Nachrichteninhalte.

## Tauschstatus

Aus einem aktiven Match kann ein Teilnehmer ausgewählte Items beider Richtungen vorschlagen. Pro Match ist höchstens ein offener Tausch erlaubt.

| Status      | Bedeutung                                                                             |
| ----------- | ------------------------------------------------------------------------------------- |
| `proposed`  | Der Initiator hat einen unveränderlichen Item-Snapshot vorgeschlagen.                 |
| `accepted`  | Der Empfänger hat angenommen; die referenzierten Einträge sind reserviert.            |
| `cancelled` | Ein Teilnehmer hat abgebrochen oder ein vorgeschlagenes Item wurde relevant geändert. |
| `completed` | Beide Teilnehmer haben den Erhalt bestätigt; die Sammlungen wurden atomar übertragen. |

Nur der Empfänger darf `proposed` annehmen. Annahmen sind idempotent. Beide Teilnehmer dürfen einen vorgeschlagenen oder angenommenen Tausch abbrechen. Ein Eintrag darf nicht gleichzeitig durch zwei angenommene Tausche reserviert sein. Status-, Zustands-, Editions- oder Löschmutationen an reservierten Einträgen liefern `409 entry_reserved_by_trade`. Ein vorgeschlagener Tausch wird bei einer relevanten Mutation stattdessen mit `items_changed` abgebrochen; sein Thread bleibt bestehen.

## Abschluss und Sammlungsübertragung

Nach der Annahme bestätigt jeder Teilnehmer den Erhalt über `POST /api/v1/me/trades/{trade_id}/complete`. Die erste Bestätigung ist idempotent, hält den Tausch im Status `accepted` und verändert noch keine Sammlung. Erst die zweite Bestätigung führt die Übertragung in einer einzigen Datenbanktransaktion aus und setzt den Tausch auf `completed`. Ein Abbruch bleibt bis zu diesem Zeitpunkt möglich.

Vor der Übertragung werden Eigentümer, Status, Ausgabe, Zustand und Edition aller reservierten Einträge gegen den unveränderlichen Tausch-Snapshot geprüft. Bei einer Abweichung liefert die API `409 trade_items_changed`; auch die zweite Bestätigung wird dann zurückgerollt. Parallele Wiederholungen werden über die gesperrte Tauschzeile serialisiert und erzeugen keine doppelten Exemplare.

Abgegebene `duplicate`-Einträge werden entfernt. Der jeweilige Wunscheintrag des Empfängers wird zu `owned` und übernimmt Zustand sowie Edition aus dem Snapshot; weitere empfangene Exemplare erhalten die kleinste freie `copy_number`. Eine eigene Notiz am Wunscheintrag bleibt bestehen. Persönliche Notizen und Fotos des Absenders werden nicht übertragen. Die Fotos werden über die persistente Medien-Löschwarteschlange entfernt.

## Nachrichten und Aufbewahrung

Jeder Tausch erzeugt genau einen Thread. Nachrichten sind unveränderlicher Klartext mit höchstens 4.000 Unicode-Zeichen. HTML und Markdown werden nicht interpretiert. `client_message_id` ermöglicht idempotente Wiederholungen, ohne eine Nachricht doppelt anzulegen.

Die Inbox und Threads verwenden serverseitige Paginierung. Die Oberfläche aktualisiert Threads alle zehn Sekunden und den Benachrichtigungszähler alle 30 Sekunden sowie beim Fensterfokus. Der Read-Endpunkt markiert nur empfangene Nachrichten bis zur angegebenen Nachrichten-ID und synchronisiert die zugehörigen Benachrichtigungen.

Threads und Nachrichten bleiben nach Annahme, Abschluss oder Abbruch erhalten. Wird eines der beiden Konten gelöscht, entfernen MariaDB-Kaskaden den gemeinsamen Match-, Tausch-, Thread-, Nachrichten- und Benachrichtigungskontext vollständig.

## Oberflächen

- `/trades`: Vorschläge, aktive Tausche und abgeschlossene/abgebrochene Historie;
- `/trades/offers`: eigene tauschbare Exemplare;
- `/trades/wanted`: eigene Wunschliste;
- `/trades/[id]`: Tausch-Snapshot, Aktionen und Thread;
- `/messages`: zentrale Inbox;
- `/messages/[id]`: einzelner Thread;
- globale Benachrichtigungsglocke: Match-, Tausch- und Nachrichtenereignisse.
