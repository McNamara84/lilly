# LILLY – UI/UX-Konzept

## Listing Inventory for Lovely Little Yellowbacks

**Version 1.0** | Stand: 06. März 2026 | Autor: Holger Ehrmann

---

## Inhaltsverzeichnis

1. [Design-Philosophie](#1-design-philosophie)
2. [Maschinenlesbare Spezifikationen](#2-maschinenlesbare-spezifikationen)
3. [Visual Identity](#3-visual-identity)
4. [Layout und Navigation](#4-layout-und-navigation)
5. [Kernkomponenten](#5-kernkomponenten)
6. [Screens und Seitenstruktur](#6-screens-und-seitenstruktur)
7. [Interaktions- und Animationskonzept](#7-interaktions--und-animationskonzept)
8. [Responsive Strategie](#8-responsive-strategie)
9. [Barrierefreiheit](#9-barrierefreiheit)
10. [Copilot-Hinweise für Entwickler](#10-copilot-hinweise-für-entwickler)

---

## 1. Design-Philosophie

LILLY folgt der Ästhetik des **Glassmorphism**: halbtransparente, milchig-unscharfe Oberflächen, die über einem farbigen Hintergrund schweben. Diese Designsprache wurde bewusst gewählt, weil Heftroman-Cover kräftige, farbenfrohe Illustrationen besitzen – auf dem glasartigen, leicht abgedunkelten Hintergrund kommen diese besonders gut zur Geltung.

**Kernprinzipien:**

- **Cover first:** Die Titelbilder der Heftromane sind das visuelle Herzstück. Jede Designentscheidung priorisiert deren Sichtbarkeit und Wirkung.
- **Glassmorphism mit Maß:** Blur-Effekte werden für Container, Navigation und modale Elemente eingesetzt, nicht für jeden Textblock. Content-Lesbarkeit geht vor Effekt.
- **Adaptives Theming:** Die App respektiert die Systemeinstellung (`prefers-color-scheme`) und bietet zusätzlich einen manuellen Toggle (Light / Dark / System). Dark Mode ist die optimale Ansicht für Cover-Präsentation, Light Mode für längeres Lesen.
- **Thumb-friendly Mobile UX:** Navigation über Bottom Navigation Bar, große Touch-Targets (min. 44px), Gesten für Schnellaktionen.
- **Sammlerfreude:** Micro-Animations beim Hinzufügen zur Sammlung, visuelle Fortschrittsbalken, und ein Sammlungsgrid, das den Besitz stolz visualisiert.

---

## 2. Maschinenlesbare Spezifikationen

Das UI/UX-Konzept ist in drei maschinenlesbare JSON-Dateien aufgeteilt, die von AI-Assistenten wie GitHub Copilot direkt interpretiert werden können:

| Datei | Inhalt | Zweck |
|---|---|---|
| `design-tokens.json` | Farben, Typografie, Abstände, Glassmorphism-Parameter, Animationen, Breakpoints | Fundament des Design-Systems. Wird in Tailwind-Config und CSS Custom Properties überführt. |
| `components.json` | Komponentenspezifikationen mit Props, Varianten, visuellem Verhalten, Dateistruktur | Blaupause für jede Svelte-Komponente. Copilot kann daraus Boilerplate generieren. |
| `screens.json` | Seitenstruktur, Routing, Abschnitte pro Screen, Datenquellen, Interaktionen | Definiert jede Seite der App mit ihren Bestandteilen und Verhalten. |

**Verwendung mit Copilot / AI-Assistenten:**

Beim Entwickeln einer neuen Komponente oder Seite sollte der AI-Assistent die relevante JSON-Datei als Kontext erhalten. Beispiel-Prompt: "Erstelle die Svelte-Komponente CoverCard basierend auf der Spezifikation in components.json und den Design Tokens aus design-tokens.json."

---

## 3. Visual Identity

### 3.1 Farbsystem

Das Farbsystem ist in `design-tokens.json` unter `color` vollständig spezifiziert.

**Primärfarbe: Cyan (brand.primary)**
Die Cyan-Palette (50–950) bildet die Markenidentität. Der Basiswert `#06b6d4` (500) wird für Akzente, aktive Zustände und CTAs verwendet. Auf dunklem Hintergrund erzeugt Cyan einen unverwechselbaren, technisch-modernen Look, der sich klar von den warmen, bunten Heftroman-Covern abhebt und diese dadurch visuell hervorhebt.

**Akzentfarbe: Sky Blue (brand.accent)**
Sky Blue wird sparsam für sekundäre Highlights und Links eingesetzt und ergänzt die Primärfarbe harmonisch.

**Zustandsfarben (condition.Z0–Z5)**
Eine intuitive Grün-zu-Rot-Skala kodiert den Heftzustand von neuwertig (Grün) bis defekt (Rot). Diese Farben werden als Badges auf Cover-Karten und als Chips im Zustandswähler verwendet.

**Sammlungsstatus-Farben (collection_status)**
Vier Farben kennzeichnen den Besitzstatus: Cyan für vorhanden, Lila für doppelt/tauschbar, Amber für gesucht, transparent für fehlend.

### 3.2 Typografie

**Primärschrift: Inter**

Inter ist eine speziell für Bildschirme optimierte Open-Source-Schrift mit exzellentem Support für tabulare Zahlen (`font-feature-settings: "tnum"`), was für die zahlenlastigen Sammlungsstatistiken ideal ist. Die Schrift wird als Self-Hosted-Font mit dem Frontend gebündelt (kein externer CDN-Aufruf zur Laufzeit), mit einem System-Font-Stack als Fallback.

Die typografische Hierarchie (h1–h4, body, caption, label, stat_number) ist in `design-tokens.json` unter `typography.styles` definiert.

### 3.3 Glassmorphism

Vier Glassmorphism-Varianten sind definiert (siehe `design-tokens.json` unter `glassmorphism`):

- **card:** Standard-Container für Inhalte. 16px Blur, subtiler Border, Schatten.
- **card_elevated:** Für modale Elemente (Detail-Sheets, Command Palette). 24px Blur, stärkerer Schatten.
- **navbar:** Top Bar und Bottom Navigation. 20px Blur, unterer/oberer Border.
- **sidebar:** Desktop-Sidebar. 20px Blur, rechter Border.

Der Blur-Effekt wird über `backdrop-filter: blur()` realisiert. Die Hintergrund- und Border-Farben passen sich über CSS Custom Properties automatisch an Light/Dark Mode an.

### 3.4 Ikonografie

LILLY verwendet **Lucide Icons** (Open Source, MIT-Lizenz), die bereits als Svelte-Komponenten verfügbar sind (`lucide-svelte`). Lucide bietet einen konsistenten, clean Style mit 24px-Standardgröße und 1.5px Strichstärke, der hervorragend zur Glassmorphism-Ästhetik passt.

---

## 4. Layout und Navigation

### 4.1 App Shell

Die App verwendet ein adaptives Shell-Layout, das sich am Breakpoint `lg` (1024px) grundlegend ändert:

**Desktop (≥ 1024px):**
```
┌──────────────────────────────────────────────────────┐
│  TopBar (sticky, glassmorphism)           🔍 🌓 🔔 👤│
├────────────────┬─────────────────────────────────────┤
│                │                                     │
│  Sidebar       │    Main Content Area                │
│  (glass,       │    (scrollable)                     │
│  280px / 72px, │                                     │
│  collapsible)  │    max-width: 1280px                │
│                │    centered                         │
│                │                                     │
│  ┌──────┐      │                                     │
│  │☰ / ◀│      │                                     │
│  └──────┘      │                                     │
└────────────────┴─────────────────────────────────────┘
```

**Mobil (< 1024px):**
```
┌──────────────────────────┐
│  TopBar (sticky, glass)  │
├──────────────────────────┤
│                          │
│   Main Content Area      │
│   (scrollable,           │
│    full width)           │
│                          │
│                          │
├──────────────────────────┤
│  BottomNav (fixed, glass)│
│  🏠  📚  🔄  💬  👤     │
└──────────────────────────┘
```

### 4.2 Navigationsstruktur

Die fünf primären Navigationsziele sind in Sidebar und BottomNav identisch:

| Icon | Label | Route | Funktion |
|---|---|---|---|
| LayoutGrid | Übersicht | `/` | Übersicht, Statistiken, Schnellzugriff |
| Library | Sammlung | `/collection` | Eigene Sammlung verwalten |
| ArrowLeftRight | Tausch | `/trades` | Tausch-Matching und aktive Tausche |
| MessageCircle | Nachrichten | `/messages` | Internes Nachrichtensystem |
| User | Profil | `/profile` | Eigenes Profil und Einstellungen |

Die Sidebar enthält zusätzlich den sekundären Eintrag **Entdecken** (`/explore`, Icon: Search), der unterhalb der fünf Haupteinträge platziert ist. In der BottomNav ist Entdecken nicht enthalten – der Bereich ist mobil über die globale Suche (Command Palette) erreichbar.

Für Nutzer mit der Rolle `admin` wird unterhalb der sekundären Einträge ein weiterer Abschnitt **Admin** angezeigt (Icon: Shield, Route: `/admin`). Dieser Eintrag ist für normale Nutzer nicht sichtbar. In der BottomNav wird der Admin-Bereich nicht angezeigt — auf Mobilgeräten ist er über die Sidebar oder einen Admin-Link im Profil-Menü erreichbar.

### 4.3 Globale Suche (Command Palette)

Über `Cmd+K` / `Ctrl+K` oder das Suchsymbol in der TopBar öffnet sich eine zentrierte, glassmorphe Suchbox, die serienübergreifend nach Heften, Serien und Sammlern suchen kann. Das Konzept folgt dem etablierten Command-Palette-Pattern (bekannt aus VS Code, GitHub, Linear).

---

## 5. Kernkomponenten

Alle Komponenten sind in `components.json` vollständig spezifiziert. Hier eine Zusammenfassung der wichtigsten:

### 5.1 CoverCard

Die CoverCard ist das visuelle Herzstück von LILLY. Sie zeigt ein Heftroman-Cover im Format 2:3 mit kontextuellen Overlays:

- **Heftnummer** (oben links, Glass-Pill, Monospace-Font)
- **Zustandsbadge** (oben rechts, farbig nach Z0–Z5)
- **Statusindikator** (unten, farbig nach owned/duplicate/wanted/missing)

Hover-Effekt: 5% Scale-up mit Glow-Schatten (Cyan). Fehlende Hefte werden mit 40% Opacity und gestricheltem Rand dargestellt, was visuell sofort die Lücken in der Sammlung zeigt.

### 5.2 CoverGrid

CSS-Grid mit `auto-fill` und `minmax(140px, 1fr)` für vollautomatische responsive Spaltenanzahl. Virtualisiertes Scrolling für Serien mit 500+ Heften. Lazy-Loading der Cover-Bilder via Intersection Observer.

### 5.3 ConditionChips

Horizontale Chip-Gruppe für die Zustandsauswahl Z0–Z5. Jeder Chip ist in der jeweiligen Zustandsfarbe gehalten (von Grün bis Rot). Ausgewählter Chip: gefüllt mit Glow. Nicht ausgewählt: Glass-Hintergrund, gedämpfter Text.

### 5.4 SeriesProgressBar

Animierter Fortschrittsbalken mit Cyan-zu-Blue-Gradient. Zeigt "437 von 620 — 70,5%" im Body-Small-Stil. Lila-Segment am Ende für doppelte Hefte. Spring-Animation beim Einblenden.

### 5.5 TradeMatchCard

Glassmorphism-Karte mit Zwei-Spalten-Layout: "Du bietest" (links) und "Du erhältst" (rechts), getrennt durch einen vertikalen Glass-Divider mit ArrowLeftRight-Icon. Kreisförmige Match-Score-Anzeige.

### 5.6 IssueDetailSheet

Kontextabhängiges Detail-Panel: auf Mobil als Bottom Sheet (von unten hochziehbar, max 85% Viewport-Höhe), auf Desktop als Seitenpanel (von rechts einfahrend, 420px breit). Enthält alle Informationen und Aktionen zu einem einzelnen Heft.

---

## 6. Screens und Seitenstruktur

Alle Screens sind in `screens.json` vollständig spezifiziert. Hier eine Übersicht der Seitenstruktur:

### 6.1 Sitemap

```
/                         Dashboard (auth)
/collection               Sammlung mit CoverGrid (auth)
/collection/add           Hefte schnell hinzufügen (auth)
/series                   Serienübersicht – alle aktiven Serien (public)
/series/[slug]            Seriendetail mit Heft-Grid (public)
/issues/[id]              Heftdetail (public)
/trades                   Tausch-Hub: Matches + Aktive (auth)
/trades/[id]              Einzelner Tausch (auth)
/messages                 Nachrichtenübersicht (auth)
/messages/[id]            Nachrichtenthread (auth)
/explore                  Entdecken: Serien, Stats, Sammler (public)
/users/[name]             Öffentliches Sammlerprofil (public)
/profile                  Eigenes Profil + Settings (auth)
/admin                    Admin-Bereich Einstieg (admin, redirect zu /admin/series)
/admin/series             Serien-Verwaltung: alle Serien inkl. inaktive (admin)
/admin/import             Import starten + Import-Historie (admin)
/admin/import/[id]        Import-Detail: Fortschritt + Prüfansicht (admin)
/login                    Anmeldung
/register                 Registrierung
/privacy                  Datenschutzerklärung
```

### 6.2 Schlüssel-Screens im Detail

**Dashboard (`/`):** Vier StatsCards als Übersichts-KPIs, Fortschrittsbalken pro Serie, Top-3-Tauschvorschläge, Aktivitäts-Timeline. Zwei-Spalten-Layout auf Desktop.

**Sammlung (`/collection`):** FilterBar (sticky) + CoverGrid mit Infinite Scroll. Floating Action Button zum Hinzufügen. Long-Press für Multi-Select und Batch-Operationen.

**Hefte hinzufügen (`/collection/add`):** Serienauswahl, dann Nummern-Grid mit farbiger Kennzeichnung. Optimiert für schnelles Batch-Hinzufügen: Tap zum Togglen, Bereich-Auswahl für zusammenhängende Nummern.

**Tausch (`/trades`):** Zwei-Tab-Ansicht mit "Vorschläge" (TradeMatchCards) und "Aktive Tausche". Semi-automatisches Matching wird visuell durch Match-Score-Ringe dargestellt.

**Anmelden (`/login`):** Zentrierte Glassmorphism-Karte auf animiertem Gradient-Mesh-Hintergrund. E-Mail/Passwort-Formular + OAuth-Buttons (Google, GitHub).

**Serien-Übersicht (`/series`):** Cards pro aktive Serie mit Cover des ersten Hefts, Name, Genre, Heftanzahl und Status-Badge. Öffentlich zugänglich, keine Auth nötig.

**Serien-Detail (`/series/[slug]`):** Header mit Serien-Metadaten (Name, Verlag, Genre, Frequency, Status). Darunter paginiertes CoverGrid aller Hefte der Serie.

### 6.3 Admin-Screens

Der Admin-Bereich ist unter `/admin/` als eigener Routenpräfix vom Nutzer-Bereich getrennt. Zugang nur für authentifizierte Nutzer mit Rolle `admin`. Das Layout enthält eine dedizierte Admin-Navigation (Sidebar-Eintrag oder TopBar-Link, nur für Admins sichtbar).

**Serien-Verwaltung (`/admin/series`):** Tabelle aller Serien (inkl. inaktive) mit Spalten: Name, Status (running/completed/cancelled), Aktiv-Badge (ja/nein), Anzahl importierte Hefte, letzter Import. Pro Serie: Toggle-Button zum Aktivieren/Deaktivieren.

**Import starten (`/admin/import`):** Dropdown zur Auswahl des Import-Adapters (aus API: Name + Version). Button „Import starten". Darunter: Import-Historie als Tabelle (Datum, Adapter, Status, Dauer, importierte Hefte, gestartet von).

**Import-Detail & Prüfansicht (`/admin/import/[id]`):**
- *Während Import läuft:* Fortschrittsbalken mit „243 / 620 Hefte importiert", Status-Badge (pending → running → completed/failed). Automatisches Polling alle 3 Sekunden.
- *Nach Abschluss:* Zusammenfassung (Gesamtzahl, Dauer, Fehler). Paginierte Liste der importierten Hefte (Nummer, Titel, Autor, Datum). Klick auf ein Heft öffnet Detail mit Cover-Vorschau, allen Metadaten und Link zum Wiki-Quelleneintrag. Button „Serie aktivieren" (wenn Serie noch inaktiv).

---

## 7. Interaktions- und Animationskonzept

### 7.1 Micro-Interactions

- **Heft zur Sammlung hinzufügen:** Spring-Animation (Scale 0.95 → 1.05 → 1.0) + kurzer Cyan-Glow + Toast-Notification "Heft #123 hinzugefügt ✓"
- **Zustand ändern:** Chip füllt sich mit Farbe, Ripple-Effekt vom Touchpunkt aus
- **Tausch vorschlagen:** CoverCards der angebotenen Hefte "fliegen" visuell zur Mitte (300ms)
- **Nachricht senden:** Message-Bubble federt von unten ein (spring easing)
- **Sammlung scrollen:** Cover-Bilder faden via Intersection Observer sanft ein (opacity 0 → 1, 200ms)

### 7.2 Seitenübergänge

SvelteKit Page Transitions mit Crossfade: ausgehende Seite fadet aus (200ms), eingehende Seite fadet ein (250ms). Detail-Ansichten (Issue, Trade) nutzen shared element transitions wo möglich (Cover-Bild animiert von Grid-Position zu Detail-Position).

### 7.3 Loading States

- **Skeleton Loading:** CoverGrid zeigt 12 Platzhalter-Karten mit Shimmer-Animation (linearer Gradient, der von links nach rechts wandert, 1.5s Loop)
- **Pull to Refresh:** Auf Mobil, Cyan-farbiger Spinner im TopBar-Bereich
- **Infinite Scroll:** Subtle pulsierender Ladeindikator am unteren Rand des Grids

---

## 8. Responsive Strategie

### 8.1 Breakpoints

| Breakpoint | Breite | Layout-Änderungen |
|---|---|---|
| **< 640px** (sm) | Telefon | 2–3 Cover-Spalten, kompakte Stats, einzeilige FilterBar mit horizontalem Scroll |
| **640–767px** (sm–md) | Großes Telefon | 3–4 Cover-Spalten |
| **768–1023px** (md–lg) | Tablet | 4–5 Cover-Spalten, erweiterte FilterBar |
| **≥ 1024px** (lg) | Desktop | Sidebar erscheint, BottomNav verschwindet, 5–8 Cover-Spalten, Zwei-Spalten-Layouts |
| **≥ 1280px** (xl) | Großer Desktop | Max-Width-Container greift (1280px), zentrierter Content |

### 8.2 Touch vs. Pointer

- **Touch-Geräte:** Größere Touch-Targets (min. 44×44px), Swipe-Gesten, Long-Press für Kontextaktionen, Bottom Sheet statt Dropdown
- **Pointer-Geräte:** Hover-Effekte, Tooltips, Rechtsklick-Kontextmenü, Side Panel statt Bottom Sheet, Keyboard Shortcuts (Cmd+K etc.)

---

## 9. Barrierefreiheit

- **WCAG 2.1 Level AA** als Mindestziel
- **Farbkontrast:** Alle Text-auf-Glass-Kombinationen erfüllen ein Kontrastverhältnis von mindestens 4.5:1. Glassmorphism-Hintergründe müssen ausreichend opak sein.
- **Fokus-Management:** Sichtbarer Fokusring (2px brand.primary, offset 2px) auf allen interaktiven Elementen. Focus-Trap in Modalen/Sheets.
- **Semantisches HTML:** Korrekte Heading-Hierarchie, ARIA-Labels für Icon-Buttons, role="navigation" für Nav-Bereiche.
- **Screenreader:** Cover-Bilder erhalten alt-Text: "Cover von {Serienname} #{Nummer}: {Titel}". Status-Overlays werden als aria-label kommuniziert.
- **Reduzierte Bewegung:** Bei `prefers-reduced-motion: reduce` werden alle Animationen auf sofortige Zustandswechsel reduziert.
- **Tastaturnavigation:** Alle Funktionen per Tastatur erreichbar. CoverGrid unterstützt Pfeil-Tasten-Navigation.

---

## 10. Copilot-Hinweise für Entwickler

### 10.1 Dateistruktur der Spezifikation

```
docs/
├── uxdesign.md                ← Dieses Dokument (Übersicht, Richtlinien)
├── design-tokens.json         ← Farben, Fonts, Spacing, Glassmorphism
├── components.json            ← Komponentenspezifikationen
└── screens.json               ← Screen-/Seitenspezifikationen
```

### 10.2 Wie Copilot diese Dateien nutzen soll

Beim Erstellen einer neuen Svelte-Komponente:

1. Lade `design-tokens.json` für Farben, Abstände und Glassmorphism-Werte.
2. Lade `components.json` und suche die Spezifikation der gewünschten Komponente.
3. Implementiere die Komponente gemäß der Props, Varianten und visuellen Beschreibung.
4. Verwende Skeleton UI-Komponenten wo passend und ergänze Tailwind CSS für Custom-Styling.
5. Beachte die Responsive-Strategie und teste beide Modi (Light/Dark).

Beim Erstellen einer neuen Seite/Route:

1. Lade `screens.json` und suche die Spezifikation der gewünschten Seite.
2. Beachte `auth_required`, `layout`, `sections` und `data_source` Felder.
3. Komponiere die Seite aus den in `sections` referenzierten Komponenten.
4. Implementiere die in `interactions` beschriebenen Nutzerinteraktionen.

### 10.3 Tailwind CSS Custom Theme

Die Design Tokens müssen in die `tailwind.config.js` überführt werden. Hier das Mapping:

- `color.brand.*` → `theme.extend.colors.brand.*`
- `color.condition.*` → `theme.extend.colors.condition.*`
- `color.collection_status.*` → `theme.extend.colors.status.*`
- `glassmorphism.*` → Custom CSS Utility Classes (`.glass-card`, `.glass-elevated`, `.glass-nav`)
- `typography.font_family.*` → `theme.fontFamily.*`
- `shadows.glow*` → `theme.extend.boxShadow.glow*`
- `animation.*` → `theme.extend.transitionDuration.*` und `theme.extend.transitionTimingFunction.*`

### 10.4 CSS Custom Properties für Theming

Glassmorphism-Farben werden als CSS Custom Properties definiert, die sich je nach Theme ändern:

```css
/* Dark Mode (default or prefers-color-scheme: dark) */
:root[data-theme="dark"] {
  --glass: rgba(255, 255, 255, 0.05);
  --glass-border: rgba(255, 255, 255, 0.10);
  --glass-hover: rgba(255, 255, 255, 0.08);
  --surface-base: #0a0a0f;
  --surface-raised: #12121a;
  --text-primary: #f1f5f9;
  --text-secondary: #94a3b8;
}

/* Light Mode */
:root[data-theme="light"] {
  --glass: rgba(255, 255, 255, 0.60);
  --glass-border: rgba(255, 255, 255, 0.40);
  --glass-hover: rgba(255, 255, 255, 0.75);
  --surface-base: #f8fafc;
  --surface-raised: #ffffff;
  --text-primary: #0f172a;
  --text-secondary: #475569;
}
```

---

*Ende des UI/UX-Konzepts*