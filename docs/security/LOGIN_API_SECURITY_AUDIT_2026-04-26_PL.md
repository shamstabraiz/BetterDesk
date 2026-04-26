# Audyt bezpieczeństwa logowania i API

Data: 2026-04-26
Zakres: przepływy logowania BetterDesk, obsługa sesji, obsługa tokenów oraz autoryzacja HTTP/WebSocket API w konsoli Node.js i serwerze Go
Metoda: audyt kodu źródłowego bieżącej gałęzi main, skoncentrowany na uwierzytelnianiu, autoryzacji, rate limiting, cyklu życia tokenów oraz granicach zaufania między przeglądarką i API
Status: łatki 1–4 zastosowane 2026-04-26 (zerowe lub minimalne ryzyko kompatybilności). Łatki 5–6 są odroczone i śledzone osobno, ponieważ wymagają etapowego wdrożenia, aby nie zepsuć działających instalacji.

### Status remediacji (2026-04-26)

| # | Ustalenie | Waga | Status | Uwagi |
|---|---|---|---|---|
| 1 | Obejście brute-force lockout (brakujące `await`) | Wysoka | **Naprawione** | `web-nodejs/routes/rustdesk-api.routes.js` — dodano `await` przed `authService.checkBruteForce`. Brak wpływu na schemat, role i tokeny. |
| 2 | Zakończenie 2FA bez regeneracji sesji | Średnia | **Naprawione** | `web-nodejs/routes/auth.routes.js` — zapisy do sesji po TOTP są teraz w `req.session.regenerate(...)`, identycznie jak w standardowym logowaniu. Nazwa cookie nie zmieniona; istniejące sesje działają dalej. |
| 3 | Endpointy audit / WS-events bez wyraźnego RBAC | Średnia | **Naprawione** | `betterdesk-server/api/server.go` — `GET /api/audit/events` i `GET /api/ws/events` opakowane w `requirePermission(auth.PermAuditView, ...)`. Wszystkie role wbudowane, które wcześniej korzystały z tych endpointów (super_admin, admin, server_admin, global_admin, operator, viewer) już posiadają `audit.view`; dostęp traci wyłącznie rola `pro` — patrz CHANGELOG. |
| 4 | Wyciek surowych `err.Error()` z handlerów Go | Średnia | **Naprawione** | `betterdesk-server/api/auth_handlers.go` — dziewięć ścieżek 500 zwraca teraz `{"error":"internal error"}` i loguje pełny szczegół po stronie serwera przez `log.Printf`. Kody statusów niezmienione; pozostałe ścieżki bez zmian. |
| 5 | Plaintext tokeny dostępu w `access_tokens` | Średnia | **Odroczone** | Wymaga 3-fazowego wdrożenia, żeby nie wymusić ponownego logowania na każdym aktywnym kliencie RustDesk. Patrz sekcja *Odroczone łatki* poniżej. |
| 6 | CSP nadal z `'unsafe-inline'` (script attrs) i `'unsafe-eval'` (remote viewer) | Niska | **Odroczone** | Usunięcie tych flag wymaga refaktoringu inline’owych handlerów EJS i podmiany `eval`-owej generacji kodu protobuf.js. Patrz sekcja *Odroczone łatki* poniżej. |

## Streszczenie wykonawcze

BetterDesk ma solidną bazę bezpieczeństwa jak na wieloskładnikową platformę do zdalnego zarządzania. Projekt zawiera już uwierzytelnianie sesyjne dla panelu web, TOTP 2FA, ochronę CSRF, rate limiting, RBAC, klucze API, JWT oraz rozsądną liczbę testów regresyjnych.

Główna słabość nie polega na całkowitym braku zabezpieczeń, ale na niespójności między kilkoma ścieżkami uwierzytelniania:

- logowaniem do panelu web
- API kompatybilnym z RustDesk wystawionym przez konsolę Node.js
- warstwą administracyjną i API w serwerze Go

Najmocniejszą ścieżką jest obecnie standardowy przepływ logowania do panelu web. Najwyższe ryzyka koncentrują się wokół Node.js RustDesk API oraz niespójnego wymuszania uprawnień RBAC po stronie Go API.

Ocena ogólna: dojrzałość na poziomie średnio-wysokim, z kilkoma konkretnymi problemami, które warto usunąć zanim warstwa logowania i API będzie traktowana jako w pełni utwardzona.

## Zakres przeglądu

Audyt objął następujące obszary:

