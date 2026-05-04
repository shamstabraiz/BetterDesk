# Plan operacyjny: Agent Client + rozbudowa panelu web

**Data:** 2026-04-18
**Kontekst:** Agent client obecnie to 6 plików Rust (rejestracja + sysinfo + stub-czat). Brak zdalnego pulpitu, brak WS do czatu, brak trybu użytkownika vs operatora. Panel web ma `help-requests.ejs` + API oraz slide-over `device-detail` — wymagają rozbudowy.

## Założenia architektoniczne

1. **Agent client = TRAY-FIRST aplikacja.** Menu główne w zasobniku systemowym. Okno GUI otwiera się tylko dla konkretnych funkcji (czat, help-request). Brak głównego sidebara.
2. **Tryby uprawnień:**
   - **User mode** (zwykły użytkownik): "Poproś o pomoc", "Czat", "Pokaż ID".
   - **Admin mode** (OS administrator / root): wszystko powyżej + "Ustawienia", "Zamknij agenta".
   - Detekcja: `IsUserAnAdmin()` (Win32), `geteuid() == 0` (Unix).
3. **Szyfrowanie E2E zachowane** — chat w agencie używa istniejącego `chatCrypto.js` (P-256 ECDH + AES-256-GCM) z panelu web, tylko port Rust.
4. **Kanały komunikacji:**
   - **REST HTTP** — rejestracja, help-request, sysinfo upload (już jest).
   - **WebSocket `/ws/agent/{device_id}`** (NOWE) — chat, powiadomienia, heartbeat, remote session signaling.
   - **Relay TCP/WS** (Faza 4) — video/audio/input dla zdalnego pulpitu.
5. **Bezpieczeństwo:** wszystkie WS z JWT token (z rejestracji), TLS wymagane w produkcji, rate-limit IP, audit log każdej akcji.

---

## Faza 1: Fundament UX + powiadomienia (TA SESJA)

**Cel:** Przeorganizować agent UI + dodać dzwonek powiadomień w panelu web.

### Agent Client
- [x] ~~Obecne okno z sidebarem~~ → **usunięte**. Nowe: minimalne okno "o programie" + pełne menu w trayu.
- [x] Detekcja admina w Rust (`is_os_admin()`).
- [x] Tray menu:
  - **Zawsze:** Pokaż ID | Poproś o pomoc | Czat | Sprawdź połączenie | ───
  - **Admin only:** Ustawienia | Zamknij agenta
  - **Autostart:** domyślnie włączony (już jest)
- [x] Pełny ekran tylko dla: `/chat` (z brandingiem konsoli), `/help` (formularz), `/settings` (tylko admin).
- [x] Rozmiar okna zmniejszony do 480×560 (chat/help/settings), brak rozbudowanego menu bocznego.

### Web panel
- [x] **Dzwonek powiadomień** w navbar (obok `refresh-btn`):
  - Badge z licznikiem nieprzeczytanych help-requestów
  - Dropdown z ostatnimi 10 powiadomieniami
  - Klik → przejście do `/help-requests` + oznaczenie jako przeczytane
- [x] Endpoint `GET /api/bd/notifications` (z filtrem `unread_only`).
- [x] Endpoint `POST /api/bd/notifications/:id/read`.
- [x] Socket.IO event `help-request` już istnieje — podłączyć do dzwonka (real-time).
- [x] i18n klucze `notifications.*` (EN/PL/ZH).

---

## Faza 2: Device Management Modal (NASTĘPNA SESJA)

**Cel:** Zastąpić slide-over `device-detail` pełnoekranowym modalem z rozbudowanym UI inspirowanym nVision.

### Backend
- Rozszerzony endpoint `GET /api/peers/:id` o:
  - `hardware` (CPU, GPU, RAM, disks) — z inventory
  - `software` (zainstalowane aplikacje) — z inventory
  - `services` (uruchomione usługi systemowe) — z agent nowy endpoint
  - `processes` (aktywne procesy) — z agent nowy endpoint
  - `events` (logi Windows/journalctl) — z agent nowy endpoint
  - `usage_stats` (czas uruchomienia aplikacji) — z agent activity tracker
- Endpoint `POST /api/peers/:id/rename` (zmiana przyjaznej nazwy).
- Endpoint `POST /api/peers/:id/files/browse` — proxy do agent file browser.
- Endpoint `POST /api/peers/:id/terminal/execute` — proxy do agent terminal.

