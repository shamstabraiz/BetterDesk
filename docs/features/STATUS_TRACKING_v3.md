# Yomie Server v3.0 - Ulepszony System Statusu

## Przegląd Zmian

Wersja 3.0 wprowadza znacząco ulepszony system śledzenia statusu urządzeń:

### Kluczowe Ulepszenia

| Cecha | v2.0 | v3.0 |
|-------|------|------|
| Timeout wykrywania offline | Stały 30s | **Konfigurowalny (domyślnie 15s)** |
| Interwał heartbeat | Stały 5s | **Konfigurowalny (domyślnie 3s)** |
| Statusy urządzeń | Online/Offline | **Online/Degraded/Critical/Offline** |
| Statystyki | Brak | **Pełne statystyki z API** |
| Konfiguracja runtime | Brak | **Przez API i zmienne środowiskowe** |

---

## Nowe Statusy Urządzeń

```
┌─────────────────────────────────────────────────────────┐
│  ONLINE    │ Wszystko OK, heartbeat otrzymany          │
├─────────────────────────────────────────────────────────┤
│  DEGRADED  │ 2-3 pominięte heartbeaty                  │
├─────────────────────────────────────────────────────────┤
│  CRITICAL  │ 4+ pominięte heartbeaty, wkrótce offline  │
├─────────────────────────────────────────────────────────┤
│  OFFLINE   │ Przekroczony timeout, brak połączenia     │
└─────────────────────────────────────────────────────────┘
```

---

## Konfiguracja

### Zmienne Środowiskowe

| Zmienna | Domyślnie | Opis |
|---------|-----------|------|
| `PEER_TIMEOUT_SECS` | 15 | Sekundy do uznania za offline |
| `HEARTBEAT_INTERVAL_SECS` | 3 | Interwał sprawdzania statusu |
| `HEARTBEAT_WARNING_THRESHOLD` | 2 | Ilość pominiętych HB dla DEGRADED |
| `HEARTBEAT_CRITICAL_THRESHOLD` | 4 | Ilość pominiętych HB dla CRITICAL |
| `HEARTBEAT_VERBOSE` | false | Szczegółowe logowanie |
| `DB_SYNC_INTERVAL_SECS` | 5 | Interwał synchronizacji z DB |

### Konfiguracja przez Systemd

```ini
# /etc/systemd/system/rustdesksignal.service
[Service]
Environment="PEER_TIMEOUT_SECS=10"
Environment="HEARTBEAT_INTERVAL_SECS=2"
Environment="HEARTBEAT_VERBOSE=true"
```

### Konfiguracja przez Docker

```yaml
services:
  hbbs:
    environment:
      - PEER_TIMEOUT_SECS=10
      - HEARTBEAT_INTERVAL_SECS=2
```

---

## Nowe Endpointy API

### GET /api/config
Pobiera aktualną konfigurację serwera.

**Response:**
```json
{
  "success": true,
  "data": {
    "peer_timeout_secs": 15,
    "heartbeat_interval_secs": 3,
    "warning_threshold": 2,
    "critical_threshold": 4,
    "verbose_logging": false,
    "db_sync_interval_secs": 5
  },
  "version": "3.0.0"
}
```

### POST /api/config
Aktualizuje konfigurację (zapisuje do bazy danych).

**Request:**
```json
{
  "key": "peer_timeout_secs",
  "value": "10"
}
```

**Response:**
```json
{
  "success": true,
  "data": true,
  "version": "3.0.0"
}
```

### GET /api/peers/stats
Zwraca szczegółowe statystyki urządzeń.

**Response:**
```json
{
  "success": true,
  "data": {
    "total_peers": 150,
    "online_peers": 87,
    "offline_peers": 63,
    "banned_peers": 2,
    "total_heartbeats": 1548762
  },
  "version": "3.0.0"
}
```

### GET /api/server/stats
Statystyki serwera.

**Response:**
```json
{
  "success": true,
  "data": {
    "uptime_seconds": 86400,
    "memory_usage_mb": 45.2,
    "api_version": "3.0.0",
    "db_version": "3.0"
  },
  "version": "3.0.0"
}
```

---

## Nowe Kolumny Bazy Danych

Migracja automatycznie dodaje:

| Kolumna | Typ | Opis |
|---------|-----|------|
| `last_heartbeat` | DATETIME | Czas ostatniego heartbeatu |
| `heartbeat_count` | INTEGER | Licznik heartbeatów |
| `previous_ids` | TEXT (JSON) | Historia poprzednich ID |
| `id_changed_at` | DATETIME | Data zmiany ID |

### Tabela server_config

Nowa tabela dla konfiguracji runtime:

```sql
CREATE TABLE server_config (
    key VARCHAR(100) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME
);
```

---

## Rekomendowane Ustawienia

### Środowisko Produkcyjne (stabilność)
```bash
PEER_TIMEOUT_SECS=15
HEARTBEAT_INTERVAL_SECS=3
```

### Środowisko Krytyczne (szybka reakcja)
```bash
PEER_TIMEOUT_SECS=8
HEARTBEAT_INTERVAL_SECS=2
```

### Środowisko z Wolnymi Łączami
```bash
PEER_TIMEOUT_SECS=30
HEARTBEAT_INTERVAL_SECS=5
```

---

## Migracja z v2.0

1. **Backup bazy danych:**
   ```bash
   cp /opt/rustdesk/db_v2.sqlite3 /opt/rustdesk/db_v2.sqlite3.backup
   ```

2. **Wymiana binarek:**
   ```bash
   sudo ./install-improved.sh --fix
   ```

3. **Restart usług:**
   ```bash
   sudo systemctl restart rustdesksignal rustdeskrelay
   ```

Migracja schematu bazy danych wykonuje się automatycznie przy pierwszym uruchomieniu.

---

## Pliki Źródłowe v3.0

| Plik | Opis |
|------|------|
| `peer_v3.rs` | Ulepszony system statusu peer |
| `database_v3.rs` | Rozszerzona obsługa bazy danych |
| `http_api_v3.rs` | Nowe endpointy API |

### Użycie w Kompilacji

Aby użyć nowych plików, zamień oryginalne:

```bash
cp hbbs-patch-v2/src/peer_v3.rs hbbs-patch-v2/src/peer.rs
cp hbbs-patch-v2/src/database_v3.rs hbbs-patch-v2/src/database.rs
cp hbbs-patch-v2/src/http_api_v3.rs hbbs-patch-v2/src/http_api.rs
```

Następnie skompiluj:
```bash
./build-yomie.sh --auto
```

---

## Znane Ograniczenia

1. **Zmiany konfiguracji przez API** wymagają restartu dla niektórych parametrów
2. **WebSocket real-time push** nie jest jeszcze zaimplementowany (planowane w v3.1)
3. **Zmiana ID urządzeń** jest przygotowana w bazie danych ale wymaga zmian w kliencie RustDesk

---

## Changelog

### v3.0.0 (2026-02-06)
- Konfigurowalny timeout offline
- Konfigurowalny interwał heartbeat  
- Statusy pośrednie (DEGRADED, CRITICAL)
- Nowe endpointy API (/api/config, /api/peers/stats, /api/server/stats)
- Automatyczne migracje bazy danych
- Tabela server_config dla runtime configuration
- Batch operations dla lepszej wydajności
