# Importquellen und Mappingvertrag

## Verbindliche Quellen

| Adapter | `source_key` | Erlaubter Host | Serien-Quell-ID | Indexseite |
|---|---|---|---|---|
| `maddrax` | `maddraxikon` | `de.maddraxikon.com` | `Hauptseite` | `https://de.maddraxikon.com/wiki/Hauptseite` |
| `john-sinclair` | `gruselroman-wiki` | `www.gruselroman-wiki.de` | `JS_Romanhefte` | `https://www.gruselroman-wiki.de/index.php?title=JS_Romanhefte` |

Serien und Hefte erhalten neben der klickbaren Quell-URL immer `source_key` und `source_record_id`. Importdaten werden nur akzeptiert, wenn die URL HTTPS verwendet, der Host exakt zum Adapter passt und die Quell-ID nicht leer ist. Damit kann ein Adapter keine Daten einer fremden Quelle in eine bestehende Serie schreiben.

## Feldzuordnung

| Zielfeld | Maddraxikon | Gruselroman-Wiki |
|---|---|---|
| Heftnummer | angeforderter Redirect `Quelle:MX{n}` | kanonische Nummer im Linkziel der Übersicht `JS_Romanhefte` |
| Titel | Infoboxfeld `Titel` | Titel des kanonischen Übersichtslinks |
| Autor(en) | Infoboxfeld `Autor` | Detailfeld `Autoren`, Fallback auf Übersicht |
| Ersterscheinung | Infoboxfeld `Erscheinungsdatum` | Detailfeld `Erscheinungsdatum`, Fallback auf Übersicht |
| Teilposition | Marker `Teil n von m` in `Besonderes` | Feld `Teil` oder Übersichtsspalte |
| Coverzeichner | `Titelbildzeichner` | `Cover`/`Coverzeichner`, Fallback auf Übersicht |
| Quell-ID | stabiler Redirect `Quelle:MX{n}` | vollständiger kanonischer Seitentitel, z. B. `JS 1000 - Das Schwert des Salomo` |
| Quell-URL | Zielseite aus der Parse-Antwort | URL des kanonischen Seitentitels |

Pflichtfelder sind Heftnummer, Titel, mindestens ein Autor, Ersterscheinungsdatum und vollständige Provenienz. Listen werden getrimmt, geleert, dedupliziert und deterministisch sortiert. Ungültige Teilpositionen oder fehlende Pflichtfelder erzeugen einen recordbezogenen Fehler; ein bereits gespeicherter Stand bleibt dabei unverändert.

## Vollständige Synchronisation

Jeder manuelle und geplante Lauf liest die aktuelle Quellliste vollständig und vergleicht jedes gemeldete Heft kanonisch mit MariaDB. Das Ergebnis ist genau eine der Kategorien `created`, `updated`, `unchanged`, `skipped` oder `failed`. Zukünftige Hefte werden zentral als `skipped` gezählt. Hefte, die nur lokal vorkommen, werden protokolliert, aber weder gelöscht noch deaktiviert.

Cover werden nur für neue Hefte oder bei fehlendem lokalem Cover geladen. Ein Coverfehler wird mit Quellenkontext gespeichert, verhindert aber nicht das Schreiben valider bibliografischer Metadaten. Beide Adapter warten standardmäßig 500 ms zwischen Wiki-Zugriffen. Transiente Netzwerk- und Rate-Limit-Fehler werden sowohl bei der vollständigen Quellliste als auch bei Heftdetails höchstens dreimal versucht; erst der letzte fehlgeschlagene Listenversuch beendet den Lauf und wird als Fehler der Phase `list` gespeichert. Parse-, Validierungs- und andere nicht transiente Fehler werden nicht wiederholt.

## Betrieb und Recovery