### Frontend
- Nowy komponent `device-modal.js` — pełnoekranowy modal (bez slide-over).
- Zakładki wzorowane na nVision:
  - **General** — hero z nazwą/zdjęciem + status + szybkie akcje (Remote, Chat, Terminal, Files)
  - **Activity** — wykres użycia aplikacji (czas pracy)
  - **Screenshots** — galeria screenshotów (z agent screenshot capability)
  - **Events** — log zdarzeń systemowych
  - **DataGuard** — polityki DLP (już jest moduł)
  - **Blockades** — bany/ograniczenia (już jest moduł)
  - **Settings** — config per-device
- Przycisk "Remote access" → otwiera `/remote/:id` (web viewer — już jest).
- Przycisk "Files" → panel przeglądania plików z upload/download.
- Przycisk "Terminal" → xterm.js terminal w nowym oknie.

### Agent — nowe capabilities
- `system_services` — `Get-Service` (Win) / `systemctl list-units` (Linux)
- `system_processes` — `Get-Process` / `ps aux`
- `system_events` — `Get-EventLog` / `journalctl`
- `file_browser` — list/read/write/delete z path traversal guard
- `screenshot_capture` — JPEG snapshot on demand
- `activity_tracker` — śledzenie czasu użycia aplikacji (background thread)
- `terminal_session` — PTY (unix) / ConPTY (Windows)

**Implementacja:** agent-client pobiera większość z natywnego `yomie-agent` (Go) — portować moduły do Rust lub uruchamiać jako sidecar subprocess.

---

## Faza 3: Chat z brandingiem + E2E (FAZA 3)

**Cel:** Pełny czat między użytkownikami + operatorami, E2E, uploady plików.

### Backend
- Endpoint WS `/ws/chat/agent/:device_id` — WS z JWT.
- Reuse `chatRelay.js` (już istnieje).
- Endpoint `GET /api/chat/contacts` — lista operatorów (top) + pracowników z organizacji.
- Endpoint `POST /api/chat/attachment` — upload pliku (max 50 MB, encrypted blob).
- Endpoint `GET /api/chat/attachment/:id` — download.

### Agent
- `chat.rs` — WS client (tokio-tungstenite), P-256 keypair (ring crate), message encrypt/decrypt.
- UI: lewa kolumna z kontaktami (operatorzy na górze, oznaczeni ikoną), prawa z historią.
- Upload drag-and-drop + paste obrazka.
- Branding: wykorzystuje `GET /api/config/branding` (logo, primary color, nazwa firmy).

### Web panel
- Istniejąca strona `chat.ejs` — rozszerzyć o:
  - Lista agent-ów online (z device_id + hostname)
  - Wybór odbiorcy: operator, kolega z firmy, klient (device)
  - Upload plików
- Czat E2E już zaimplementowany (Phase 2 — chatCrypto.js).

---

## Faza 4: Remote Desktop Core (FAZA 4 — NAJWIĘKSZA)

**Cel:** Pełny zdalny pulpit z multi-monitor, audio, wielokierunkowy clipboard, file transfer, supervised+unattended mode.

### Agent — nowe crates
- `scrap` / `xcap` — screen capture (Win+Linux+macOS)
- `openh264` — video encode (fallback: JPEG)
- `cpal` — audio capture
- `enigo` — input inject (już w MGMT)
- `arboard` — clipboard sync
- `tokio-tungstenite` — WS do relay
- `protobuf` — protokół (reuse z yomie-server/proto)

### Agent — nowe moduły
- `remote/capture.rs` — capture loop (30-60 FPS adaptive)
- `remote/encoder.rs` — H.264 encoder + VP9 fallback
- `remote/audio.rs` — audio capture + Opus encode
- `remote/input.rs` — input injection receiver
- `remote/session.rs` — session manager (negocjacja codec, quality, multi-monitor)
- `remote/consent.rs` — **supervised mode:** popup JAK NA ZDJĘCIU 1
  - Przyciski: Klawiatura, Schowek, Audio, Transfer plików, Nagrywanie, Kamera, Blokada
  - Imię i nazwisko operatora + nick z konsoli web
  - Countdown "Automatyczna zgoda za X s" (opcjonalne)
