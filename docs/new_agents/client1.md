# Prompt: Yomie MGMT Client

## Cel
Na bazie istniejącego projektu `shamstabraiz/Yomie` utwórz nowy, wydzielony typ klienta o nazwie **Yomie MGMT Client** (klient administracyjno-operatorski), którego zadaniem jest zapewnienie administratorom i operatorom centrum wsparcia zdalnego pełnego, bezpiecznego i wysokowydajnego dostępu do urządzeń podłączonych do serwera Yomie.

Klient MGMT ma być aplikacją typu **standalone**, zoptymalizowaną pod kątem:
- maksymalnej jakości obrazu,
- niskich opóźnień,
- szerokiego wsparcia kodeków,
- stabilności połączeń,
- zgodności z różnymi systemami operacyjnymi,
- bezpieczeństwa operacyjnego i kryptograficznego,
- centralnego zarządzania infrastrukturą Yomie.

## Główna rola klienta MGMT
Klient MGMT to narzędzie dla administratorów, helpdesku, operatorów i personelu wsparcia technicznego. Ma umożliwiać:

1. zdalne łączenie się do klientów typu Agent,
2. wyświetlanie listy urządzeń zarejestrowanych nie tylko w Yomie, ale również – jeśli architektura projektu na to pozwala – urządzeń pochodzących z integracji z RustDesk,
3. zarządzanie serwerem Yomie z poziomu aplikacji,
4. prowadzenie czatu z użytkownikami i agentami podłączonymi do serwera,
5. odbieranie powiadomień o prośbach o pomoc,
6. nadzorowanie i obsługę scenariuszy CDAP (narzędzie operatora do zdalnej pomocy i zarządzania),
7. zapewnienie maksymalnej jakości i płynności transmisji obrazu dzięki pełnemu wykorzystaniu dostępnych technologii projektu.

## Wymagania architektoniczne
Przeanalizuj aktualną architekturę repozytorium `Yomie` i zaprojektuj klienta MGMT tak, aby:
- wykorzystywał maksymalnie istniejące komponenty i protokoły projektu,
- nie dublował logiki już obecnej w backendzie i usługach serwera,
- wydzielał warstwę GUI od warstwy komunikacyjnej,
- wspierał modularność,
- umożliwiał dalszą rozbudowę o dodatkowe kodeki, integracje i mechanizmy polityk bezpieczeństwa.

Zaproponuj architekturę, która może wykorzystywać:
- frontend oparty o obecne zasoby JavaScript/HTML/CSS,
- wydajny backend/bridge lokalny w Go tam, gdzie wymagana jest wydajność, natywna integracja systemowa, streaming, zarządzanie procesami i funkcje bezpieczeństwa,
- skrypty Shell/PowerShell do instalacji, detekcji środowiska i działań administracyjnych per system operacyjny.

## Kluczowe funkcje klienta MGMT

### 1. Panel urządzeń
Klient MGMT ma prezentować zunifikowaną listę urządzeń:
- online/offline,
- typ urządzenia,
- system operacyjny,
- wersja klienta,
- nazwa hosta,
- adresy sieciowe,
- status bezpieczeństwa,
- ostatnie połączenie,
- tagi, grupy, lokalizacje, właściciel urządzenia,
- źródło urządzenia: Yomie / integracja RustDesk / inne przyszłe źródła.

Lista ma wspierać:
- wyszukiwanie,
- filtrowanie,
- sortowanie,
- grupowanie,
- widoki administracyjne,
- szybkie akcje masowe.

### 2. Zdalna sesja premium
Połączenie z klientem Agent ma oferować:
- maksymalną jakość obrazu,
- adaptacyjny bitrate,
- dynamiczne dopasowanie rozdzielczości,
- szerokie wsparcie kodeków zależnie od możliwości systemu i sprzętu,
- preferowanie sprzętowej akceleracji, jeśli jest dostępna,
- automatyczny fallback do bezpiecznych i kompatybilnych trybów software,
- przesyłanie dźwięku, jeśli projekt to umożliwia,
- tryb wielu monitorów,
- wybór monitora,
- kopiuj/wklej, schowek,
- transfer plików,
- opcjonalny zdalny terminal / shell administracyjny, jeśli architektura to wspiera,
- nagrywanie sesji zgodnie z polityką uprawnień i audytu,
- mechanizmy rekonfiguracji jakości w trakcie sesji.

### 3. Czat i komunikacja
Klient MGMT ma umożliwiać:
- czat 1:1 z użytkownikiem końcowym,
- czat operator ↔ agent,
- czat grupowy dla kontekstu incydentu lub zgłoszenia,
- gotowe odpowiedzi,
- historię konwersacji,
- powiadomienia push/in-app,
- eskalację zgłoszeń,
- oznaczanie sesji jako wymagającej pilnej pomocy.