- trasy uwierzytelniania Node.js w [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js)
- API kompatybilne z RustDesk po stronie Node.js w [web-nodejs/routes/rustdesk-api.routes.js](../../web-nodejs/routes/rustdesk-api.routes.js)
- usługę auth i obsługę tokenów po stronie Node.js w [web-nodejs/services/authService.js](../../web-nodejs/services/authService.js)
- persystencję bazy i tokenów w [web-nodejs/services/database.js](../../web-nodejs/services/database.js) i [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js)
- middleware bezpieczeństwa Node.js w [web-nodejs/middleware/auth.js](../../web-nodejs/middleware/auth.js), [web-nodejs/middleware/csrf.js](../../web-nodejs/middleware/csrf.js), [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js) oraz [web-nodejs/middleware/rateLimiter.js](../../web-nodejs/middleware/rateLimiter.js)
- auth i routing serwera Go w [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go), [betterdesk-server/api/server.go](../../betterdesk-server/api/server.go), [betterdesk-server/api/client_api_handlers.go](../../betterdesk-server/api/client_api_handlers.go), [betterdesk-server/api/token_handlers.go](../../betterdesk-server/api/token_handlers.go)
- prymitywy auth po stronie Go w [betterdesk-server/auth/jwt.go](../../betterdesk-server/auth/jwt.go), [betterdesk-server/auth/password.go](../../betterdesk-server/auth/password.go) i [betterdesk-server/auth/totp.go](../../betterdesk-server/auth/totp.go)
- testy powiązane w [web-nodejs/tests/auth.routes.test.js](../../web-nodejs/tests/auth.routes.test.js), [web-nodejs/tests/middleware.auth.test.js](../../web-nodejs/tests/middleware.auth.test.js), [web-nodejs/tests/security.middleware.test.js](../../web-nodejs/tests/security.middleware.test.js), [betterdesk-server/auth/password_test.go](../../betterdesk-server/auth/password_test.go), [betterdesk-server/auth/jwt_test.go](../../betterdesk-server/auth/jwt_test.go) i [betterdesk-server/auth/totp_test.go](../../betterdesk-server/auth/totp_test.go)

## Mocne strony

### 1. Dobra bazowa ochrona sesji panelu web

Standardowy przepływ logowania do panelu poprawnie regeneruje sesję po udanym uwierzytelnieniu w [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L106). To właściwa obrona przed session fixation w głównej ścieżce panelu.

### 2. CSRF jest zaimplementowane prawidłowo dla ruchu przeglądarkowego