- **Unattended mode:** konfigurowalny w `/settings`, akceptacja domyślna (bez popupu)

### Web viewer
- Już istnieje `remote.ejs` (web remote client). Rozszerzyć o:
  - Dropdown wyboru monitora
  - Przycisk "Request audio" → agent popup
  - Přycisk "Start recording" → encoder zapisuje MP4
  - Wskaźnik supervised/unattended (ikonka shield)

### Protokół
- Signal: agent rejestruje ws://server/ws/agent/:id (push connection)
- Operator łączy się przez web remote → server broadcastuje `connection_request` do agenta
- Agent: jeśli supervised → pokazuje popup → czeka na `accept` z mapą uprawnień → wysyła `ready`
- Agent: jeśli unattended → akceptuje od razu
- Relay: obie strony łączą się do `/relay/:session_id`, server forwarduje bajty (istniejący `relay/`)

### User identity
- W bazie `users` dodać kolumny `first_name`, `last_name`, `email`, `phone`, `role_display` (np. "IT Support Level 2").
- Popup w agencie pokazuje: avatar + imię+nazwisko + rola + organizacja.

---

## Faza 5: Agent Generator + Portal pobierania (FAZA 5)

**Cel:** Zaawansowany generator w konsoli web + publiczny portal pobierania.

### Generator (w panelu web)
- Rozbudować istniejący `generator.ejs`:
  - **Step 1:** Template (blank / organization preset / custom)
  - **Step 2:** Connection (server, relay, TLS, STUN/TURN)
  - **Step 3:** Access policy (supervised / unattended / hybrid)
  - **Step 4:** Branding (logo, kolor, nazwa firmy)
  - **Step 5:** Features (chat enabled?, file transfer?, terminal?, screenshots?)
  - **Step 6:** Security (API key, allowed operators, MAC pinning)
  - **Generate button** → tworzy podpisany JSON config + enrollment token
- Output: **installer .exe / .msi / .deb / .pkg / .dmg** z zaszytym configiem (build przez `cargo tauri build` z env var `AGENT_PRESET_CONFIG`).
- Templates per-org w bazie: `agent_templates` (name, description, config_json, created_by, org_id).

### Portal pobierania (oddzielny port — np. 5001)
- Nowa aplikacja Node.js `downloads-portal/` (lub route prefix w istniejącej konsoli).
- Publiczny dostęp (bez auth).
- Strona z brandingiem firmy: logo, nazwa, opis, "Pobierz Yomie Agent" × OS.
- Każdy link → pobiera najnowszy build z zaszytym preset configiem.
- Po instalacji agent auto-rejestruje się do serwera (z tokenem z config).

### Auto-enrollment flow
1. Agent startuje
2. Jeśli `preset_enrollment_token` w config → `POST /api/bd/enroll` z tokenem
3. Serwer weryfikuje token + tworzy device + zwraca JWT
4. Agent zapisuje device_id + JWT → normalny flow

---

## Kolejność realizacji

| Faza | Czas (szacunek) | Priorytet |
|------|----------------|-----------|
| 1. Fundament UX + powiadomienia | Ta sesja | 🔴 Krytyczne |
| 2. Device Modal + agent capabilities | 2-3 sesje | 🟠 Wysokie |
| 3. Chat E2E + attachments | 1-2 sesje | 🟠 Wysokie |
| 4. Remote Desktop | 4-6 sesji | 🔴 Krytyczne |
| 5. Generator + Portal | 1-2 sesje | 🟡 Średnie |

## Uwagi bezpieczeństwa (cross-cutting)

- **JWT token z rejestracji** — używany do wszystkich WS (agent side) + device API.
- **Rate-limit IP** — na wszystkich publicznych endpointach (istnieje w Go server).
- **Audit log** — każda akcja (login, connect, file transfer, command exec) do `audit_log` tabeli.
- **E2E chat** — już zaimplementowane (P-256 ECDH + AES-256-GCM) w `chatCrypto.js`.
- **Token rotation** — JWT krótki TTL (15 min) + refresh token.
- **Certificate pinning** — agent pinuje SHA256 serwera po pierwszej rejestracji.
- **Supervised mode consent** — user ma fizyczną kontrolę nad uprawnieniami per-sesja.
- **Admin-only actions w agencie** — ustawienia, unregister, zamknięcie — wymagają OS admin (lokalny).
