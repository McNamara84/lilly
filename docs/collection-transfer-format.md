# Sammlung übertragen: CSV und JSON, Version 1

Die Seite `/collection/import` bietet Vorlagen, eine Vorschau und den Export der
gesamten eigenen Sammlung. Aktuelle Suchfilter begrenzen den Export nicht.
Alle Operationen benötigen ein aktives angemeldetes Konto und eine Onlineverbindung.
Ausstehende lokale Änderungen und Offline-Konflikte werden vor der Übertragung geprüft.

## Exemplaridentität und Konflikte

`copy_key` bezeichnet dauerhaft ein persönliches Exemplar. Eine manuell gepflegte
Vorlage kann beispielsweise `bestand-0001` verwenden; heruntergeladene Kennungen
werden unverändert übernommen. Zulässig sind 1–128 ASCII-Buchstaben, Ziffern sowie
Punkt, Unterstrich, Doppelpunkt und Bindestrich. Groß-/Kleinschreibung ist relevant.
Die Kennung ist kein Zugangsschlüssel und wird immer innerhalb des aktuellen Kontos
ausgewertet. Nutzer- oder Datenbank-IDs können kein anderes Zielkonto auswählen.

Die Heftreferenz besteht aus `series_slug` und `issue_number`. Slugs verwenden
ASCII-Buchstaben, Ziffern und Bindestriche und werden kleingeschrieben normalisiert.
Quellenreferenzen ergänzen die Identität, wenn beide Teile vorhanden sind.
Der Import erzeugt keine unbekannten Kataloghefte und aktiviert keine Serien.

| Situation | Ergebnis |
| --- | --- |
| Neue Kennung, bekanntes aktives Heft, freie Exemplarnummer | Neues Exemplar mit seinen eigenen Daten. |
| Bekannte Kennung und gleiche normalisierte Daten | Übersprungen; kein Duplikat. |
| Bekannte Kennung und andere Daten | Konflikt; der vorhandene Eintrag bleibt erhalten. |
| Bekannte Kennung eines gelöschten oder abgegebenen Exemplars | Konflikt `entry_removed`; keine unbemerkte Wiederanlage aus einer alten Datei. |
| Neue Kennung bei belegter Exemplarnummer | Konflikt `copy_number_occupied`; die Nummer wird nicht still umgebogen. |
| Identische Zeile mehrfach in derselben Datei | Nur die erste wird verarbeitet; weitere werden als Dateiduplikate ausgewiesen. |
| Gleiche Kennung mit widersprüchlichen Zeilen | Alle betreffenden Zeilen sind Konflikte. |

Editionstext allein identifiziert kein Exemplar: Zwei Kopien dürfen dieselbe Edition
haben. Die Exemplarnummer liegt zwischen 1 und 255 und ist je Nutzer/Heft eindeutig.
Beim Entfernen bleibt eine kontogebundene Löschmarkierung der Kennung erhalten.
Bei einer Kontolöschung wird auch diese Zuordnung gelöscht.

## JSON

```json
{
  "format": "lilly.collection",
  "format_version": 1,
  "exported_at": null,
  "entries": [
    {
      "copy_key": "bestand-0001",
      "series_slug": "maddrax",
      "issue_number": 1,
      "copy_number": 1,
      "status": "owned",
      "condition_grade": "Z2",
      "edition_label": "Erstauflage",
      "notes": "Eigene Zustandsnotiz"
    }
  ]
}
```

`format`, `format_version` und `entries` sind erforderlich. `exported_at` ist ein
informativer UTC-Zeitpunkt oder `null`; bei manuellen Dateien darf das Feld fehlen.
Eine leere Sammlung wird als leere Liste exportiert.

## Felder je Exemplar

| Feld | Inhalt |
| --- | --- |
| `copy_key` | Erforderliche dauerhafte Exemplar-Kennung. |
| `series_slug`, `issue_number` | Erforderliche Heftreferenz; positive ganze Heftnummer. |
| `copy_number` | Erforderliche ganze Exemplarnummer 1–255. |
| `status` | Erforderlich: `owned`, `duplicate` oder `wanted`. |
| `condition_grade` | `Z0`–`Z4`; erforderlich für `owned` und `duplicate`, bei `wanted` optional. |
| `edition_label` | Optional, höchstens 120 Unicode-Zeichen nach der bestehenden Normalisierung. |
| `notes` | Optional, höchstens 10.000 Unicode-Zeichen nach der bestehenden Normalisierung. |
| `source_key`, `source_record_id` | Optionales Paar. Gelieferte Werte müssen zur Heftreferenz passen. Unvollständige historische Katalogreferenzen werden beim Export als leeres Paar ausgegeben. |
| `series_name`, `title` | Optionale Orientierung aus dem Export. Diese Felder verändern keine Katalogmetadaten und entscheiden nicht über die Exemplaridentität. |