Panel korzysta ze wzorca double-submit cookie przez csrf-csrf w [web-nodejs/middleware/csrf.js](../../web-nodejs/middleware/csrf.js#L24), co jest wyraźnie lepsze od własnej, prowizorycznej logiki tokenów formularzy.

### 3. Ograniczenie user enumeration po stronie Node auth istnieje

Usługa auth po stronie Node używa przygotowanego wcześniej dummy bcrypt hash w [web-nodejs/services/authService.js](../../web-nodejs/services/authService.js#L18), więc brak użytkownika nadal kosztuje realistyczne sprawdzenie hasła.

### 4. Ograniczanie długości wejścia istnieje już na brzegu logowania web

Panel odrzuca zbyt długie username i password w [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L61), co zmniejsza wpływ ataków kosztowych na bcrypt.

### 5. Go API ma rate limiting dla logowania i 2FA

Handlery login oraz login/2fa po stronie Go wymuszają per-IP rate limiting w [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L177) i [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L250).

### 6. Częściowe tokeny 2FA po stronie Go są krótkotrwałe

Go API poprawnie wydaje 5-minutowy częściowy token dla kroku 2FA w [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L213). To dobry domyślny wybór w porównaniu z długowiecznymi tokenami pośrednimi.

### 7. Transport kluczy API jest lepszy niż w starszym modelu

Go API akceptuje klucze API wyłącznie z nagłówka X-API-Key, a nie z query string, zgodnie z [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L801). To zmniejsza ryzyko wycieku do logów, cache i reverse proxy.

### 8. Heartbeat i sysinfo po stronie Go są realnie limitowane

Serwer definiuje i wykorzystuje osobny limiter dla heartbeat/sysinfo w [betterdesk-server/api/server.go](../../betterdesk-server/api/server.go#L76), [betterdesk-server/api/client_api_handlers.go](../../betterdesk-server/api/client_api_handlers.go#L563), [betterdesk-server/api/client_api_handlers.go](../../betterdesk-server/api/client_api_handlers.go#L631) oraz [betterdesk-server/api/client_api_handlers.go](../../betterdesk-server/api/client_api_handlers.go#L708).

## Podsumowanie ustaleń

| Waga | Obszar | Status | Skrót |
|---|---|---|---|
| Wysoka | Node RustDesk API | **Naprawione (2026-04-26)** | Ochrona brute-force jest obchodzona, bo async lockout check jest wywołany bez await |
| Średnia | Panel web Node | **Naprawione (2026-04-26)** | Ścieżka zakończenia 2FA ustawia zalogowaną sesję bez jej regeneracji |
| Średnia | RBAC Go API | **Naprawione (2026-04-26)** | Endpointy audit/events nie są opakowane dedykowaną permisją audit.view |
| Średnia | Obsługa błędów Go API | **Naprawione (2026-04-26)** | Część handlerów auth/admin zwraca klientowi surowe err.Error |
| Średnia | Magazyn tokenów Node | **Odroczone** | Tokeny dostępu klientów RustDesk są przechowywane w bazie w plaintext |
| Niska | CSP Node | **Odroczone** | Konsola nadal ma wyjątki kompatybilnościowe takie jak unsafe-inline dla atrybutów skryptowych i unsafe-eval dla remote viewer |

## Szczegółowe ustalenia

### Ustalenie 1: brute-force lockout jest omijany w Node RustDesk API

Waga: Wysoka

Dowody:

- [web-nodejs/routes/rustdesk-api.routes.js](../../web-nodejs/routes/rustdesk-api.routes.js#L828)
- [web-nodejs/services/authService.js](../../web-nodejs/services/authService.js#L705)

Opis:

Trasa logowania kompatybilna z RustDesk w konsoli Node.js wywołuje authService.checkBruteForce bez await:

- zapisuje Promise do bruteCheck
- natychmiast czyta bruteCheck.blocked

Ponieważ checkBruteForce jest funkcją asynchroniczną, wynik lockoutu konta lub IP nie jest faktycznie oczekiwany. W efekcie trasa może przejść dalej do uwierzytelnienia, mimo że logika blokady powinna zatrzymać żądanie.

Wpływ:

- blokady kont i ograniczenia per-IP mogą być cicho omijane na rustdeskowej ścieżce logowania Node
- projekt ma w praktyce słabszą ochronę na ścieżce WAN/API niż na standardowej ścieżce panelu przeglądarkowego
- niespójność podważa założenie, że współdzielone zabezpieczenia authService działają jednakowo w całym produkcie

Rekomendacja:

- zmienić trasę tak, aby używała await authService.checkBruteForce(username, ip)
- dodać test regresyjny obejmujący blokadę konta i blokadę IP specjalnie dla logowania RustDesk API

### Ustalenie 2: zakończenie 2FA w panelu web nie regeneruje sesji

Waga: Średnia

Dowody:

- regeneracja sesji podczas zwykłego loginu: [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L106)
- przepływ zakończenia TOTP: [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L268) i [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L272)

Opis:

Standardowa ścieżka logowania web poprawnie regeneruje sesję po udanym loginie username/password. Jednak gdy wymagane jest TOTP, końcowa ścieżka weryfikacji jedynie usuwa pola tymczasowe i wpisuje zalogowanego użytkownika do istniejącego obiektu sesji.

Wpływ:

- ochrona przed session fixation jest pełna tylko dla logowań bez 2FA
- konta z włączonym 2FA mogą odziedziczyć istniejący identyfikator sesji przez drugi etap logowania
- osłabia to jedną z najważniejszych ochron dla kont uprzywilejowanych

Rekomendacja:

- po poprawnej weryfikacji TOTP ponownie zregenerować sesję przed ustawieniem req.session.userId i req.session.user
- dodać test regresyjny weryfikujący, że ID sesji zmienia się także po kroku kończącym 2FA

### Ustalenie 3: endpointy audit/events po stronie Go nie są chronione dedykowaną permisją audit.view

Waga: Średnia

Dowody:

- permisja audit istnieje: [betterdesk-server/auth/permissions.go](../../betterdesk-server/auth/permissions.go#L36)
- rejestracja trasy audytu: [betterdesk-server/api/server.go](../../betterdesk-server/api/server.go#L201)
- rejestracja websocketów eventów: [betterdesk-server/api/server.go](../../betterdesk-server/api/server.go#L204)
- handler audytu: [betterdesk-server/api/server.go](../../betterdesk-server/api/server.go#L1190)
- typy eventów rozgłaszanych przez event bus: [betterdesk-server/events/bus.go](../../betterdesk-server/events/bus.go)

Opis:

Serwer Go definiuje dedykowaną permisję audit.view, ale główny endpoint zdarzeń audytu oraz websocket streamujący eventy nie są opakowane requirePermission(auth.PermAuditView, ...). Chroni je tylko ogólne middleware auth.

Wpływ:

- uwierzytelnieni użytkownicy mogą uzyskać wgląd w operacyjne lub bezpieczeństwowe telemetry, które powinny być zarezerwowane dla ról audit-capable
- osłabia to zamierzony model RBAC i utrudnia jednoznaczne określenie, kto powinien widzieć audit

Rekomendacja:

- opakować GET /api/audit/events przez requirePermission(auth.PermAuditView, ...)
- zdecydować, czy GET /api/ws/events powinien wymagać audit.view, osobnej permisji event-stream, czy filtrowania zależnego od roli
- dodać testy tras Go sprawdzające odmowę dla ról o niższych uprawnieniach

### Ustalenie 4: handlery auth/admin po stronie Go ujawniają wewnętrzne błędy

Waga: Średnia

Dowody:

- [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L336)
- [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L498)
- [betterdesk-server/api/auth_handlers.go](../../betterdesk-server/api/auth_handlers.go#L743)

Opis:

Część handlerów zwraca err.Error bezpośrednio w odpowiedziach 500. To wzorzec information disclosure: wewnętrzne błędy storage lub walidacji stają się widoczne dla każdego już uwierzytelnionego klienta, który może wywołać te trasy.

Wpływ:

- szczegóły warstwy bazy danych, naruszenia unikalności i błędy runtime mogą wyciekać do klientów
- rozpoznanie po przejęciu konta lub klucza API staje się łatwiejsze

Rekomendacja:

- zastąpić surowe err.Error komunikatami ogólnymi typu internal error lub operation failed
- logować prawdziwy błąd po stronie serwera z wystarczającym kontekstem dla operatorów

### Ustalenie 5: tokeny dostępu RustDesk w Node są przechowywane w plaintext

Waga: Średnia

Dowody:

- deklaracja schematu: [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L253) i [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L255)
- tworzenie i lookup tokenów: [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L1147) i [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L1154)

Opis:

Tokeny dostępu klientów RustDesk są generowane z dobrą entropią, ale są zapisywane bezpośrednio w bazie zamiast jako hash jednokierunkowy. Sam odczyt bazy pozwala więc odtworzyć i ponownie użyć aktywne tokeny aż do czasu wygaśnięcia lub unieważnienia.

Wpływ:

- wyciek bazy oznacza wyciek aktywnych sesji
- backupy, snapshoty i przypadkowe ekspozycje bazy mają większy blast radius niż to konieczne

Rekomendacja:

- przechowywać tylko hash tokenu oparty na SHA-256 lub HMAC
- porównywać hash przy lookupie
- plaintext token zachowywać wyłącznie w pamięci podczas jego wydawania klientowi

### Ustalenie 6: CSP nadal zawiera wyjątki kompatybilnościowe

Waga: Niska

Dowody:

- wyjątek unsafe-eval dla remote viewer: [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js#L27)
- wyjątek inline dla atrybutów skryptowych: [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js#L37)

Opis:

CSP konsoli jest lepsze niż w typowej aplikacji Express, ale nadal zawiera wyjątki kompatybilnościowe dla wybranych ścieżek UI. To nie jest dowód aktywnego XSS, ale osłabia defense-in-depth.

Wpływ:

- ewentualny przyszły XSS na dotkniętych stronach będzie łatwiejszy do uzbrojenia
- szczególnie strona remote viewer ma węższy margines bezpieczeństwa niż reszta panelu

Rekomendacja:

- dalej usuwać inline handlery z szablonów i renderowanego HTML po stronie klienta
- wymienić ścieżkę zależności protobuf/runtime, która obecnie wymusza unsafe-eval w remote viewer

## Obserwacja architektoniczna

Projekt utrzymuje obecnie równolegle kilka modeli uwierzytelniania:

- auth sesyjny dla konsoli web
- nieprzezroczyste tokeny trzymane w bazie dla Node RustDesk API
- JWT i klucze API w serwerze Go

Samo w sobie nie jest to błędem, ale wyraźnie zwiększa ryzyko dryfu. Najważniejszy motyw tego audytu to spójność: największe problemy wynikają z tego, że jedna ścieżka jest bezpieczniejsza od drugiej, mimo że obie należą do tej samej granicy zaufania produktu.

## Ocena pokrycia testami

Pozytywne sygnały:

- istnieją dedykowane testy dla auth i middleware panelu w [web-nodejs/tests/auth.routes.test.js](../../web-nodejs/tests/auth.routes.test.js), [web-nodejs/tests/middleware.auth.test.js](../../web-nodejs/tests/middleware.auth.test.js) oraz [web-nodejs/tests/security.middleware.test.js](../../web-nodejs/tests/security.middleware.test.js)
- istnieją testy prymitywów auth po stronie Go w [betterdesk-server/auth/password_test.go](../../betterdesk-server/auth/password_test.go), [betterdesk-server/auth/jwt_test.go](../../betterdesk-server/auth/jwt_test.go) oraz [betterdesk-server/auth/totp_test.go](../../betterdesk-server/auth/totp_test.go)

Luki w pokryciu ujawnione przez ten audyt:

- nie widać testu regresyjnego obejmującego lockout brute-force dla logowania RustDesk API po stronie Node
- nie widać testu wymuszającego regenerację sesji w kroku kończącym TOTP w Node
- nie widać testu tras Go wymuszającego audit.view dla audit/events, ponieważ wrappera obecnie brak
- nie ma testu pilnującego hashed-at-rest dla tokenów RustDesk po stronie Node, bo tokeny nadal są przechowywane w plaintext

## Zalecana kolejność napraw

### Priorytet 1

- naprawić brak await w brute-force check dla Node RustDesk API
- regenerować sesję po poprawnej weryfikacji TOTP w panelu web
- domknąć RBAC dla Go audit/events przez jawne requirePermission

### Priorytet 2

- usunąć wycieki raw err.Error z auth/admin handlerów po stronie Go
- haszować tokeny dostępu RustDesk po stronie Node przed zapisem do bazy

### Priorytet 3

- dalej zaostrzać wyjątki CSP w konsoli i remote viewer
- rozważyć długofalowe ujednolicenie modeli auth między panelem Node, RustDesk API Node i Go API

## Wniosek końcowy

Warstwa logowania i API BetterDesk jest wyraźnie dojrzalsza niż w przeciętnym autorskim stosie do zdalnego zarządzania. Projekt używa wielu właściwych prymitywów i wzorców. Najważniejsza dalsza praca polega jednak na usunięciu niespójności bezpieczeństwa między różnymi ścieżkami dostępu.

Po naprawieniu elementów z Priorytetu 1 ogólny poziom ryzyka tej warstwy spadnie wyraźnie bez potrzeby przebudowy całej architektury.

## Odroczone łatki

Poniższe dwa ustalenia świadomie **nie** są naprawione w tej serii, ponieważ naiwna łatka popsułaby istniejące instalacje BetterDesk. Są udokumentowane tutaj, żeby można je było wdrożyć później jako osobne, dobrze przetestowane rollouty.

### Łatka 5 — hashowanie tokenów dostępu RustDesk w bazie (Średnia)

Dlaczego odroczone: każdy aktywny dziś klient RustDesk trzyma token, którego plaintextowa wartość leży w [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L1140-L1160). Podmiana kolumny na hash w jednym commicie unieważni każdą żywą sesję klienta.

Wymagany rollout:

1. **Faza 1 (additive, kompatybilna wstecz):** dodać kolumnę `token_hash TEXT` do `access_tokens`. Przy wystawianiu tokenu zapisywać zarówno `token` (plaintext, legacy), jak i `token_hash`. Przy lookup sprawdzać najpierw `token_hash`, fallbackiem `token`.
2. **Faza 2 (drain):** odczekać jeden TTL tokenu (domyślnie 30 dni), żeby wszystkie żywe tokeny zostały wystawione już w trybie dual-write. Logując warning za każdym razem, gdy lookup wpada na legacy plaintext.
3. **Faza 3 (enforce):** usunąć kolumnę `token`, usunąć fallback w lookupie i wymusić `token_hash NOT NULL`.

Do czasu ukończenia Fazy 3 traktować ograniczenie uprawnień systemu plików na bazie auth jako środek kompensacyjny.

### Łatka 6 — usunięcie `'unsafe-inline'` (script attrs) i `'unsafe-eval'` z CSP (Niska)

Dlaczego odroczone: usunięcie tych flag to nie zmiana konfiguracji — to refaktor.

Wymagana praca:

1. Przejrzeć każdy szablon EJS w `web-nodejs/views/` pod kątem inline’owych `onclick=`, `onsubmit=`, `onload=` itp. i przenieść obsługę do delegowanych listenerów w modułach JS.
2. Zastąpić ścieżkę runtime `protobuf.js` używaną w remote viewer (która wymaga `eval`) wariantem precompiled / static module.
3. Usunąć wpis `'unsafe-inline'` z `scriptSrcAttr` oraz warunkowy blok `'unsafe-eval'` z [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js#L15-L45).
4. Dodać test E2E, który ładuje panel i remote viewer ze ścisłym CSP i sprawdza brak naruszeń w konsoli.

To jest wielo-PR-owe przedsięwzięcie i jest śledzone osobno w backlogu.