Der Start-Endpunkt persistiert zuerst einen `pending`-Job und antwortet mit HTTP 202. Fortschritt, Abbruchwunsch und Fehler liegen ausschließlich in MariaDB; die Adminseite pollt alle drei Sekunden. Ein Abbruch wird kooperativ vor Abrufen und vor Persistenz erkannt und endet als `cancelled`. Bei einem Backend-Neustart werden verwaiste aktive Jobs als `interrupted` markiert. Ein Retry ist ein neuer, über `retry_of_job_id` verknüpfter Vollscan und ist nur für `failed`, `cancelled` oder `interrupted` erlaubt.

Für die Erstinbetriebnahme:

1. Scheduler deaktiviert lassen.
2. Beide Adapter manuell vollständig synchronisieren.
3. Kontrollieren, dass keine Heft-Provenienz mehr repariert werden muss:
   `SELECT COUNT(*) FROM issues i JOIN series s ON s.id = i.series_id WHERE s.slug IN ('maddrax', 'john-sinclair') AND (i.source_key IS NULL OR i.source_record_id IS NULL);`
   Das Ergebnis muss `0` sein.
4. Die sechs Referenzhefte und Stichproben in der Adminansicht prüfen.
5. Serien aktivieren und einen unveränderten zweiten Lauf kontrollieren.
6. Erst danach den Wochenscheduler aktivieren.

Die Migration kann Maddrax-IDs deterministisch aus der Heftnummer übernehmen. Bei bestehenden
John-Sinclair-Heften lässt sich der vollständige kanonische Wiki-Seitentitel dagegen nicht
verlustfrei aus den alten relationalen Feldern rekonstruieren. Diese Datensätze bleiben deshalb
zunächst vollständig ohne Quellenidentität, statt eine nur teilweise abgesicherte Identität zu
erhalten. Der verpflichtende erste Vollscan ist der kontrollierte Reparaturpfad und schreibt
`source_key` und `source_record_id` gemeinsam aus der autoritativen Übersicht. Eine
Datenbank-Constraint verbietet danach halb gesetzte Quellenidentitäten.

## Referenz-Fixtures

Die Parser-Tests prüfen mindestens:

- Maddrax 1, 409 und 555
- John Sinclair 1, 1000 und 2303

Bei Mappingänderungen müssen die lokalen Fixtures bewusst aktualisiert und sämtliche
Workspace-Tests ausgeführt werden. Live-Wiki-Inhalte sind keine reproduzierbare
Testgrundlage. Die quellenspezifischen Parser und Fixtures liegen im Crate
`importer-adapters`; `importer-core` enthält ausschließlich den generischen Vertrag und die
gemeinsame Vertragsprüfung. Der Ablauf für weitere Quellen ist unter
[Neuen Import-Adapter hinzufügen](adding-import-adapter.md) dokumentiert.

Die Vertragsprüfung ruft jeden Adapter zweimal gegen lokale Fixtures auf, prüft eindeutige
positive Heftnummern, die gepinnten Referenzrecords, Pflichtfelder, Provenienz und
Idempotenz. Ein zusätzlicher MariaDB-Test persistiert alle sechs Referenzhefte und erwartet
beim zweiten Lauf ausschließlich `unchanged`.

## Abbruchnachweis

Ein manueller Abbruch schreibt `cancel_requested_at` und `cancel_requested_by` atomar in den
Importjob. Wiederholte Abbruchanforderungen verändern den zuerst protokollierten Admin nicht.
Wird das Benutzerkonto später gelöscht, bleibt der Zeitpunkt erhalten und der Fremdschlüssel
wird auf `NULL` gesetzt. Der Worker prüft den persistenten Abbruchwunsch vor externen Abrufen
und unmittelbar vor schreibenden Verarbeitungsschritten. MariaDB-Trigger für Insert und
Update ergänzen den Zeitpunkt auch dann, wenn der Akteur außerhalb des regulären
Anwendungspfads gesetzt würde. Eine `CHECK`-Constraint ist hier nicht mit dem für die
Audit-Aufbewahrung notwendigen `ON DELETE SET NULL` kombinierbar.