### 4. Zarządzanie serwerem Yomie
Klient MGMT ma zawierać panel administracyjny pozwalający – zależnie od uprawnień – na:
- przegląd stanu serwera,
- przegląd podłączonych klientów,
- zarządzanie operatorami i rolami,
- konfigurację polityk bezpieczeństwa,
- podgląd logów i zdarzeń,
- zarządzanie kluczami, tokenami i rejestracją klientów,
- zarządzanie integracjami,
- zarządzanie kolejką powiadomień o pomocy,
- zarządzanie grupami urządzeń i regułami przypisania.

### 5. Tryb CDAP / operator
Klient MGMT ma być głównym narzędziem operatora. Powinien:
- obsługiwać wiele równoczesnych sesji,
- oferować kolejkę zgłoszeń,
- priorytety incydentów,
- dashboard aktywnych połączeń,
- monitoring jakości sesji,
- wskaźniki wykorzystania pasma, kodeka, FPS, opóźnienia,
- podgląd alertów i problemów z agentami,
- możliwość przejmowania, przekazywania i kończenia sesji zgodnie z uprawnieniami.

## Bezpieczeństwo — wymagania krytyczne
To jest priorytet najwyższy. Implementacja ma być projektowana zgodnie z zasadą secure-by-design.

Wymagania:
- silna wzajemna autoryzacja klient ↔ serwer,
- TLS/mTLS tam, gdzie możliwe,
- rotacja tokenów sesyjnych,
- krótko żyjące tokeny dostępu,
- role i uprawnienia RBAC,
- opcjonalnie wsparcie MFA dla operatorów,
- pinning certyfikatów lub bezpieczny trust model,
- pełen audyt zdarzeń administracyjnych,
- podpisywanie lub weryfikacja integralności aktualizacji,
- minimalizacja powierzchni ataku,
- bezpieczne przechowywanie sekretów lokalnie z użyciem systemowych magazynów poświadczeń jeśli możliwe,
- ograniczenie wykonywania działań wysokiego ryzyka,
- jawna zgoda użytkownika tam, gdzie polityka firmy lub tryb pracy tego wymaga,
- rozdzielenie funkcji operatora i superadministratora,
- ścisła walidacja komunikatów przychodzących,
- odporność na replay attack, downgrade attack i nieautoryzowane przejęcie sesji.

## Kompatybilność systemowa
Klient MGMT musi być zaprojektowany z myślą o:
- Windows,
- Linux,
- macOS.

Uwzględnij:
- różnice w przechwytywaniu obrazu,
- różnice w modelu uprawnień,
- różnice w przechowywaniu sekretów,
- różnice w instalacji i aktualizacji,
- różnice w akceleracji sprzętowej i dostępności kodeków,
- różnice w działaniu usług/system tray/notyfikacji.

## UX/UI
Interfejs ma być:
- nowoczesny,
- czytelny,
- szybki,
- zoptymalizowany dla operatora pracującego pod obciążeniem,
- wspierać motyw ciemny i jasny,
- mieć priorytet na ergonomię, gęstość informacji i szybkość działania.

Widoki minimalne:
- logowanie,
- dashboard,
- lista urządzeń,
- szczegóły urządzenia,
- okno zdalnej sesji,
- panel czatu,
- centrum powiadomień,
- panel zarządzania serwerem,
- panel ustawień,
- panel audytu/logów.

## Integracja z RustDesk
Jeśli repozytorium Yomie technicznie umożliwia integrację z RustDesk, zaprojektuj warstwę integracyjną tak, aby:
- nie łamać obecnej architektury,
- oddzielać źródła urządzeń logicznie,
- normalizować metadane urządzeń,
- zachować spójny model uprawnień,
- zapewniać bezpieczne mapowanie sesji, statusów i identyfikatorów.

Jeśli pełna integracja nie jest możliwa bez dużych zmian, zaproponuj:
- etapowy plan integracji,
- warstwę kompatybilności,
- adapter źródeł urządzeń.

## Wymagania implementacyjne
Na bazie istniejącego projektu:
1. przeanalizuj strukturę repozytorium i wskaż, które moduły można ponownie wykorzystać,
2. zaproponuj docelową strukturę kodu klienta MGMT,
3. wskaż, które części napisać w Go, a które w JavaScript,
4. uwzględnij budowanie aplikacji wieloplatformowej,
5. uwzględnij instalatory i aktualizacje,
6. zaproponuj warstwę konfiguracji i polityk,
7. zaproponuj testy:
   - jednostkowe,
   - integracyjne,
   - e2e,
   - testy bezpieczeństwa,
   - testy wydajności streamingu,
   - testy kompatybilności systemowej.

## Oczekiwany rezultat
Wygeneruj kompletny plan i implementację klienta **Yomie MGMT Client**, obejmującą:
- architekturę,
- strukturę katalogów,
- komponenty GUI,
- warstwę komunikacji,
- obsługę zdalnych sesji,
- panel serwera,
- czat,
- powiadomienia,
- model bezpieczeństwa,
- integrację z istniejącym backendem Yomie,
- plan integracji z RustDesk,
- strategię wdrożenia cross-platform.

Jeśli w repozytorium znajdują się ograniczenia techniczne, nie ignoruj ich — opisz je i zaproponuj najlepsze możliwe obejście bez psucia bezpieczeństwa i zgodności architektury.