Fehlende optionale JSON-Felder und `null` bezeichnen keinen Wert. Notiz und Edition
werden wie bei gewöhnlicher Sammlungsbearbeitung normalisiert. Unbekannte Felder
werden abgelehnt. Das gilt insbesondere für vermeintliche Nutzer-, Preis- oder
Kontrollfelder. Fotos gehören nicht zum Sammlungsimport; eigene gespeicherte Dateien
sind in der persönlichen Datenauskunft enthalten.

## CSV

CSV verwendet UTF-8, Semikolon und eine Kopfzeile. Der Export enthält ein UTF-8-BOM;
beim Import wird das Format mit und ohne BOM akzeptiert. Die Pflichtspalten heißen:

```text
format_version;copy_key;series_slug;issue_number;copy_number;status
```

Die optionalen Spalten entsprechen den obigen Feldern. Der Export ergänzt außerdem
`text_encoding`. Die Reihenfolge der Spalten ist frei; jede darf nur einmal vorkommen.
Optionale leere CSV-Zellen entsprechen `null`. Anführungszeichen werden verdoppelt;
Trennzeichen und Zeilenumbrüche innerhalb einer Zelle werden korrekt zitiert.

```csv
format_version;copy_key;series_slug;issue_number;copy_number;status;condition_grade;edition_label;notes
1;bestand-0001;maddrax;1;1;owned;Z2;Erstauflage;Eigene Zustandsnotiz
```

In einer manuellen Datei fehlt `text_encoding` oder hat den Wert `plain`. Dann
werden Texte wörtlich eingelesen. Der Export verwendet `spreadsheet-safe-v1`:
Textzellen mit führendem `=`, `+`, `-`, `@`, Apostroph oder ASCII-Leerraum erhalten
ein zusätzliches führendes Apostroph. Beim Einlesen dieses ausdrücklich markierten
Formats wird genau dieses zusätzliche Zeichen wieder entfernt. Damit werden
Formeln beim Öffnen in Tabellenprogrammen entschärft, ohne echte Apostrophe oder
Notizen beim Roundtrip zu verlieren. Version, Heft- und Exemplarnummer sind Zahlen.

## Vorschau, Fehler und Wiederholungen

Voreinstellung: maximal 10 MiB und 10.000 Datensätze je Datei. Der Betreiber kann
`DATA_IMPORT_MAX_BYTES` und `DATA_IMPORT_MAX_ROWS` anpassen. Eine Überschreitung wird
vollständig abgelehnt; die Datei wird nicht abgeschnitten.

Die Vorschau verändert die Sammlung nicht und gilt 30 Minuten. Ein neuer Upload
ersetzt eine noch unbestätigte vorherige Vorschau. Erst die ausdrückliche Bestätigung
reiht den Auftrag ein. Bei der Verarbeitung werden aktuelle Daten und Berechtigungen
erneut geprüft. Zwischenzeitliche Änderungen können neue Konflikte ergeben.

Lesbare ungültige Zeilen verhindern nicht die Übernahme anderer gültiger Zeilen.
Strukturell unlesbares JSON/CSV, ungültiges UTF-8, fremde Spalten und unbekannte
Formatversionen verhindern dagegen eine verlässliche Vorschau und werden als
Dateifehler gemeldet. Die Ergebnisse nennen Datensatznummer, Kennung, Status und
Fehlercode; lange private Freitexte werden nicht in Fehlermeldungen kopiert.

Jede erfolgreiche neue Zeile und ihr Ergebnisbeleg werden gemeinsam committed.
Ein Infrastrukturfehler unterbricht den Auftrag mit erkennbarem Teilstand und
markiert noch ausstehende neue Zeilen als fehlgeschlagen. Dieselbe Datei kann erneut
hochgeladen werden: Bereits übernommene Exemplare werden anhand ihrer Kennung erkannt.
Eine erneute Bestätigung desselben Auftrags startet keinen zweiten Import.

Die Auftragskennung steht nach einer Vorschau in der Seitenadresse. Ein Neuladen
holt den eigenen Auftrag erneut vom Server. Nach erfolgreicher Verarbeitung wird
der lokale Offline-Snapshot aktualisiert. Scheitert nur dieser lokale Schritt,
bleibt der Serverimport erfolgreich und die Oberfläche zeigt den Unterschied an.

Rohdateien liegen ausschließlich im privaten Transfer-Volume und werden nach
Abschluss, Abbruch oder Ablauf entfernt. Löschfehler werden wiederholt. Operative
Auftragsdaten werden spätestens nach sieben Tagen bereinigt.

API-Operationen und Fehlerantworten stehen im [API-Handbuch](api.md) und im
[generierten OpenAPI-Vertrag](openapi.json).
