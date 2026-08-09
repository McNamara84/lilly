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

Cover werden nur für neue Hefte oder bei fehlendem lokalem Cover geladen. Ein Coverfehler wird mit Quellenkontext gespeichert, verhindert aber nicht das Schreiben valider bibliografischer Metadaten. Beide Adapter warten standardmäßig 500 ms zwischen Wiki-Zugriffen; transiente Detailfehler werden höchstens dreimal versucht.

## Betrieb und Recovery

Der Start-Endpunkt persistiert zuerst einen `pending`-Job und antwortet mit HTTP 202. Fortschritt, Abbruchwunsch und Fehler liegen ausschließlich in MariaDB; die Adminseite pollt alle drei Sekunden. Ein Abbruch wird kooperativ vor Abrufen und vor Persistenz erkannt und endet als `cancelled`. Bei einem Backend-Neustart werden verwaiste aktive Jobs als `interrupted` markiert. Ein Retry ist ein neuer, über `retry_of_job_id` verknüpfter Vollscan und ist nur für `failed`, `cancelled` oder `interrupted` erlaubt.

Für die Erstinbetriebnahme:

1. Scheduler deaktiviert lassen.
2. Beide Adapter manuell vollständig synchronisieren.
3. Die sechs Referenzhefte und Stichproben in der Adminansicht prüfen.
4. Serien aktivieren und einen unveränderten zweiten Lauf kontrollieren.
5. Erst danach den Wochenscheduler aktivieren.

## Referenz-Fixtures

Die Parser-Tests prüfen mindestens:

- Maddrax 1, 409 und 555
- John Sinclair 1, 1000 und 2303

Bei Mappingänderungen müssen die lokalen Fixtures bewusst aktualisiert und sämtliche `importer-core`-Tests ausgeführt werden. Live-Wiki-Inhalte sind keine reproduzierbare Testgrundlage.
